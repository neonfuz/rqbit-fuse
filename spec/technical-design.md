# Technical Design Document

## Data Structures

### Torrent

```rust
#[derive(Debug, Clone)]
pub struct Torrent {
    pub id: u64,
    pub info_hash: String,
    pub name: String,
    pub output_folder: PathBuf,
    pub file_count: usize,
    pub files: Vec<TorrentFile>,
    pub piece_length: u64,
}
```

### TorrentFile

```rust
#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub name: String,
    pub length: u64,
    pub components: Vec<String>,
    pub file_idx: usize,  // Index within torrent
}
```

### InodeEntry

```rust
#[derive(Debug)]
pub enum InodeEntry {
    Root,
    Torrent { torrent_id: u64, name: String },
    File { torrent_id: u64, file_idx: usize, name: String },
}

impl InodeEntry {
    pub fn is_dir(&self) -> bool {
        matches!(self, InodeEntry::Root | InodeEntry::Torrent { .. })
    }
    
    pub fn is_file(&self) -> bool {
        matches!(self, InodeEntry::File { .. })
    }
}
```

### FileAttr

```rust
#[derive(Debug, Clone)]
pub struct FileAttr {
    pub size: u64,
    pub mode: u32,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
}

impl FileAttr {
    pub fn dir() -> Self {
        Self {
            mode: libc::S_IFDIR | 0o555,
            size: 4096,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
        }
    }
    
    pub fn file(size: u64) -> Self {
        Self {
            mode: libc::S_IFREG | 0o444,
            size,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
        }
    }
}
```

## Inode Management

### InodeTable

Manages mapping between inodes and filesystem entries.

```rust
use dashmap::DashMap;

pub struct InodeTable {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    torrent_inode_map: DashMap<u64, u64>,  // torrent_id -> inode
}

impl InodeTable {
    pub fn new() -> Self {
        let entries = DashMap::new();
        entries.insert(1, InodeEntry::Root);  // Root inode
        
        Self {
            next_inode: AtomicU64::new(2),
            entries,
            torrent_inode_map: DashMap::new(),
        }
    }
    
    pub fn allocate(&self, entry: InodeEntry) -> u64 {
        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);
        
        if let InodeEntry::Torrent { torrent_id, .. } = &entry {
            self.torrent_inode_map.insert(*torrent_id, inode);
        }
        
        self.entries.insert(inode, entry);
        inode
    }
    
    pub fn get(&self, inode: u64) -> Option<InodeEntry> {
        self.entries.get(&inode).map(|e| e.clone())
    }
    
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<u64> {
        self.torrent_inode_map.get(&torrent_id).map(|i| *i)
    }
    
    pub fn clear_torrents(&self) {
        self.entries.retain(|_, entry| !matches!(entry, InodeEntry::Torrent { .. } | InodeEntry::File { .. }));
        self.torrent_inode_map.clear();
        self.next_inode.store(2, Ordering::SeqCst);
    }
}
```

## FUSE Callbacks Implementation

### init()

```rust
fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> Result<(), c_int> {
    // Load torrent list from API
    let torrents = self.api_client.list_torrents()?;
    
    for torrent in torrents {
        let entry = InodeEntry::Torrent {
            torrent_id: torrent.id,
            name: torrent.name.clone(),
        };
        let inode = self.inodes.allocate(entry);
        
        // Cache torrent files
        for (idx, file) in torrent.files.iter().enumerate() {
            let file_entry = InodeEntry::File {
                torrent_id: torrent.id,
                file_idx: idx,
                name: file.name.clone(),
            };
            self.inodes.allocate(file_entry);
        }
        
        self.cache.insert_torrent(inode, torrent);
    }
    
    Ok(())
}
```

### lookup()

```rust
fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    let name = name.to_string_lossy();
    
    match self.inodes.get(parent) {
        Some(InodeEntry::Root) => {
            // Looking for a torrent directory
            if let Some((inode, _)) = self.cache.find_torrent_by_name(&name) {
                let attr = self.get_attr(inode);
                reply.entry(&TTL, &attr, 0);
            } else {
                reply.error(ENOENT);
            }
        }
        Some(InodeEntry::Torrent { torrent_id, .. }) => {
            // Looking for a file in torrent
            if let Some((inode, file)) = self.cache.find_file_in_torrent(torrent_id, &name) {
                let attr = self.get_attr(inode);
                reply.entry(&TTL, &attr, 0);
            } else {
                reply.error(ENOENT);
            }
        }
        _ => {
            reply.error(ENOENT);
        }
    }
}
```

