use crate::api::client::RqbitClient;
use crate::api::types::{TorrentState, TorrentStatus};
use crate::config::Config;
use crate::fs::inode::InodeManager;
use crate::types::inode::InodeEntry;
use anyhow::{Context, Result};
use dashmap::DashMap;
use fuser::Filesystem;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, error, info, trace, warn};

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
    /// Cache of torrent statuses for monitoring
    torrent_statuses: Arc<DashMap<u64, TorrentStatus>>,
    /// Handle to the status monitoring task
    monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
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
            torrent_statuses: Arc::new(DashMap::new()),
            monitor_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Start the background status monitoring task
    fn start_status_monitoring(&self) {
        let api_client = Arc::clone(&self.api_client);
        let statuses = Arc::clone(&self.torrent_statuses);
        let poll_interval = self.config.monitoring.status_poll_interval;
        let stalled_timeout = Duration::from_secs(self.config.monitoring.stalled_timeout);

        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(poll_interval));
            
            loop {
                ticker.tick().await;
                
                // Get list of torrents to monitor
                let torrent_ids: Vec<u64> = statuses.iter().map(|e| *e.key()).collect();
                
                for torrent_id in torrent_ids {
                    match api_client.get_torrent_stats(torrent_id).await {
                        Ok(stats) => {
                            // Try to get piece bitfield
                            let bitfield_result = api_client.get_piece_bitfield(torrent_id).await.ok();
                            
                            let mut new_status = TorrentStatus::new(torrent_id, &stats, bitfield_result.as_ref());
                            
                            // Check if torrent appears stalled
                            if let Some(existing) = statuses.get(&torrent_id) {
                                let time_since_update = existing.last_updated.elapsed();
                                if time_since_update > stalled_timeout && !new_status.is_complete() {
                                    new_status.state = TorrentState::Stalled;
                                }
                            }
                            
                            statuses.insert(torrent_id, new_status);
                            trace!("Updated status for torrent {}", torrent_id);
                        }
                        Err(e) => {
                            warn!("Failed to get stats for torrent {}: {}", torrent_id, e);
                            // Mark as error if we can't get stats
                            if let Some(mut status) = statuses.get_mut(&torrent_id) {
                                status.state = TorrentState::Error;
                            }
                        }
                    }
                }
            }
        });

        if let Ok(mut h) = self.monitor_handle.lock() {
            *h = Some(handle);
        }

        info!("Started status monitoring with {} second poll interval", poll_interval);
    }

    /// Stop the status monitoring task
    fn stop_status_monitoring(&self) {
        if let Ok(mut handle) = self.monitor_handle.lock() {
            if let Some(h) = handle.take() {
                h.abort();
                info!("Stopped status monitoring");
            }
        }
    }

    /// Get the current status of a torrent
    pub fn get_torrent_status(&self, torrent_id: u64) -> Option<TorrentStatus> {
        self.torrent_statuses.get(&torrent_id).map(|s| s.clone())
    }

    /// Add a torrent to status monitoring
    pub fn monitor_torrent(&self, torrent_id: u64, initial_status: TorrentStatus) {
        self.torrent_statuses.insert(torrent_id, initial_status);
        debug!("Started monitoring torrent {}", torrent_id);
    }

    /// Remove a torrent from status monitoring
    pub fn unmonitor_torrent(&self, torrent_id: u64) {
        self.torrent_statuses.remove(&torrent_id);
        debug!("Stopped monitoring torrent {}", torrent_id);
    }

    /// Get all monitored torrent statuses
    pub fn list_torrent_statuses(&self) -> Vec<TorrentStatus> {
        self.torrent_statuses.iter().map(|e| e.clone()).collect()
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

    /// Remove a file (or torrent directory).
    /// This allows removing torrents by unlinking their root directory from the mount point.
    /// Individual files cannot be removed (read-only).
    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let name_str = name.to_string_lossy();
        debug!("unlink: parent={}, name={}", parent, name_str);

        // Only allow unlinking torrent directories from root
        if parent != 1 {
            debug!("unlink: rejecting - can only remove torrent directories from root");
            reply.error(libc::EROFS);
            return;
        }

        // Look up the torrent directory by name
        let path = format!("/{}", name_str);
        let ino = match self.inode_manager.lookup_by_path(&path) {
            Some(ino) => ino,
            None => {
                debug!("unlink: torrent directory '{}' not found", name_str);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Verify this is a torrent directory
        let torrent_id = match self.inode_manager.get(ino) {
            Some(entry) => {
                if !entry.is_directory() {
                    debug!("unlink: entry is not a directory");
                    reply.error(libc::ENOTDIR);
                    return;
                }
                // Find the torrent ID
                match self.inode_manager.torrent_to_inode()
                    .iter()
                    .find(|item| *item.value() == ino)
                    .map(|item| *item.key()) {
                    Some(id) => id,
                    None => {
                        warn!("unlink: no torrent ID found for inode {}", ino);
                        reply.error(libc::EIO);
                        return;
                    }
                }
            }
            None => {
                debug!("unlink: inode {} not found", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check for open file handles in this torrent
        let has_open_handles = {
            let read_states = self.read_states.lock().unwrap();
            // Check if any file in this torrent has read state
            let file_inodes: Vec<u64> = self.inode_manager.get_children(ino)
                .iter()
                .filter(|(_, entry)| entry.is_file())
                .map(|(ino, _)| *ino)
                .collect();
            
            file_inodes.iter().any(|file_ino| read_states.contains_key(file_ino))
        };

        if has_open_handles {
            warn!("unlink: torrent {} has open file handles, cannot remove", torrent_id);
            reply.error(libc::EBUSY);
            return;
        }

        // Perform the removal
        if let Err(e) = self.remove_torrent(torrent_id, ino) {
            error!("unlink: failed to remove torrent {}: {}", torrent_id, e);
            reply.error(libc::EIO);
            return;
        }

        info!("Successfully removed torrent {} ({})", torrent_id, name_str);
        reply.ok();
    }

    /// Get extended attribute value.
    /// Exposes torrent status information via extended attributes.
    fn getxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        let name_str = name.to_string_lossy();
        debug!("getxattr: ino={}, name={}", ino, name_str);

        // Only support the "user.torrent.status" attribute
        if name_str != "user.torrent.status" {
            reply.error(libc::ENOATTR);
            return;
        }

        // Get the torrent ID for this inode
        let torrent_id = match self.inode_manager.get(ino) {
            Some(entry) => match entry {
                InodeEntry::File { torrent_id, .. } => torrent_id,
                InodeEntry::Directory { .. } => {
                    // For directories, try to find torrent_id by looking up which torrent maps to this inode
                    self.inode_manager.torrent_to_inode()
                        .iter()
                        .find(|item| *item.value() == ino)
                        .map(|item| *item.key())
                        .unwrap_or(0)
                }
            },
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if torrent_id == 0 {
            // This directory is not associated with a torrent (e.g., subdirectory)
            reply.error(libc::ENOATTR);
            return;
        }

        // Get the status
        match self.torrent_statuses.get(&torrent_id) {
            Some(status) => {
                let json = status.to_json();
                let data = json.as_bytes();

                if size == 0 {
                    // Return the size needed
                    reply.size(data.len() as u32);
                } else if data.len() <= size as usize {
                    // Return the data
                    reply.data(data);
                } else {
                    // Buffer too small
                    reply.error(libc::ERANGE);
                }
            }
            None => {
                // Torrent not being monitored yet, return empty status
                let json = format!(r#"{{"torrent_id":{},"state":"unknown"}}"#, torrent_id);
                let data = json.as_bytes();

                if size == 0 {
                    reply.size(data.len() as u32);
                } else {
                    reply.data(data);
                }
            }
        }
    }

    /// List extended attributes.
    fn listxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!("listxattr: ino={}", ino);

        // Check if inode exists
        if self.inode_manager.get(ino).is_none() {
            reply.error(libc::ENOENT);
            return;
        }

        // The only attribute we support
        let attr_list = "user.torrent.status\0";
        let data = attr_list.as_bytes();

        if size == 0 {
            reply.size(data.len() as u32);
        } else if data.len() <= size as usize {
            reply.data(data);
        } else {
            reply.error(libc::ERANGE);
        }
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

        // Start the background status monitoring task
        self.start_status_monitoring();

        self.initialized = true;
        info!("torrent-fuse filesystem initialized successfully");

        Ok(())
    }

    /// Clean up filesystem.
    /// Called on unmount.
    fn destroy(&mut self) {
        info!("Shutting down torrent-fuse filesystem");
        self.initialized = false;
        // Stop the status monitoring task
        self.stop_status_monitoring();
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

/// Torrent addition flow implementation
impl TorrentFS {
    /// Adds a torrent from a magnet link and creates the filesystem structure.
    /// Returns the torrent ID if successful.
    pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<u64> {
        // First, add the torrent to rqbit
        let response = self
            .api_client
            .add_torrent_magnet(magnet_link)
            .await
            .context("Failed to add torrent from magnet link")?;

        info!(
            "Added torrent {} with hash {}",
            response.id, response.info_hash
        );

        // Check for duplicate torrent
        if self.inode_manager.lookup_torrent(response.id).is_some() {
            warn!("Torrent {} already exists in filesystem, skipping structure creation", response.id);
            return Ok(response.id);
        }

        // Get torrent details to build the file structure
        let torrent_info = self
            .api_client
            .get_torrent(response.id)
            .await
            .context("Failed to get torrent details after adding")?;

        // Create the filesystem structure
        self.create_torrent_structure(&torrent_info)
            .context("Failed to create filesystem structure for torrent")?;

        Ok(response.id)
    }

    /// Adds a torrent from a torrent file URL and creates the filesystem structure.
    /// Returns the torrent ID if successful.
    pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<u64> {
        // First, add the torrent to rqbit
        let response = self
            .api_client
            .add_torrent_url(torrent_url)
            .await
            .context("Failed to add torrent from URL")?;

        info!(
            "Added torrent {} with hash {}",
            response.id, response.info_hash
        );

        // Check for duplicate torrent
        if self.inode_manager.lookup_torrent(response.id).is_some() {
            warn!("Torrent {} already exists in filesystem, skipping structure creation", response.id);
            return Ok(response.id);
        }

        // Get torrent details to build the file structure
        let torrent_info = self
            .api_client
            .get_torrent(response.id)
            .await
            .context("Failed to get torrent details after adding")?;

        // Create the filesystem structure
        self.create_torrent_structure(&torrent_info)
            .context("Failed to create filesystem structure for torrent")?;

        Ok(response.id)
    }

    /// Creates the filesystem directory structure for a torrent.
    fn create_torrent_structure(
        &self,
        torrent_info: &crate::api::types::TorrentInfo,
    ) -> Result<()> {
        use std::collections::HashMap;

        // Sanitize torrent name for use as directory name
        let torrent_name = sanitize_filename(&torrent_info.name);
        let torrent_id = torrent_info.id;

        debug!(
            "Creating filesystem structure for torrent {}: {}",
            torrent_id, torrent_name
        );

        // Create the root directory for this torrent
        let torrent_dir_inode = self
            .inode_manager
            .allocate_torrent_directory(torrent_id, torrent_name.clone(), 1);

        // Add torrent directory to root's children
        self.inode_manager.add_child(1, torrent_dir_inode);

        // Track created directories to avoid duplicates
        let mut created_dirs: HashMap<String, u64> = HashMap::new();
        created_dirs.insert("".to_string(), torrent_dir_inode);

        // Process each file in the torrent
        for (file_idx, file_info) in torrent_info.files.iter().enumerate() {
            self.create_file_entry(
                file_info,
                file_idx,
                torrent_id,
                torrent_dir_inode,
                &mut created_dirs,
            )?;
        }

        // Start monitoring this torrent's status
        // Create an initial unknown status that will be updated by the monitoring task
        let initial_status = TorrentStatus {
            torrent_id,
            state: TorrentState::Unknown,
            progress_pct: 0.0,
            progress_bytes: 0,
            total_bytes: torrent_info.files.iter().map(|f| f.length).sum(),
            downloaded_pieces: 0,
            total_pieces: 0,
            last_updated: Instant::now(),
        };
        self.monitor_torrent(torrent_id, initial_status);

        info!(
            "Created filesystem structure for torrent {} with {} files",
            torrent_id,
            torrent_info.files.len()
        );

        Ok(())
    }

    /// Creates a file entry (and any necessary parent directories) for a torrent file.
    fn create_file_entry(
        &self,
        file_info: &crate::api::types::FileInfo,
        file_idx: usize,
        torrent_id: u64,
        torrent_dir_inode: u64,
        created_dirs: &mut std::collections::HashMap<String, u64>,
    ) -> Result<()> {
        let components = &file_info.components;

        if components.is_empty() {
            return Ok(());
        }

        // Build parent directories
        let mut current_dir_inode = torrent_dir_inode;
        let mut current_path = String::new();

        // Process all components except the last one (which is the filename)
        for dir_component in components.iter().take(components.len().saturating_sub(1)) {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(dir_component);

            // Check if this directory already exists
            if let Some(&inode) = created_dirs.get(&current_path) {
                current_dir_inode = inode;
            } else {
                // Create new directory
                let dir_name = sanitize_filename(dir_component);
                let new_dir_inode = self.inode_manager.allocate(InodeEntry::Directory {
                    ino: 0, // Will be assigned
                    name: dir_name.clone(),
                    parent: current_dir_inode,
                    children: Vec::new(),
                });

                // Add to parent
                self.inode_manager.add_child(current_dir_inode, new_dir_inode);

                created_dirs.insert(current_path.clone(), new_dir_inode);
                current_dir_inode = new_dir_inode;

                debug!("Created directory {} at inode {}", current_path, new_dir_inode);
            }
        }

        // Create the file entry
        let file_name = components.last().unwrap();
        let sanitized_name = sanitize_filename(file_name);

        let file_inode = self.inode_manager.allocate_file(
            sanitized_name,
            current_dir_inode,
            torrent_id,
            file_idx,
            file_info.length,
        );

        // Add to parent directory
        self.inode_manager.add_child(current_dir_inode, file_inode);

        debug!(
            "Created file {} at inode {} (size: {})",
            file_name, file_inode, file_info.length
        );

        Ok(())
    }

    /// Checks if a torrent is already in the filesystem.
    pub fn has_torrent(&self, torrent_id: u64) -> bool {
        self.inode_manager.lookup_torrent(torrent_id).is_some()
    }

    /// Gets the list of torrent IDs currently in the filesystem.
    pub fn list_torrents(&self) -> Vec<u64> {
        self.inode_manager.get_all_torrent_ids()
    }

    /// Remove a torrent from the filesystem and rqbit.
    /// 
    /// This method:
    /// 1. Stops monitoring the torrent
    /// 2. Removes the torrent from rqbit (forget - keeps files)
    /// 3. Removes all inodes associated with the torrent
    /// 4. Removes the torrent directory from root's children
    fn remove_torrent(&self, torrent_id: u64, torrent_inode: u64) -> Result<()> {
        debug!("Removing torrent {} (inode {})", torrent_id, torrent_inode);

        // Stop monitoring this torrent
        self.unmonitor_torrent(torrent_id);

        // Remove from rqbit (forget - keeps downloaded files)
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.api_client.forget_torrent(torrent_id).await
            })
        }).with_context(|| format!("Failed to remove torrent {} from rqbit", torrent_id))?;

        // Remove torrent directory from root's children list
        self.inode_manager.remove_child(1, torrent_inode);

        // Remove all inodes associated with this torrent (recursively)
        self.inode_manager.remove_inode(torrent_inode);

        info!("Successfully removed torrent {} from filesystem", torrent_id);
        Ok(())
    }

    /// Removes a torrent by its ID.
    /// Convenience method that finds the inode and calls remove_torrent.
    pub fn remove_torrent_by_id(&self, torrent_id: u64) -> Result<()> {
        let torrent_inode = self.inode_manager.lookup_torrent(torrent_id)
            .ok_or_else(|| anyhow::anyhow!("Torrent {} not found in filesystem", torrent_id))?;
        
        self.remove_torrent(torrent_id, torrent_inode)
    }
}

/// Sanitizes a filename for use in the filesystem.
/// Removes or replaces characters that are problematic in filenames.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            // Null character
            '\0' => '_',
            // Path separators
            '/' | '\\' => '_',
            // Control characters
            c if c.is_control() => '_',
            // Other problematic characters
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
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

    #[test]
    fn test_remove_torrent_cleans_up_inodes() {
        let config = Config::default();
        let fs = TorrentFS::new(config).unwrap();

        // Create a torrent structure manually
        let torrent_id = 123u64;
        let torrent_inode = fs.inode_manager.allocate_torrent_directory(
            torrent_id,
            "test_torrent".to_string(),
            1
        );
        fs.inode_manager.add_child(1, torrent_inode);

        // Add a file to the torrent
        let file_inode = fs.inode_manager.allocate_file(
            "test.txt".to_string(),
            torrent_inode,
            torrent_id,
            0,
            1024,
        );
        fs.inode_manager.add_child(torrent_inode, file_inode);

        // Verify structures exist
        assert!(fs.inode_manager.get(torrent_inode).is_some());
        assert!(fs.inode_manager.get(file_inode).is_some());
        assert!(fs.inode_manager.lookup_torrent(torrent_id).is_some());

        // Remove the torrent (this would normally call rqbit API)
        // Since we can't call the API in tests, we manually clean up
        fs.inode_manager.remove_child(1, torrent_inode);
        fs.inode_manager.remove_inode(torrent_inode);

        // Verify structures are cleaned up
        assert!(fs.inode_manager.get(torrent_inode).is_none());
        assert!(fs.inode_manager.get(file_inode).is_none());
        assert!(fs.inode_manager.lookup_torrent(torrent_id).is_none());
        
        // Verify torrent is no longer in root's children
        let root_children = fs.inode_manager.get_children(1);
        assert!(!root_children.iter().any(|(ino, _)| *ino == torrent_inode));
    }
}
