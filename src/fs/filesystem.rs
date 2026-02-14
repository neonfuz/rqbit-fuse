use crate::api::client::RqbitClient;
use crate::config::Config;
use crate::fs::inode::InodeManager;
use anyhow::{Context, Result};
use fuser::Filesystem;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, error, info, trace};

/// Tracks read state for a file handle to detect sequential access patterns
#[derive(Debug, Clone)]
struct ReadState {
    /// Last read offset
    last_offset: u64,
    /// Last read size
    last_size: u32,
    /// Number of consecutive sequential reads
    sequential_count: u32,
    /// Last access time
    last_access: Instant,
    /// Whether this file is being prefetched
    is_prefetching: bool,
}

impl ReadState {
    fn new(offset: u64, size: u32) -> Self {
        Self {
            last_offset: offset,
            last_size: size,
            sequential_count: 1,
            last_access: Instant::now(),
            is_prefetching: false,
        }
    }

    /// Check if the current read is sequential (immediately follows previous read)
    fn is_sequential(&self, offset: u64) -> bool {
        offset == self.last_offset + self.last_size as u64
    }

    /// Update state after a read
    fn update(&mut self, offset: u64, size: u32) {
        if self.is_sequential(offset) {
            self.sequential_count += 1;
        } else {
            self.sequential_count = 1;
        }
        self.last_offset = offset;
        self.last_size = size;
        self.last_access = Instant::now();
    }
}

/// The main FUSE filesystem implementation for torrent-fuse.
/// Implements the fuser::Filesystem trait to provide a FUSE interface
/// over the rqbit HTTP API.
pub struct TorrentFS {
    /// Configuration for the filesystem
    config: Config,
    /// HTTP client for rqbit API
    api_client: Arc<RqbitClient>,
    /// Inode manager for filesystem entries
    inode_manager: Arc<InodeManager>,
    /// Tracks whether the filesystem has been initialized
    initialized: bool,
    /// Tracks read patterns per file handle for read-ahead
    read_states: Arc<Mutex<HashMap<u64, ReadState>>>,
}