### getattr()

```rust
fn getattr(&mut self, _req: &Request, inode: u64, reply: ReplyAttr) {
    match self.inodes.get(inode) {
        Some(InodeEntry::Root) => {
            let attr = FileAttr::dir();
            reply.attr(&TTL, &self.to_fuse_attr(inode, attr));
        }
        Some(InodeEntry::Torrent { .. }) => {
            let attr = FileAttr::dir();
            reply.attr(&TTL, &self.to_fuse_attr(inode, attr));
        }
        Some(InodeEntry::File { torrent_id, file_idx, .. }) => {
            if let Some(file) = self.cache.get_file(torrent_id, file_idx) {
                let attr = FileAttr::file(file.length);
                reply.attr(&TTL, &self.to_fuse_attr(inode, attr));
            } else {
                reply.error(ENOENT);
            }
        }
        None => {
            reply.error(ENOENT);
        }
    }
}
```

### readdir()

```rust
fn readdir(
    &mut self,
    _req: &Request,
    inode: u64,
    _fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
) {
    match self.inodes.get(inode) {
        Some(InodeEntry::Root) => {
            // List all torrents
            let torrents = self.cache.list_torrents();
            let mut entries = vec![
                (1, FileType::Directory, "."),
                (1, FileType::Directory, ".."),
            ];
            
            for (inode, torrent) in torrents {
                entries.push((inode, FileType::Directory, &torrent.name));
            }
            
            for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
                if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                    break;
                }
            }
            reply.ok();
        }
        Some(InodeEntry::Torrent { torrent_id, .. }) => {
            // List files in torrent
            if let Some(files) = self.cache.get_torrent_files(torrent_id) {
                let mut entries = vec![
                    (inode, FileType::Directory, "."),
                    (inode, FileType::Directory, ".."),
                ];
                
                for (file_inode, file) in files {
                    entries.push((file_inode, FileType::RegularFile, &file.name));
                }
                
                for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
                    if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                        break;
                    }
                }
                reply.ok();
            } else {
                reply.error(ENOENT);
            }
        }
        _ => {
            reply.error(ENOTDIR);
        }
    }
}
```

### read()

```rust
fn read(
    &mut self,
    _req: &Request,
    inode: u64,
    _fh: u64,
    offset: i64,
    size: u32,
    _flags: i32,
    _lock_owner: Option<u64>,
    reply: ReplyData,
) {
    match self.inodes.get(inode) {
        Some(InodeEntry::File { torrent_id, file_idx, .. }) => {
            let offset = offset as u64;
            let size = size as u64;
            
            // Get file info
            let file = match self.cache.get_file(torrent_id, file_idx) {
                Some(f) => f,
                None => {
                    reply.error(ENOENT);
                    return;
                }
            };
            
            // Check bounds
            if offset >= file.length {
                reply.data(&[]);
                return;
            }
            
            // Calculate actual read size
            let end = (offset + size).min(file.length);
            let read_size = (end - offset) as usize;
            
            // Make HTTP request with semaphore for concurrency control
            let data = match self.read_with_backoff(torrent_id, file_idx, offset, read_size) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    reply.error(EIO);
                    return;
                }
            };
            
            reply.data(&data);
        }
        _ => {
            reply.error(EISDIR);
        }
    }
}
```

## HTTP Read with Retry

```rust
use std::time::Duration;
use tokio::time::sleep;

impl TorrentFs {
    async fn read_with_backoff(
        &self,
        torrent_id: u64,
        file_idx: usize,
        offset: u64,
        size: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let max_retries = 3;
        let mut attempt = 0;
        
        loop {
            match self.read_data(torrent_id, file_idx, offset, size).await {
                Ok(data) => return Ok(data),
                Err(e) => {
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(e);
                    }
                    let delay = Duration::from_millis(100 * 2_u64.pow(attempt));
                    sleep(delay).await;
                }
            }
        }
    }
    
    async fn read_data(
        &self,
        torrent_id: u64,
        file_idx: usize,
        offset: u64,
        size: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let _permit = self.semaphore.acquire().await?;
        
        let start = offset;
        let end = offset + size as u64 - 1;
        
        let url = format!(
            "{}/torrents/{}/stream/{}",
            self.config.api_url, torrent_id, file_idx
        );
        
        let response = self.client
            .get(&url)
            .header("Range", format!("bytes={}-{}", start, end))
            .timeout(Duration::from_secs(self.config.read_timeout))
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }
        
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
```

