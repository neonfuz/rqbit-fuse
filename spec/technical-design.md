# Technical Design Document

## Data Structures

### Torrent (src/types/torrent.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: u64,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
}
```

**Note:** The actual implementation uses a simplified metadata structure. Full file information comes from the API's `TorrentInfo` struct.

### TorrentFile (src/types/file.rs)

```rust
#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub path: Vec<String>,      // Path components (was "components" in earlier design)
    pub length: u64,
    pub offset: u64,            // Byte offset within the torrent
}
```

**Field Changes:**
- Uses `path` instead of `components` (same purpose, different name)
- Includes `offset` field for calculating byte positions
- Does not include `name` or `file_idx` - these are tracked separately in InodeEntry

### InodeEntry (src/types/inode.rs)

```rust
#[derive(Debug, Clone)]
pub enum InodeEntry {
    Directory {
        ino: u64,
        name: String,
        parent: u64,
        children: Vec<u64>,
    },
    File {
        ino: u64,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: usize,      // Note: was "file_idx" in earlier design
        size: u64,
    },
    Symlink {
        ino: u64,
        name: String,
        parent: u64,
        target: String,
    },
}

impl InodeEntry {
    pub fn ino(&self) -> u64;
    pub fn name(&self) -> &str;
    pub fn parent(&self) -> u64;
    pub fn is_directory(&self) -> bool;
    pub fn is_file(&self) -> bool;
    pub fn is_symlink(&self) -> bool;
}
```

**Major Changes from Spec:**
- Root is a `Directory` with `ino: 1` (not a separate `Root` variant)
- Torrent directories use `Directory` variant (not separate `Torrent` variant)
- Each entry tracks its own inode number (`ino` field)
- Parent-child relationships are explicit with `parent` field
- Directories maintain a `children: Vec<u64>` list
- **Symlinks are supported** (not in original spec)

### FileAttr (src/types/attr.rs)

Uses `fuser::FileAttr` directly with helper functions:

```rust
pub fn default_file_attr(ino: u64, size: u64) -> FileAttr {
    FileAttr {
        ino,
        size,
        blocks: (size + 511) / 512,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        kind: FileType::RegularFile,
        perm: 0o444,        // Read-only
        nlink: 1,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn default_dir_attr(ino: u64) -> FileAttr {
    FileAttr {
        ino,
        size: 4096,
        blocks: 8,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        kind: FileType::Directory,
        perm: 0o555,        // Read-only, executable for dirs
        nlink: 2,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}
```

**Note:** The implementation uses `fuser::FileAttr` directly rather than a custom struct.

## Inode Management

### InodeManager (src/fs/inode.rs)

Manages mapping between inodes and filesystem entries with thread-safe concurrent access.

```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    path_to_inode: DashMap<String, u64>,    // Reverse lookup
    torrent_to_inode: DashMap<u64, u64>,    // torrent_id -> directory inode
}

impl InodeManager {
    pub fn new() -> Self {
        // Creates root directory with ino: 1
    }
    
    // Specialized allocation methods
    pub fn allocate_torrent_directory(
        &self,
        torrent_id: u64,
        name: String,
        parent: u64,
    ) -> u64;
    
    pub fn allocate_file(
        &self,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: usize,
        size: u64,
    ) -> u64;
    
    pub fn allocate_symlink(
        &self,
        name: String,
        parent: u64,
        target: String,
    ) -> u64;
    
    // Lookup methods
    pub fn get(&self, inode: u64) -> Option<InodeEntry>;
    pub fn lookup_by_path(&self, path: &str) -> Option<u64>;
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<u64>;
    pub fn get_children(&self, inode: u64) -> Vec<InodeEntry>;
    pub fn entries(&self) -> Vec<InodeEntry>;
    pub fn torrent_to_inode(&self, torrent_id: u64) -> Option<u64>;
}
```

**Key Differences from Spec:**
- Name changed from `InodeTable` to `InodeManager`
- Uses specialized allocation methods instead of single generic method
- Maintains `path_to_inode` for reverse path lookups (not in spec)
- Maintains explicit children lists in directories
- Supports symlinks

## FUSE Callbacks Implementation

### init() (src/fs/filesystem.rs)

```rust
fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> Result<(), c_int> {
    // 1. Validates mount point
    // 2. Checks root inode exists
    // 3. Starts background status monitoring task
    // 4. Starts background torrent discovery task
    // 5. Does NOT load torrents immediately (done lazily by discovery task)
}
```

**Difference from Spec:** Uses lazy/async approach with background tasks rather than synchronous loading.

### lookup() (src/fs/filesystem.rs)

```rust
fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    let name = name.to_string_lossy();
    
    // Build full path from parent and name
    let path = if parent == 1 {
        format!("/{}", name)
    } else {
        match self.inode_manager.get(parent) {
            Some(parent_entry) => {
                format!("{}/{}", parent_entry.name(), name)
            }
            None => {
                reply.error(ENOENT);
                return;
            }
        }
    };
    
    // Lookup by path
    match self.inode_manager.lookup_by_path(&path) {
        Some(ino) => {
            if let Some(entry) = self.inode_manager.get(ino) {
                let attr = build_file_attr(ino, &entry);
                reply.entry(&Duration::new(1, 0), &attr, 0);
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

**Difference from Spec:** Uses `inode_manager.lookup_by_path()` instead of cache methods.

### getattr() (src/fs/filesystem.rs)

```rust
fn getattr(&mut self, _req: &Request, inode: u64, reply: ReplyAttr) {
    match self.inode_manager.get(inode) {
        Some(entry) => {
            let attr = build_file_attr(inode, &entry);
            reply.attr(&Duration::new(1, 0), &attr);
        }
        None => {
            reply.error(ENOENT);
        }
    }
}
```

**Difference from Spec:** Derives attributes directly from `InodeEntry` rather than using cache.

### readdir() (src/fs/filesystem.rs)

```rust
fn readdir(
    &mut self,
    _req: &Request,
    inode: u64,
    _fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
) {
    // Trigger torrent discovery when listing root
    if inode == 1 {
        self.trigger_torrent_discovery();
    }
    
    match self.inode_manager.get(inode) {
        Some(InodeEntry::Directory { children, .. }) => {
            let mut entries = vec![
                (inode, FileType::Directory, "."),
                (inode, FileType::Directory, ".."),
            ];
            
            // Add children
            for child_ino in children {
                if let Some(child) = self.inode_manager.get(*child_ino) {
                    let file_type = match &child {
                        InodeEntry::Directory { .. } => FileType::Directory,
                        InodeEntry::File { .. } => FileType::RegularFile,
                        InodeEntry::Symlink { .. } => FileType::Symlink,
                    };
                    entries.push((*child_ino, file_type, child.name()));
                }
            }
            
            // Reply with entries
            for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
                if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                    break;
                }
            }
            reply.ok();
        }
        _ => {
            reply.error(ENOTDIR);
        }
    }
}
```

**Differences from Spec:**
- Triggers torrent discovery when listing root
- Uses `inode_manager.get_children()` pattern
- Supports symlinks

### read() (src/fs/filesystem.rs)

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
    match self.inode_manager.get(inode) {
        Some(InodeEntry::File { torrent_id, file_index, size: file_size, .. }) => {
            let offset = offset as u64;
            let size = size as u64;
            
            // Check bounds
            if offset >= file_size {
                reply.data(&[]);
                return;
            }
            
            // Calculate actual read size
            let end = (offset + size).min(file_size);
            let read_size = (end - offset) as usize;
            
            // Clamp to FUSE_MAX_READ (64KB)
            let read_size = read_size.min(64 * 1024);
            
            // Make HTTP request with persistent streaming
            let result = self.runtime.block_on(async {
                let timeout = Duration::from_secs(self.config.performance.read_timeout);
                match tokio::time::timeout(timeout, async {
                    // Check piece availability if enabled
                    if self.config.performance.piece_check_enabled {
                        if let Err(e) = self.check_piece_availability(torrent_id, file_index, offset, read_size).await {
                            if self.config.performance.return_eagain_for_unavailable {
                                return Err(libc::EAGAIN);
                            }
                        }
                    }
                    
                    // Read with persistent streaming
                    self.api_client.read_file_streaming(
                        torrent_id,
                        file_index,
                        offset,
                        read_size,
                    ).await
                }).await {
                    Ok(Ok(data)) => Ok(data),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(libc::EAGAIN),  // Timeout
                }
            });
            
            match result {
                Ok(data) => {
                    // Track read for prefetching
                    self.track_and_prefetch(torrent_id, file_index, offset, read_size);
                    reply.data(&data);
                }
                Err(err) => {
                    reply.error(err);
                }
            }
        }
        _ => {
            reply.error(EISDIR);
        }
    }
}
```

**Major Differences from Spec:**
1. Uses `read_file_streaming()` with persistent connections (not simple retry)
2. Implements read-ahead/prefetching with `track_and_prefetch()`
3. Piece availability checking with EAGAIN option
4. Clamps read size to 64KB (FUSE_MAX_READ)
5. Uses `tokio::time::timeout` for timeout handling

## HTTP Read Implementation

### Persistent Streaming (src/api/streaming.rs)

The actual implementation uses a `PersistentStreamManager` instead of simple retry logic:

```rust
pub struct PersistentStreamManager {
    client: Client,
    active_streams: DashMap<String, Arc<Mutex<HttpStream>>>,
}

pub struct HttpStream {
    url: String,
    response: Response,
    current_offset: u64,
}

impl PersistentStreamManager {
    pub async fn read_file_streaming(
        &self,
        torrent_id: u64,
        file_idx: usize,
        offset: u64,
        size: usize,
    ) -> Result<Vec<u8>, ApiError> {
        let url = format!("{}/torrents/{}/stream/{}", self.api_url, torrent_id, file_idx);
        
        // Get or create persistent stream
        let stream = self.get_or_create_stream(&url).await?;
        let mut stream = stream.lock().await;
        
        // Seek to offset if needed
        if offset != stream.current_offset {
            // Create new connection at requested offset
            drop(stream);
            self.close_stream(&url).await;
            let stream = self.create_stream_at_offset(&url, offset).await?;
            stream.lock().await
        } else {
            stream
        };
        
        // Read data
        let mut buffer = vec![0u8; size];
        let bytes_read = stream.response.read_exact(&mut buffer).await?;
        buffer.truncate(bytes_read);
        stream.current_offset += bytes_read as u64;
        
        Ok(buffer)
    }
}
```

**Key Features:**
- Connection reuse for sequential reads
- Handles rqbit bug: returns 200 OK instead of 206 Partial Content
- Streaming response handling
- Circuit breaker integration

## Cache Implementation

### Generic Cache (src/cache.rs)

The actual implementation is a generic LRU cache:

```rust
pub struct Cache<K, V> {
    entries: DashMap<K, Arc<CacheEntry<V>>>,
    max_entries: usize,
    lru_counter: AtomicU64,
}

pub struct CacheEntry<T> {
    value: T,
    created_at: Instant,
    sequence: u64,  // For LRU ordering
}

pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
    pub expired: AtomicU64,
}

impl<K: Eq + std::hash::Hash, V: Clone> Cache<K, V> {
    pub fn get(&self, key: &K, ttl: Duration) -> Option<V> {
        // Check if entry exists and not expired
        // Update LRU sequence on hit
        // Return cloned value
    }
    
    pub fn insert(&self, key: K, value: V) {
        // Insert with TTL check
        // Evict LRU entries if at capacity
    }
    
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        // Insert with specific TTL
    }
    
    pub fn remove(&self, key: &K) -> Option<V>;
    pub fn clear(&self);
    pub fn stats(&self) -> CacheStats;
    pub fn contains_key(&self, key: &K) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

**Major Differences from Spec:**
- Generic design (not torrent-specific)
- LRU eviction in addition to TTL
- Statistics tracking
- No torrent-specific methods

## Error Mapping

### ApiError (src/api/types.rs)

```rust
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    #[error("Torrent not found")]
    TorrentNotFound,
    
    #[error("File not found")]
    FileNotFound,
    
    #[error("Invalid range")]
    InvalidRange,
    
    #[error("Retry limit exceeded")]
    RetryLimitExceeded,
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Connection timeout")]
    ConnectionTimeout,
    
    #[error("Read timeout")]
    ReadTimeout,
    
    #[error("Server disconnected")]
    ServerDisconnected,
    
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Service unavailable")]
    ServiceUnavailable,
}

impl ApiError {
    /// Check if error is transient and can be retried
    pub fn is_transient(&self) -> bool {
        matches!(self,
            ApiError::ConnectionTimeout |
            ApiError::ReadTimeout |
            ApiError::ServerDisconnected |
            ApiError::NetworkError(_) |
            ApiError::CircuitBreakerOpen |
            ApiError::ServiceUnavailable
        )
    }
    
    /// Convert to FUSE error code
    pub fn to_fuse_error(&self) -> c_int {
        match self {
            ApiError::TorrentNotFound => libc::ENOENT,
            ApiError::FileNotFound => libc::ENOENT,
            ApiError::InvalidRange => libc::EINVAL,
            ApiError::ConnectionTimeout => libc::EAGAIN,
            ApiError::ReadTimeout => libc::EAGAIN,
            ApiError::ServerDisconnected => libc::ENOTCONN,
            ApiError::NetworkError(_) => libc::ENETUNREACH,
            ApiError::CircuitBreakerOpen => libc::EAGAIN,
            ApiError::ServiceUnavailable => libc::EAGAIN,
            ApiError::ApiError { status: 404, .. } => libc::ENOENT,
            ApiError::ApiError { status: 403, .. } => libc::EACCES,
            ApiError::ApiError { status: 503, .. } => libc::EAGAIN,
            _ => libc::EIO,
        }
    }
}
```

**Differences from Spec:**
- More comprehensive error types
- `is_transient()` method for retry logic
- HTTP status code to FUSE error mapping

## Configuration Structure

### Config (src/config/mod.rs)

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    pub cache: CacheConfig,
    pub mount: MountConfig,
    pub performance: PerformanceConfig,
    pub monitoring: MonitoringConfig,    // Not in original spec
    pub logging: LoggingConfig,          // Not in original spec
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
    pub mount_point: PathBuf,    // Not in original spec
    pub allow_other: bool,
    pub auto_unmount: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerformanceConfig {
    pub read_timeout: u64,
    pub max_concurrent_reads: usize,
    pub readahead_size: u64,
    pub piece_check_enabled: bool,              // Not in original spec
    pub return_eagain_for_unavailable: bool,    // Not in original spec
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitoringConfig {     // Not in original spec
    pub status_poll_interval: u64,
    pub stalled_timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {        // Not in original spec
    pub level: String,
    pub log_fuse_operations: bool,
    pub log_api_calls: bool,
    pub metrics_enabled: bool,
    pub metrics_interval_secs: u64,
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
                mount_point: PathBuf::from("/mnt/torrents"),  // Default from code
                allow_other: false,
                auto_unmount: true,
            },
            performance: PerformanceConfig {
                read_timeout: 30,
                max_concurrent_reads: 10,
                readahead_size: 33554432,
                piece_check_enabled: true,
                return_eagain_for_unavailable: false,
            },
            monitoring: MonitoringConfig {
                status_poll_interval: 5,
                stalled_timeout: 300,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                log_fuse_operations: true,
                log_api_calls: true,
                metrics_enabled: true,
                metrics_interval_secs: 60,
            },
        }
    }
}
```

**Additions not in original spec:**
- `monitoring` section with poll intervals
- `logging` section with operation logging
- Additional performance options

## API Client with Circuit Breaker

### RqbitClient (src/api/client.rs)

```rust
pub struct RqbitClient {
    client: Client,
    base_url: String,
    circuit_breaker: CircuitBreaker,
    metrics: Arc<ApiMetrics>,
}

pub struct CircuitBreaker {
    state: AtomicU8,  // Closed, Open, HalfOpen
    failure_count: AtomicU32,
    last_failure_time: Mutex<Option<Instant>>,
    threshold: u32,
    timeout: Duration,
}

impl RqbitClient {
    // API Methods
    pub async fn list_torrents(&self) -> Result<Vec<Torrent>, ApiError>;
    pub async fn get_torrent(&self, id: u64) -> Result<Torrent, ApiError>;
    pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats, ApiError>;
    pub async fn read_file(&self, torrent_id: u64, file_idx: usize, offset: u64, size: usize) -> Result<Vec<u8>, ApiError>;
    pub async fn read_file_streaming(&self, torrent_id: u64, file_idx: usize, offset: u64, size: usize) -> Result<Vec<u8>, ApiError>;
    pub async fn check_piece_availability(&self, torrent_id: u64) -> Result<Vec<u8>, ApiError>;
    
    // Circuit breaker methods
    async fn call_with_circuit_breaker<T>(&self, operation: impl FnOnce() -> Fut) -> Result<T, ApiError>;
    fn record_success(&self);
    fn record_failure(&self);
}
```

## Metrics Collection

### Metrics (src/metrics.rs)

```rust
pub struct FuseMetrics {
    pub getattr_count: AtomicU64,
    pub lookup_count: AtomicU64,
    pub readdir_count: AtomicU64,
    pub read_count: AtomicU64,
    pub open_count: AtomicU64,
    pub release_count: AtomicU64,
    pub readlink_count: AtomicU64,
    pub error_count: AtomicU64,
    pub total_bytes_read: AtomicU64,
}

pub struct ApiMetrics {
    pub request_count: AtomicU64,
    pub retry_count: AtomicU64,
    pub circuit_breaker_state: AtomicU8,
    pub response_time_ms: AtomicU64,
}
```

Last updated: 2024-02-14