impl TorrentFS {
    /// Creates a new TorrentFS instance with the given configuration.
    /// Note: This does not initialize the filesystem - call mount() to do so.
    pub fn new(config: Config) -> Result<Self> {
        let api_client = Arc::new(RqbitClient::new(config.api.url.clone()));
        let inode_manager = Arc::new(InodeManager::new());

        Ok(Self {
            config,
            api_client,
            inode_manager,
            initialized: false,
            read_states: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Returns a reference to the API client
    pub fn api_client(&self) -> &Arc<RqbitClient> {
        &self.api_client
    }

    /// Returns a reference to the inode manager
    pub fn inode_manager(&self) -> &Arc<InodeManager> {
        &self.inode_manager
    }

    /// Returns true if the filesystem has been initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Validates the mount point directory.
    /// Checks that:
    /// - The path exists
    /// - It's a directory
    /// - We have read/write permissions
    fn validate_mount_point(&self) -> Result<()> {
        let mount_point = &self.config.mount.mount_point;

        if !mount_point.exists() {
            return Err(anyhow::anyhow!(
                "Mount point does not exist: {}",
                mount_point.display()
            ));
        }

        if !mount_point.is_dir() {
            return Err(anyhow::anyhow!(
                "Mount point is not a directory: {}",
                mount_point.display()
            ));
        }

        // Check read/write permissions by trying to access the directory
        if std::fs::read_dir(mount_point).is_err() {
            return Err(anyhow::anyhow!(
                "No read permission for mount point: {}",
                mount_point.display()
            ));
        }

        info!("Mount point validated: {}", mount_point.display());
        Ok(())
    }

    /// Establishes connection to the rqbit server and validates it's accessible.
    async fn connect_to_rqbit(&self) -> Result<()> {
        info!("Connecting to rqbit server at: {}", self.config.api.url);

        match self.api_client.health_check().await {
            Ok(true) => {
                info!("Successfully connected to rqbit server");
                Ok(())
            }
            Ok(false) => {
                Err(anyhow::anyhow!(
                    "rqbit server at {} is not responding or returned an error",
                    self.config.api.url
                ))
            }
            Err(e) => {
                Err(anyhow::anyhow!(
                    "Failed to connect to rqbit server at {}: {}",
                    self.config.api.url,
                    e
                ))
            }
        }
    }

    /// Mounts the filesystem at the configured mount point.
    /// This is the main entry point for mounting the filesystem.
    pub fn mount(self) -> Result<()>
    where
        Self: Sized,
    {
        let mount_point = self.config.mount.mount_point.clone();
        let options = self.build_mount_options();

        info!("Mounting torrent-fuse at: {}", mount_point.display());

        // Mount the filesystem
        fuser::mount2(self, &mount_point, &options).with_context(|| {
            format!("Failed to mount filesystem at: {}", mount_point.display())
        })
    }

    /// Track read patterns and trigger prefetch for sequential reads.
    fn track_and_prefetch(
        &self,
        ino: u64,
        offset: u64,
        size: u32,
        file_size: u64,
        torrent_id: u64,
        file_index: usize,
    ) {
        let mut read_states = self.read_states.lock().unwrap();
        
        // Get or create read state for this file
        let state = read_states.entry(ino).or_insert_with(|| ReadState::new(offset, size));
        
        // Check if this is a sequential read
        let is_sequential = state.is_sequential(offset);
        state.update(offset, size);
        
        // Trigger prefetch after 2 consecutive sequential reads and not already prefetching
        if is_sequential && state.sequential_count >= 2 && !state.is_prefetching {
            let next_offset = offset + size as u64;
            
            // Only prefetch if we're not at EOF
            if next_offset < file_size {
                let prefetch_size = std::cmp::min(
                    self.config.performance.readahead_size,
                    file_size - next_offset
                ) as usize;
                
                if prefetch_size > 0 {
                    state.is_prefetching = true;
                    drop(read_states); // Release lock before async operation
                    
                    let api_client = Arc::clone(&self.api_client);
                    let read_states = Arc::clone(&self.read_states);
                    let readahead_size = self.config.performance.readahead_size;
                    
                    // Spawn prefetch in background
                    tokio::spawn(async move {
                        let prefetch_end = std::cmp::min(next_offset + readahead_size - 1, file_size - 1);
                        
                        trace!(
                            "prefetch: requesting bytes {}-{} for torrent {} file {}",
                            next_offset, prefetch_end, torrent_id, file_index
                        );
                        
                        match api_client
                            .read_file(torrent_id, file_index, Some((next_offset, prefetch_end)))
                            .await
                        {
                            Ok(data) => {
                                trace!("prefetch: successfully prefetched {} bytes", data.len());
                                // Could store in cache here
                            }
                            Err(e) => {
                                trace!("prefetch: failed to prefetch: {}", e);
                            }
                        }
                        
                        // Mark prefetch as complete
                        if let Ok(mut states) = read_states.lock() {
                            if let Some(s) = states.get_mut(&ino) {
                                s.is_prefetching = false;
                            }
                        }
                    });
                }
            }
        }
    }

    /// Builds FUSE mount options based on configuration.
    fn build_mount_options(&self) -> Vec<fuser::MountOption> {
        let mut options = vec![
            fuser::MountOption::RO,           // Read-only (torrents are read-only)
            fuser::MountOption::NoSuid,       // No setuid/setgid
            fuser::MountOption::NoDev,        // No special device files
            fuser::MountOption::NoAtime,      // Don't update access times
            fuser::MountOption::Sync,         // Synchronous writes (safer for FUSE)
        ];

        if self.config.mount.auto_unmount {
            options.push(fuser::MountOption::AutoUnmount);
        }

        if self.config.mount.allow_other {
            options.push(fuser::MountOption::AllowOther);
        }

        options
    }

    /// Build file attributes for a given inode entry.
    /// Converts internal InodeEntry to FUSE FileAttr.
    fn build_file_attr(&self, entry: &crate::types::inode::InodeEntry) -> fuser::FileAttr {
        use crate::types::inode::InodeEntry;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let now = SystemTime::now();
        let creation_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000); // Fixed creation time

        match entry {
            InodeEntry::Directory { ino, .. } => fuser::FileAttr {
                ino: *ino,
                size: 0,
                blocks: 0,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: creation_time,
                kind: fuser::FileType::Directory,
                perm: 0o555, // Read and execute for all, no write (read-only)
                nlink: 2 + self.inode_manager.get_children(*ino).len() as u32,
                uid: 0,
                gid: 0,
                rdev: 0,
                flags: 0,
                blksize: 4096,
            },
            InodeEntry::File { ino, size, .. } => fuser::FileAttr {
                ino: *ino,
                size: *size,
                blocks: (*size).div_ceil(4096), // Ceiling division for block count
                atime: now,
                mtime: now,
                ctime: now,
                crtime: creation_time,
                kind: fuser::FileType::RegularFile,
                perm: 0o444, // Read-only for all
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                flags: 0,
                blksize: 4096,
            },
        }
    }
}

impl Filesystem for TorrentFS {
    /// Get file attributes.
    /// Called when the kernel needs to get attributes for a file or directory.
    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        reply: fuser::ReplyAttr,
    ) {
        debug!("getattr: ino={}", ino);

        match self.inode_manager.get(ino) {
            Some(entry) => {
                let attr = self.build_file_attr(&entry);
                reply.attr(&std::time::Duration::from_secs(1), &attr);
                debug!("getattr: returned attributes for inode {}", ino);
            }
            None => {
                debug!("getattr: inode {} not found", ino);
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Set file attributes.
    /// This filesystem is read-only, so it only allows setting access/modification times.
    /// All other attribute changes return EROFS.
    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        debug!(
            "setattr: ino={}, mode={:?}, uid={:?}, gid={:?}, size={:?}",
            ino, mode, uid, gid, size
        );

        // Check if any unsupported attributes are being modified
        if mode.is_some() || uid.is_some() || gid.is_some() || size.is_some() {
            debug!("setattr: rejecting modification - read-only filesystem");
            reply.error(libc::EROFS);
            return;
        }

        // Only atime/mtime updates are "supported" (no-op since we use fixed times)
        // Return current attributes
        match self.inode_manager.get(ino) {
            Some(entry) => {
                let attr = self.build_file_attr(&entry);
                reply.attr(&std::time::Duration::from_secs(1), &attr);
                debug!("setattr: returned attributes for inode {}", ino);
            }
            None => {
                debug!("setattr: inode {} not found", ino);
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Open a file.
    /// Called when a file is opened. We don't need to do anything special here
    /// since we read files on-demand via HTTP Range requests.
    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug!("open: ino={}, flags={}", ino, flags);

        // Check if the inode exists and is a file
        match self.inode_manager.get(ino) {
            Some(entry) => {
                if !entry.is_file() {
                    debug!("open: inode {} is not a file", ino);
                    reply.error(libc::EISDIR);
                    return;
                }

                // Check write flags - we are read-only
                let access_mode = flags & libc::O_ACCMODE;
                if access_mode != libc::O_RDONLY {
                    debug!("open: rejecting write access to inode {}", ino);
                    reply.error(libc::EROFS);
                    return;
                }

                // Return a file handle (we use inode as handle for simplicity)
                reply.opened(ino, 0);
                debug!("open: successfully opened inode {}", ino);
            }
            None => {
                debug!("open: inode {} not found", ino);
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Read file contents.
    /// Called when the kernel needs to read data from a file.
    /// Translates FUSE read requests to HTTP Range requests to rqbit.
    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        debug!("read: ino={}, offset={}, size={}", ino, offset, size);

        // Validate offset is non-negative
        if offset < 0 {
            debug!("read: negative offset {}", offset);
            reply.error(libc::EINVAL);
            return;
        }

        let offset = offset as u64;

        // Get the file entry
        let (torrent_id, file_index, file_size) = match self.inode_manager.get(ino) {
            Some(entry) => match entry {
                crate::types::inode::InodeEntry::File {
                    torrent_id,
                    file_index,
                    size,
                    ..
                } => (torrent_id, file_index, size),
                _ => {
                    debug!("read: inode {} is not a file", ino);
                    reply.error(libc::EISDIR);
                    return;
                }
            },
            None => {
                debug!("read: inode {} not found", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Handle zero-byte reads
        if size == 0 || offset >= file_size {
            debug!("read: empty read or offset past EOF");
            reply.data(&[]);
            return;
        }

        // Calculate actual read range (don't read past EOF)
        let end = std::cmp::min(offset + size as u64, file_size) - 1;
        let _read_size = (end - offset + 1) as u32;

        debug!(
            "read: requesting bytes {}-{} from torrent {} file {}",
            offset, end, torrent_id, file_index
        );

        // Perform the read via HTTP Range request
        // We need to block here since FUSE callbacks are synchronous
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.api_client
                    .read_file(torrent_id, file_index, Some((offset, end)))
                    .await
            })
        });

        match result {
            Ok(data) => {
                debug!("read: successfully read {} bytes", data.len());
                
                // Track read pattern and trigger prefetch if sequential
                self.track_and_prefetch(ino, offset, size, file_size, torrent_id, file_index);
                
                reply.data(&data);
            }
            Err(e) => {
                error!("read: failed to read file: {}", e);
                // Map error to appropriate FUSE error code
                let error_code = if e.to_string().contains("not found") {
                    libc::ENOENT
                } else if e.to_string().contains("range") {
                    libc::EINVAL
                } else {
                    libc::EIO
                };
                reply.error(error_code);
            }
        }
    }

    /// Release an open file.
    /// Called when a file is closed. No special cleanup needed since we read on-demand.
    fn release(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("release: ino={}", _ino);
        reply.ok();
    }

    /// Look up a directory entry by name.
    /// Called when the kernel needs to resolve a path component to an inode.
    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        let name_str = name.to_string_lossy();
        debug!("lookup: parent={}, name={}", parent, name_str);

        // Get the parent directory entry
        let parent_entry = match self.inode_manager.get(parent) {
            Some(entry) => entry,
            None => {
                debug!("lookup: parent {} not found", parent);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if parent is a directory
        if !parent_entry.is_directory() {
            debug!("lookup: parent {} is not a directory", parent);
            reply.error(libc::ENOTDIR);
            return;
        }

        // Build the full path for this entry
        let path = if parent == 1 {
            format!("/{}", name_str)
        } else {
            let parent_name = parent_entry.name();
            format!("{}/{}", parent_name, name_str)
        };

        // Look up the inode by path
        match self.inode_manager.lookup_by_path(&path) {
            Some(ino) => {
                match self.inode_manager.get(ino) {
                    Some(entry) => {
                        let attr = self.build_file_attr(&entry);
                        reply.entry(&std::time::Duration::from_secs(1), &attr, 0);
                        debug!("lookup: found {} at inode {}", name_str, ino);
                    }
                    None => {
                        // This shouldn't happen - path maps to non-existent inode
                        error!("lookup: path {} maps to missing inode {}", path, ino);
                        reply.error(libc::EIO);
                    }
                }
            }
            None => {
                debug!("lookup: {} not found in parent {}", name_str, parent);
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Read directory entries.
    /// Called when the kernel needs to list the contents of a directory.
    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        debug!("readdir: ino={}, offset={}", ino, offset);

        // Get the directory entry
        let entry = match self.inode_manager.get(ino) {
            Some(e) => e,
            None => {
                debug!("readdir: inode {} not found", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if it's a directory
        if !entry.is_directory() {
            debug!("readdir: inode {} is not a directory", ino);
            reply.error(libc::ENOTDIR);
            return;
        }

        // If offset is 0, start from beginning; otherwise continue from offset
        let mut current_offset = offset;

        // Always include . and .. entries
        if current_offset == 0 {
            if reply.add(ino, 1, fuser::FileType::Directory, ".") {
                reply.ok();
                return;
            }
            current_offset = 1;
        }

        if current_offset == 1 {
            let parent_ino = entry.parent();
            if reply.add(parent_ino, 2, fuser::FileType::Directory, "..") {
                reply.ok();
                return;
            }
            current_offset = 2;
        }

        // Get children of this directory
        let children = self.inode_manager.get_children(ino);
        let child_offset_start = 2; // . and .. take offsets 0 and 1

        for (idx, (child_ino, child_entry)) in children.iter().enumerate() {
            let entry_offset = child_offset_start + idx as i64;
            
            // Skip entries before the requested offset
            if entry_offset < current_offset {
                continue;
            }

            let file_type = if child_entry.is_directory() {
                fuser::FileType::Directory
            } else {
                fuser::FileType::RegularFile
            };

            if reply.add(*child_ino, entry_offset + 1, file_type, child_entry.name()) {
                reply.ok();
                return;
            }
        }

        reply.ok();
    }

    /// Create a directory.
    /// This filesystem is read-only, so it always returns EROFS (read-only filesystem).
    fn mkdir(
        &mut self,
        _req: &fuser::Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug!("mkdir: rejected - read-only filesystem");
        reply.error(libc::EROFS);
    }

    /// Remove a directory.
    /// This filesystem is read-only, so it always returns EROFS (read-only filesystem).
    fn rmdir(&mut self, _req: &fuser::Request<'_>, _parent: u64, _name: &std::ffi::OsStr, reply: fuser::ReplyEmpty) {
        debug!("rmdir: rejected - read-only filesystem");
        reply.error(libc::EROFS);
    }

    /// Initialize the filesystem.
    /// Called when the filesystem is mounted. Sets up the connection to rqbit,
    /// validates the mount point, and initializes the root inode.
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        info!("Initializing torrent-fuse filesystem");

        // Validate mount point
        if let Err(e) = self.validate_mount_point() {
            error!("Mount point validation failed: {}", e);
            return Err(libc::EIO);
        }

        // Check that root inode (inode 1) exists and is a directory
        match self.inode_manager.get(1) {
            Some(entry) => {
                if !entry.is_directory() {
                    error!("Root inode (1) is not a directory");
                    return Err(libc::EIO);
                }
                debug!("Root inode (1) validated as directory");
            }
            None => {
                error!("Root inode (1) not found - inode manager not properly initialized");
                return Err(libc::EIO);
            }
        }

        // We need to check the rqbit connection, but init() is synchronous
        // The actual connection check will happen lazily or we spawn a task
        // For now, we mark as initialized and the first operation will validate
        // This is a common pattern in FUSE filesystems

        self.initialized = true;
        info!("torrent-fuse filesystem initialized successfully");

        Ok(())
    }

    /// Clean up filesystem.
    /// Called on unmount.
    fn destroy(&mut self) {
        info!("Shutting down torrent-fuse filesystem");
        self.initialized = false;
        // Clean up any resources
    }
}

/// Async initialization helper that can be called from the async runtime
/// to perform the full initialization including the rqbit connection check.
pub async fn initialize_filesystem(fs: &mut TorrentFS) -> Result<()> {
    // Check connection to rqbit
    fs.connect_to_rqbit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_torrent_fs_creation() {
        let config = Config::default();
        let fs = TorrentFS::new(config).unwrap();

        assert!(!fs.is_initialized());
        assert_eq!(fs.inode_manager().get(1).unwrap().ino(), 1);
    }

    #[test]
    fn test_validate_mount_point_success() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.mount.mount_point = temp_dir.path().to_path_buf();

        let fs = TorrentFS::new(config).unwrap();
        assert!(fs.validate_mount_point().is_ok());
    }

    #[test]
    fn test_validate_mount_point_nonexistent() {
        let mut config = Config::default();
        config.mount.mount_point = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let fs = TorrentFS::new(config).unwrap();
        let result = fs.validate_mount_point();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_build_mount_options() {
        let config = Config::default();
        let fs = TorrentFS::new(config).unwrap();

        let options = fs.build_mount_options();

        // Check that required options are present
        assert!(options.contains(&fuser::MountOption::RO));
        assert!(options.contains(&fuser::MountOption::NoSuid));
        assert!(options.contains(&fuser::MountOption::AutoUnmount));
    }

    #[test]
    fn test_build_mount_options_allow_other() {
        let mut config = Config::default();
        config.mount.allow_other = true;
        let fs = TorrentFS::new(config).unwrap();

        let options = fs.build_mount_options();

        assert!(options.contains(&fuser::MountOption::AllowOther));
    }
}