## Cache Implementation

```rust
use std::time::{Duration, Instant};
use dashmap::DashMap;

pub struct Cache {
    torrents: DashMap<u64, CachedTorrent>,
    files: DashMap<(u64, usize), CachedFile>,
    config: CacheConfig,
}

pub struct CachedTorrent {
    pub torrent: Torrent,
    pub cached_at: Instant,
}

pub struct CachedFile {
    pub file: TorrentFile,
    pub torrent_id: u64,
    pub cached_at: Instant,
}

pub struct CacheConfig {
    pub torrent_ttl: Duration,
    pub file_ttl: Duration,
}

impl Cache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            torrents: DashMap::new(),
            files: DashMap::new(),
            config,
        }
    }
    
    pub fn insert_torrent(&self, inode: u64, torrent: Torrent) {
        let cached = CachedTorrent {
            torrent,
            cached_at: Instant::now(),
        };
        self.torrents.insert(inode, cached);
        
        // Cache files too
        for (idx, file) in cached.torrent.files.iter().enumerate() {
            let cached_file = CachedFile {
                file: file.clone(),
                torrent_id: torrent.id,
                cached_at: Instant::now(),
            };
            self.files.insert((torrent.id, idx), cached_file);
        }
    }
    
    pub fn get_torrent(&self, inode: u64) -> Option<Torrent> {
        self.torrents.get(&inode).and_then(|cached| {
            if cached.cached_at.elapsed() < self.config.torrent_ttl {
                Some(cached.torrent.clone())
            } else {
                None
            }
        })
    }
    
    pub fn get_file(&self, torrent_id: u64, file_idx: usize) -> Option<TorrentFile> {
        self.files.get(&(torrent_id, file_idx)).and_then(|cached| {
            if cached.cached_at.elapsed() < self.config.file_ttl {
                Some(cached.file.clone())
            } else {
                None
            }
        })
    }
    
    pub fn find_torrent_by_name(&self, name: &str) -> Option<(u64, Torrent)> {
        for entry in self.torrents.iter() {
            if entry.torrent.name == name {
                return Some((*entry.key(), entry.torrent.clone()));
            }
        }
        None
    }
    
    pub fn list_torrents(&self) -> Vec<(u64, Torrent)> {
        self.torrents
            .iter()
            .filter(|e| e.cached_at.elapsed() < self.config.torrent_ttl)
            .map(|e| (*e.key(), e.torrent.clone()))
            .collect()
    }
    
    pub fn clear(&self) {
        self.torrents.clear();
        self.files.clear();
    }
}
```

## Error Mapping

```rust
impl From<reqwest::Error> for FuseError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            FuseError::Timeout
        } else if e.is_connect() {
            FuseError::ApiUnavailable
        } else {
            FuseError::Network(e.to_string())
        }
    }
}

pub fn map_to_fuse_error(e: FuseError) -> c_int {
    match e {
        FuseError::NotFound => ENOENT,
        FuseError::PermissionDenied => EACCES,
        FuseError::Timeout => EAGAIN,
        FuseError::ApiUnavailable => EIO,
        FuseError::Network(_) => EIO,
        FuseError::InvalidArgument => EINVAL,
        FuseError::NotADirectory => ENOTDIR,
        FuseError::IsADirectory => EISDIR,
    }
}
```

## Configuration Structure

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    pub cache: CacheConfig,
    pub mount: MountConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub metadata_ttl: u64,
    pub torrent_list_ttl: u64,
    pub piece_ttl: u64,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MountConfig {
    pub allow_other: bool,
    pub auto_unmount: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerformanceConfig {
    pub read_timeout: u64,
    pub max_concurrent_reads: usize,
    pub readahead_size: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api: ApiConfig {
                url: "http://127.0.0.1:3030".to_string(),
            },
            cache: CacheConfig {
                metadata_ttl: 60,
                torrent_list_ttl: 30,
                piece_ttl: 5,
                max_entries: 1000,
            },
            mount: MountConfig {
                allow_other: false,
                auto_unmount: true,
            },
            performance: PerformanceConfig {
                read_timeout: 30,
                max_concurrent_reads: 10,
                readahead_size: 33554432, // 32MB
            },
        }
    }
}
```
