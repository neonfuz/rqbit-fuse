use crate::api::client::RqbitClient;
use crate::api::types::{TorrentState, TorrentStatus};
use crate::config::Config;
use crate::fs::inode::InodeManager;
use crate::metrics::Metrics;
use crate::types::inode::InodeEntry;
use anyhow::{Context, Result};
use dashmap::DashMap;
use fuser::Filesystem;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, error, info, instrument, trace, warn};

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
    /// Metrics collection
    metrics: Arc<Metrics>,
}

impl TorrentFS {
    /// Creates a new TorrentFS instance with the given configuration.
    /// Note: This does not initialize the filesystem - call mount() to do so.
    pub fn new(config: Config, metrics: Arc<Metrics>) -> Result<Self> {
        let api_client = Arc::new(RqbitClient::new(
            config.api.url.clone(),
            Arc::clone(&metrics.api),
        ));
        let inode_manager = Arc::new(InodeManager::new());

        Ok(Self {
            config,
            api_client,
            inode_manager,
            initialized: false,
            read_states: Arc::new(Mutex::new(HashMap::new())),
            torrent_statuses: Arc::new(DashMap::new()),
            monitor_handle: Arc::new(Mutex::new(None)),
            metrics,
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
                            let bitfield_result =
                                api_client.get_piece_bitfield(torrent_id).await.ok();

                            let mut new_status =
                                TorrentStatus::new(torrent_id, &stats, bitfield_result.as_ref());

                            // Check if torrent appears stalled
                            if let Some(existing) = statuses.get(&torrent_id) {
                                let time_since_update = existing.last_updated.elapsed();
                                if time_since_update > stalled_timeout && !new_status.is_complete()
                                {
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

        info!(
            "Started status monitoring with {} second poll interval",
            poll_interval
        );
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
            Ok(false) => Err(anyhow::anyhow!(
                "rqbit server at {} is not responding or returned an error",
                self.config.api.url
            )),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to connect to rqbit server at {}: {}",
                self.config.api.url,
                e
            )),
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
        fuser::mount2(self, &mount_point, &options)
            .with_context(|| format!("Failed to mount filesystem at: {}", mount_point.display()))
    }

    /// Check if the requested data range has all pieces available.
    /// Returns true if all pieces in the range are downloaded.
    #[allow(dead_code)]
    fn check_pieces_available(
        &self,
        torrent_id: u64,
        offset: u64,
        size: u64,
        piece_length: u64,
    ) -> bool {
        // If piece checking is disabled, assume all pieces are available
        if !self.config.performance.piece_check_enabled {
            return true;
        }

        // Get the status to check piece availability
        if let Some(status) = self.torrent_statuses.get(&torrent_id) {
            // If torrent is complete, all pieces are available
            if status.is_complete() {
                return true;
            }

            // Calculate piece indices for the requested range
            let start_piece = offset / piece_length;
            let end_piece = ((offset + size - 1) / piece_length) + 1;

            // If we have no piece information, assume not available
            if status.total_pieces == 0 {
                return false;
            }

            // Check if we have enough pieces downloaded
            // This is a simplified check - ideally we'd check the actual bitfield
            // For now, use progress percentage as approximation
            let progress = status.progress_pct / 100.0;
            let pieces_needed = end_piece - start_piece;
            let pieces_available = (status.total_pieces as f64 * progress) as u64;

            // Conservative check: require more pieces to be available than needed
            pieces_available >= pieces_needed
        } else {
            // No status available, assume not ready
            false
        }
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
        let state = read_states
            .entry(ino)
            .or_insert_with(|| ReadState::new(offset, size));

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
                    file_size - next_offset,
                ) as usize;

                if prefetch_size > 0 {
                    state.is_prefetching = true;
                    drop(read_states); // Release lock before async operation

                    let api_client = Arc::clone(&self.api_client);
                    let read_states = Arc::clone(&self.read_states);
                    let readahead_size = self.config.performance.readahead_size;

                    // Spawn prefetch in background
                    tokio::spawn(async move {
                        let prefetch_end =
                            std::cmp::min(next_offset + readahead_size - 1, file_size - 1);



                        match api_client
                            .read_file(torrent_id, file_index, Some((next_offset, prefetch_end)))
                            .await
                        {
                            Ok(_data) => {
    
                                // Could store in cache here
                            }
                            Err(_e) => {
    
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
            fuser::MountOption::RO,      // Read-only (torrents are read-only)
            fuser::MountOption::NoSuid,  // No setuid/setgid
            fuser::MountOption::NoDev,   // No special device files
            fuser::MountOption::NoAtime, // Don't update access times
            fuser::MountOption::Sync,    // Synchronous writes (safer for FUSE)
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
    ///
    /// # Arguments
    /// * `entry` - The inode entry to build attributes for
    ///
    /// # Returns
    /// * `fuser::FileAttr` - The FUSE file attributes
    pub fn build_file_attr(&self, entry: &crate::types::inode::InodeEntry) -> fuser::FileAttr {
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
            InodeEntry::Symlink { ino, target, .. } => fuser::FileAttr {
                ino: *ino,
                size: target.len() as u64,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: creation_time,
                kind: fuser::FileType::Symlink,
                perm: 0o777, // Symlinks always have 777 permissions
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
    /// Read file contents.
    /// Called when the kernel needs to read data from a file.
    /// Translates FUSE read requests to HTTP Range requests to rqbit.
    #[instrument(skip(self, reply), fields(ino))]
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
        let start_time = Instant::now();

        // Clamp read size to FUSE maximum to prevent "Too much data" panic
        let size = std::cmp::min(size, Self::FUSE_MAX_READ);

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "read", ino = ino, offset = offset, size = size);
        }

        // Validate offset is non-negative
        if offset < 0 {
            self.metrics.fuse.record_error();

            if self.config.logging.log_fuse_operations {
                debug!(
                    fuse_op = "read",
                    ino = ino,
                    result = "error",
                    error = "EINVAL",
                    reason = "negative_offset"
                );
            }

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
                    self.metrics.fuse.record_error();

                    if self.config.logging.log_fuse_operations {
                        debug!(
                            fuse_op = "read",
                            ino = ino,
                            result = "error",
                            error = "EISDIR"
                        );
                    }

                    reply.error(libc::EISDIR);
                    return;
                }
            },
            None => {
                self.metrics.fuse.record_error();

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "read",
                        ino = ino,
                        result = "error",
                        error = "ENOENT"
                    );
                }

                reply.error(libc::ENOENT);
                return;
            }
        };

        // Handle zero-byte reads
        if size == 0 || offset >= file_size {
            if self.config.logging.log_fuse_operations {
                debug!(
                    fuse_op = "read",
                    ino = ino,
                    result = "success",
                    bytes_read = 0,
                    reason = "empty_read"
                );
            }

            reply.data(&[]);
            return;
        }

        // Calculate actual read range (don't read past EOF)
        // Use saturating_sub to prevent underflow when offset == file_size
        let end = std::cmp::min(offset + size as u64, file_size).saturating_sub(1);

        if self.config.logging.log_fuse_operations {
            debug!(
                fuse_op = "read",
                ino = ino,
                torrent_id = torrent_id,
                file_index = file_index,
                range_start = offset,
                range_end = end
            );
        }

        // Check if we should return EAGAIN for unavailable pieces
        if self.config.performance.return_eagain_for_unavailable {
            if let Some(status) = self.torrent_statuses.get(&torrent_id) {
                // If torrent hasn't started (0 progress) or is in error state, return EAGAIN
                if status.progress_bytes == 0
                    || status.state == crate::api::types::TorrentState::Error
                {
                    if self.config.logging.log_fuse_operations {
                        debug!(
                            fuse_op = "read",
                            ino = ino,
                            torrent_id = torrent_id,
                            result = "error",
                            error = "EAGAIN",
                            reason = "torrent_not_ready"
                        );
                    }

                    reply.error(libc::EAGAIN);
                    return;
                }
            } else {
                // No status available, torrent not monitored yet
                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "read",
                        ino = ino,
                        torrent_id = torrent_id,
                        result = "error",
                        error = "EAGAIN",
                        reason = "torrent_not_monitored"
                    );
                }

                reply.error(libc::EAGAIN);
                return;
            }
        }

        // Perform the read using persistent streaming for efficient sequential access
        // This maintains open HTTP connections and reuses them, significantly improving
        // performance when rqbit ignores Range headers and returns full file responses.
        //
        // Note: The persistent stream manager handles connection lifecycle internally,
        // including idle timeouts and automatic cleanup.
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Set a timeout to avoid blocking forever on slow pieces
                let timeout_duration =
                    std::time::Duration::from_secs(self.config.performance.read_timeout);
                tokio::time::timeout(
                    timeout_duration,
                    self.api_client
                        .read_file_streaming(torrent_id, file_index, offset, size as usize),
                )
                .await
            })
        });

        let latency = start_time.elapsed();

        match result {
            Ok(Ok(data)) => {
                let bytes_read = data.len() as u64;
                self.metrics.fuse.record_read(bytes_read, latency);

                // Log slow reads at debug level only
                if latency > std::time::Duration::from_secs(1) {
                    debug!(
                        fuse_op = "read",
                        ino = ino,
                        torrent_id = torrent_id,
                        latency_ms = latency.as_millis() as u64,
                        "Slow read detected"
                    );
                }

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "read",
                        ino = ino,
                        result = "success",
                        bytes_read = bytes_read,
                        latency_ms = latency.as_millis() as u64
                    );
                }

                // Track read pattern and trigger prefetch if sequential
                self.track_and_prefetch(ino, offset, size, file_size, torrent_id, file_index);

                // Truncate data to requested size to prevent "Too much data" FUSE panic
                // The API might return more data than requested (e.g., entire piece)
                let data_slice = if data.len() > size as usize {
                    warn!(
                        fuse_op = "read",
                        ino = ino,
                        api_response_bytes = data.len(),
                        requested_size = size,
                        "Truncating API response to requested size"
                    );
                    &data[..size as usize]
                } else {
                    &data[..]
                };
                reply.data(data_slice);
            }
            Ok(Err(e)) => {
                self.metrics.fuse.record_error();

                error!(
                    fuse_op = "read",
                    ino = ino,
                    torrent_id = torrent_id,
                    file_index = file_index,
                    error = %e,
                    "Failed to read file"
                );

                // Try to extract ApiError from anyhow error and use proper mapping
                let error_code =
                    if let Some(api_err) = e.downcast_ref::<crate::api::types::ApiError>() {
                        api_err.to_fuse_error()
                    } else if e.to_string().contains("not found") {
                        libc::ENOENT
                    } else if e.to_string().contains("range") {
                        libc::EINVAL
                    } else {
                        libc::EIO
                    };

                reply.error(error_code);
            }
            Err(_) => {
                // Timeout occurred
                self.metrics.fuse.record_error();

                warn!(
                    fuse_op = "read",
                    ino = ino,
                    torrent_id = torrent_id,
                    file_index = file_index,
                    "Timeout reading file (slow piece download)"
                );

                // Return EAGAIN to indicate the operation should be retried
                reply.error(libc::EAGAIN);
            }
        }
    }

    /// Release an open file.
    /// Called when a file is closed. No special cleanup needed since we read on-demand.
    #[instrument(skip(self, reply), fields(ino))]
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
        self.metrics.fuse.record_release();

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "release", ino = _ino);
        }

        reply.ok();
    }

    /// Look up a directory entry by name.
    /// Called when the kernel needs to resolve a path component to an inode.
    #[instrument(skip(self, reply, name), fields(parent))]
    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        self.metrics.fuse.record_lookup();

        let name_str = name.to_string_lossy();

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "lookup", parent = parent, name = %name_str);
        }

        // Get the parent directory entry
        let parent_entry = match self.inode_manager.get(parent) {
            Some(entry) => entry,
            None => {
                self.metrics.fuse.record_error();

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "lookup",
                        parent = parent,
                        result = "error",
                        error = "ENOENT"
                    );
                }

                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if parent is a directory
        if !parent_entry.is_directory() {
            self.metrics.fuse.record_error();

            if self.config.logging.log_fuse_operations {
                debug!(
                    fuse_op = "lookup",
                    parent = parent,
                    result = "error",
                    error = "ENOTDIR"
                );
            }

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

                        if self.config.logging.log_fuse_operations {
                            debug!(fuse_op = "lookup", parent = parent, name = %name_str, ino = ino, result = "success");
                        }
                    }
                    None => {
                        // This shouldn't happen - path maps to non-existent inode
                        self.metrics.fuse.record_error();

                        error!(
                            fuse_op = "lookup",
                            path = %path,
                            ino = ino,
                            "Path maps to missing inode"
                        );

                        reply.error(libc::EIO);
                    }
                }
            }
            None => {
                if self.config.logging.log_fuse_operations {
                    debug!(fuse_op = "lookup", parent = parent, name = %name_str, result = "not_found");
                }

                reply.error(libc::ENOENT);
            }
        }
    }

    /// Get file attributes.
    /// Called when the kernel needs to get attributes for a file or directory.
    /// This is a fundamental operation used by ls, stat, and most file operations.
    #[instrument(skip(self, reply), fields(ino))]
    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        reply: fuser::ReplyAttr,
    ) {
        self.metrics.fuse.record_getattr();

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "getattr", ino = ino);
        }

        // Get the inode entry
        match self.inode_manager.get(ino) {
            Some(entry) => {
                let attr = self.build_file_attr(&entry);
                let ttl = std::time::Duration::from_secs(1);

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "getattr",
                        ino = ino,
                        result = "success",
                        kind = ?attr.kind,
                        size = attr.size
                    );
                }

                reply.attr(&ttl, &attr);
            }
            None => {
                self.metrics.fuse.record_error();

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "getattr",
                        ino = ino,
                        result = "error",
                        error = "ENOENT"
                    );
                }

                reply.error(libc::ENOENT);
            }
        }
    }

    /// Open a file.
    /// Called when the kernel needs to open a file for reading.
    /// Returns a file handle that will be used in subsequent read operations.
    #[instrument(skip(self, reply), fields(ino))]
    fn open(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        self.metrics.fuse.record_open();

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "open", ino = ino, flags = flags);
        }

        // Check if the inode exists
        match self.inode_manager.get(ino) {
            Some(entry) => {
                // Check if it's a file (not a directory)
                if entry.is_directory() {
                    self.metrics.fuse.record_error();

                    if self.config.logging.log_fuse_operations {
                        debug!(
                            fuse_op = "open",
                            ino = ino,
                            result = "error",
                            error = "EISDIR"
                        );
                    }

                    reply.error(libc::EISDIR);
                    return;
                }

                // Check if it's a symlink (symlinks should be resolved before open)
                if entry.is_symlink() {
                    self.metrics.fuse.record_error();

                    if self.config.logging.log_fuse_operations {
                        debug!(
                            fuse_op = "open",
                            ino = ino,
                            result = "error",
                            error = "ELOOP"
                        );
                    }

                    reply.error(libc::ELOOP);
                    return;
                }

                // Check write access - this is a read-only filesystem
                let access_mode = flags & libc::O_ACCMODE;
                if access_mode != libc::O_RDONLY {
                    self.metrics.fuse.record_error();

                    if self.config.logging.log_fuse_operations {
                        debug!(
                            fuse_op = "open",
                            ino = ino,
                            result = "error",
                            error = "EACCES",
                            reason = "write_access_requested"
                        );
                    }

                    reply.error(libc::EACCES);
                    return;
                }

                // Use the inode as the file handle (simple approach)
                let fh = ino;

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "open",
                        ino = ino,
                        result = "success",
                        fh = fh
                    );
                }

                reply.opened(fh, 0);
            }
            None => {
                self.metrics.fuse.record_error();

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "open",
                        ino = ino,
                        result = "error",
                        error = "ENOENT"
                    );
                }

                reply.error(libc::ENOENT);
            }
        }
    }

    /// Read the target of a symbolic link.
    /// Called when the kernel needs to resolve a symlink target.
    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!("readlink: ino={}", ino);

        match self.inode_manager.get(ino) {
            Some(entry) => {
                if let crate::types::inode::InodeEntry::Symlink { target, .. } = entry {
                    reply.data(target.as_bytes());
                    debug!("readlink: resolved symlink to {}", target);
                } else {
                    debug!("readlink: inode {} is not a symlink", ino);
                    reply.error(libc::EINVAL);
                }
            }
            None => {
                debug!("readlink: inode {} not found", ino);
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Read directory entries.
    /// Called when the kernel needs to list the contents of a directory.
    #[instrument(skip(self, reply), fields(ino))]
    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        self.metrics.fuse.record_readdir();

        if self.config.logging.log_fuse_operations {
            debug!(fuse_op = "readdir", ino = ino, offset = offset);
        }

        // Get the directory entry
        let entry = match self.inode_manager.get(ino) {
            Some(e) => e,
            None => {
                self.metrics.fuse.record_error();

                if self.config.logging.log_fuse_operations {
                    debug!(
                        fuse_op = "readdir",
                        ino = ino,
                        result = "error",
                        error = "ENOENT"
                    );
                }

                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if it's a directory
        if !entry.is_directory() {
            self.metrics.fuse.record_error();

            if self.config.logging.log_fuse_operations {
                debug!(
                    fuse_op = "readdir",
                    ino = ino,
                    result = "error",
                    error = "ENOTDIR"
                );
            }

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
            } else if child_entry.is_symlink() {
                fuser::FileType::Symlink
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
    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
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
                match self
                    .inode_manager
                    .torrent_to_inode()
                    .iter()
                    .find(|item| *item.value() == ino)
                    .map(|item| *item.key())
                {
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
            let file_inodes: Vec<u64> = self
                .inode_manager
                .get_children(ino)
                .iter()
                .filter(|(_, entry)| entry.is_file())
                .map(|(ino, _)| *ino)
                .collect();

            file_inodes
                .iter()
                .any(|file_ino| read_states.contains_key(file_ino))
        };

        if has_open_handles {
            warn!(
                "unlink: torrent {} has open file handles, cannot remove",
                torrent_id
            );
            reply.error(libc::EBUSY);
            return;
        }

        // Perform the removal
        if let Err(e) = self.remove_torrent(torrent_id, ino) {
            error!("unlink: failed to remove torrent {}: {}", torrent_id, e);

            // Map error appropriately
            let error_code = if let Some(api_err) = e.downcast_ref::<crate::api::types::ApiError>()
            {
                api_err.to_fuse_error()
            } else {
                libc::EIO
            };

            reply.error(error_code);
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
                    self.inode_manager
                        .torrent_to_inode()
                        .iter()
                        .find(|item| *item.value() == ino)
                        .map(|item| *item.key())
                        .unwrap_or(0)
                }
                InodeEntry::Symlink { .. } => {
                    // Symlinks don't have torrent status
                    reply.error(libc::ENOATTR);
                    return;
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

impl TorrentFS {
    /// Maximum read size for FUSE responses (64KB).
    /// Matches rqbit's internal buffer size for optimal performance.
    /// Benchmarks show 64KB provides best throughput without "Too much data" errors.
    const FUSE_MAX_READ: u32 = 64 * 1024; // 64KB
}

/// Async initialization helper that can be called from the async runtime
/// to perform the full initialization including the rqbit connection check.
pub async fn initialize_filesystem(fs: &mut TorrentFS) -> Result<()> {
    // Check connection to rqbit
    fs.connect_to_rqbit().await?;
    Ok(())
}

/// Discover and populate existing torrents from rqbit.
/// This should be called before mounting to ensure all existing torrents
/// appear in the filesystem.
pub async fn discover_existing_torrents(fs: &TorrentFS) -> Result<()> {
    info!("Discovering existing torrents from rqbit...");

    // Get list of all torrents from rqbit
    let torrents = fs
        .api_client
        .list_torrents()
        .await
        .context("Failed to list torrents from rqbit")?;

    if torrents.is_empty() {
        info!("No existing torrents found in rqbit");
        return Ok(());
    }

    info!(
        "Found {} existing torrents, populating filesystem...",
        torrents.len()
    );

    let mut success_count = 0;
    let mut error_count = 0;

    for torrent_info in torrents {
        // Check if we already have this torrent (avoid duplicates)
        if fs.inode_manager.lookup_torrent(torrent_info.id).is_some() {
            debug!(
                "Torrent {} already exists in filesystem, skipping",
                torrent_info.id
            );
            continue;
        }

        // Create filesystem structure for this torrent
        match fs.create_torrent_structure(&torrent_info) {
            Ok(()) => {
                success_count += 1;
                debug!(
                    "Populated filesystem for torrent {}: {} ({} files)",
                    torrent_info.id,
                    torrent_info.name,
                    torrent_info.files.len()
                );
            }
            Err(e) => {
                error_count += 1;
                warn!(
                    "Failed to create filesystem structure for torrent {} ({}): {}",
                    torrent_info.id, torrent_info.name, e
                );
            }
        }
    }

    info!(
        "Finished discovering torrents: {} successful, {} failed, {} total",
        success_count,
        error_count,
        success_count + error_count
    );

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
            warn!(
                "Torrent {} already exists in filesystem, skipping structure creation",
                response.id
            );
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
            warn!(
                "Torrent {} already exists in filesystem, skipping structure creation",
                response.id
            );
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
    /// For single-file torrents, the file is added directly to root.
    /// For multi-file torrents, a directory is created with the torrent name.
    ///
    /// # Arguments
    /// * `torrent_info` - The torrent metadata from rqbit API
    ///
    /// # Returns
    /// * `Result<()>` - Ok if structure was created successfully
    ///
    /// # Errors
    /// Returns an error if inode allocation fails
    pub fn create_torrent_structure(
        &self,
        torrent_info: &crate::api::types::TorrentInfo,
    ) -> Result<()> {
        use std::collections::HashMap;

        let torrent_name = sanitize_filename(&torrent_info.name);
        let torrent_id = torrent_info.id;

        debug!(
            "Creating filesystem structure for torrent {}: {} ({} files)",
            torrent_id,
            torrent_name,
            torrent_info.files.len()
        );

        // Handle single-file torrents differently - add file directly to root
        if torrent_info.files.len() == 1 {
            let file_info = &torrent_info.files[0];
            let file_name = if file_info.components.is_empty() {
                // Use torrent name as filename if no components provided
                torrent_name.clone()
            } else {
                sanitize_filename(file_info.components.last().unwrap())
            };

            // Create file entry directly under root
            let file_inode = self.inode_manager.allocate_file(
                file_name.clone(),
                1, // parent is root
                torrent_id,
                0, // single file has index 0
                file_info.length,
            );

            // Add to root's children
            self.inode_manager.add_child(1, file_inode);

            // Track torrent mapping
            self.inode_manager
                .torrent_to_inode()
                .insert(torrent_id, file_inode);

            debug!(
                "Created single-file torrent entry {} -> {} (size: {})",
                file_name, file_inode, file_info.length
            );
        } else {
            // Multi-file torrent: create directory structure
            let torrent_dir_inode =
                self.inode_manager
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
        }

        // Start monitoring this torrent's status
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
                self.inode_manager
                    .add_child(current_dir_inode, new_dir_inode);

                created_dirs.insert(current_path.clone(), new_dir_inode);
                current_dir_inode = new_dir_inode;

                debug!(
                    "Created directory {} at inode {}",
                    current_path, new_dir_inode
                );
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
            tokio::runtime::Handle::current()
                .block_on(async { self.api_client.forget_torrent(torrent_id).await })
        })
        .with_context(|| format!("Failed to remove torrent {} from rqbit", torrent_id))?;

        // Remove torrent directory from root's children list
        self.inode_manager.remove_child(1, torrent_inode);

        // Remove all inodes associated with this torrent (recursively)
        self.inode_manager.remove_inode(torrent_inode);

        info!(
            "Successfully removed torrent {} from filesystem",
            torrent_id
        );
        Ok(())
    }

    /// Removes a torrent by its ID.
    /// Convenience method that finds the inode and calls remove_torrent.
    pub fn remove_torrent_by_id(&self, torrent_id: u64) -> Result<()> {
        let torrent_inode = self
            .inode_manager
            .lookup_torrent(torrent_id)
            .ok_or_else(|| anyhow::anyhow!("Torrent {} not found in filesystem", torrent_id))?;

        self.remove_torrent(torrent_id, torrent_inode)
    }
}

/// Sanitizes a filename for use in the filesystem.
/// Removes or replaces characters that are problematic in filenames.
/// Also prevents path traversal attacks by removing ".." components.
fn sanitize_filename(name: &str) -> String {
    // Replace path traversal sequences first
    let name = name.replace("..", "_");

    // Remove leading/trailing whitespace and dots
    let trimmed = name.trim().trim_start_matches('.').trim_end_matches('.');

    if trimmed.is_empty() {
        return "unnamed".to_string();
    }

    trimmed
        .chars()
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

/// Validates that a path component doesn't contain path traversal sequences.
/// Returns true if the component is safe to use.
#[allow(dead_code)]
pub(crate) fn is_safe_path_component(component: &str) -> bool {
    // Reject empty components, current dir, parent dir references
    if component.is_empty() || component == "." || component == ".." || component.contains("..") {
        return false;
    }

    // Reject components with path separators
    if component.contains('/') || component.contains('\\') {
        return false;
    }

    // Reject components starting with null bytes or control characters
    if component.starts_with('\0')
        || component
            .chars()
            .next()
            .map(|c| c.is_control())
            .unwrap_or(false)
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_torrent_fs_creation() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        assert!(!fs.is_initialized());
        assert_eq!(fs.inode_manager().get(1).unwrap().ino(), 1);
    }

    #[test]
    fn test_validate_mount_point_success() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.mount.mount_point = temp_dir.path().to_path_buf();

        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();
        assert!(fs.validate_mount_point().is_ok());
    }

    #[test]
    fn test_validate_mount_point_nonexistent() {
        let mut config = Config::default();
        config.mount.mount_point = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();
        let result = fs.validate_mount_point();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_build_mount_options() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

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
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        let options = fs.build_mount_options();

        assert!(options.contains(&fuser::MountOption::AllowOther));
    }

    #[test]
    fn test_remove_torrent_cleans_up_inodes() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a torrent structure manually
        let torrent_id = 123u64;
        let torrent_inode =
            fs.inode_manager
                .allocate_torrent_directory(torrent_id, "test_torrent".to_string(), 1);
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

    // Edge case tests
    #[test]
    fn test_sanitize_filename_path_traversal() {
        // Path traversal attempts should be neutralized - all separators become _
        assert_eq!(sanitize_filename("../../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize_filename(".."), "_");
        // "../secret" -> "_/secret" -> "__secret"
        assert_eq!(sanitize_filename("../secret"), "__secret");
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        // Special characters should be replaced
        assert_eq!(sanitize_filename("file:name.txt"), "file_name.txt");
        assert_eq!(sanitize_filename("file*name?.txt"), "file_name_.txt");
        // Both < and > are replaced, resulting in double underscore between script tags
        assert_eq!(
            sanitize_filename("<script>alert(1)</script>"),
            "_script_alert(1)__script_"
        );
    }

    #[test]
    fn test_sanitize_filename_control_chars() {
        // Control characters should be replaced
        assert_eq!(sanitize_filename("file\x00name"), "file_name");
        assert_eq!(sanitize_filename("file\nname"), "file_name");
        assert_eq!(sanitize_filename("file\tname"), "file_name");
    }

    #[test]
    fn test_sanitize_filename_leading_dots() {
        // Leading/trailing dots should be removed (prevents hidden files)
        assert_eq!(sanitize_filename(".hidden"), "hidden");
        assert_eq!(sanitize_filename("file."), "file");
        assert_eq!(sanitize_filename("..double"), "_double");
    }

    #[test]
    fn test_sanitize_filename_empty() {
        // Empty names should be replaced with "unnamed"
        assert_eq!(sanitize_filename(""), "unnamed");
        assert_eq!(sanitize_filename("   "), "unnamed");
        // "..." becomes "_." (".." replaced with "_", leaving "."), then trimmed to "_"
        assert_eq!(sanitize_filename("..."), "_");
    }

    #[test]
    fn test_is_safe_path_component() {
        // Safe components
        assert!(is_safe_path_component("normal_file"));
        assert!(is_safe_path_component("file.txt"));
        assert!(is_safe_path_component("my-directory"));

        // Unsafe components
        assert!(!is_safe_path_component(""));
        assert!(!is_safe_path_component("."));
        assert!(!is_safe_path_component(".."));
        assert!(!is_safe_path_component("../.."));
        assert!(!is_safe_path_component("dir/file"));
        assert!(!is_safe_path_component("dir\\file"));
    }

    #[test]
    fn test_symlink_creation() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a symlink
        let symlink_inode =
            fs.inode_manager
                .allocate_symlink("link".to_string(), 1, "/target/path".to_string());

        // Verify symlink exists
        let entry = fs.inode_manager.get(symlink_inode).unwrap();
        assert!(entry.is_symlink());
        assert_eq!(entry.name(), "link");

        // Verify attributes
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.kind, fuser::FileType::Symlink);
        assert_eq!(attr.size, "/target/path".len() as u64);
    }

    #[test]
    fn test_zero_byte_file() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a zero-byte file
        let file_inode = fs.inode_manager.allocate_file(
            "empty.txt".to_string(),
            1,
            1,
            0,
            0, // Zero size
        );

        // Verify file exists
        let entry = fs.inode_manager.get(file_inode).unwrap();
        assert!(entry.is_file());

        // Verify attributes
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.size, 0);
        assert_eq!(attr.blocks, 0);
    }

    #[test]
    fn test_large_file() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a large file (>4GB)
        let large_size = 5u64 * 1024 * 1024 * 1024; // 5 GB
        let file_inode =
            fs.inode_manager
                .allocate_file("large.iso".to_string(), 1, 1, 0, large_size);

        // Verify attributes
        let entry = fs.inode_manager.get(file_inode).unwrap();
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.size, large_size);
        assert!(attr.blocks > 0);
    }

    #[test]
    fn test_unicode_filename() {
        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Test various Unicode filenames
        let unicode_names = vec![
            "文件.txt",       // Chinese
            "ファイル.txt",   // Japanese
            "файл.txt",       // Russian
            "αρχείο.txt",     // Greek
            "📄document.txt", // Emoji
            "naïve.txt",      // Accented
        ];

        for name in unicode_names {
            let inode = fs
                .inode_manager
                .allocate_file(name.to_string(), 1, 1, 0, 100);
            let entry = fs.inode_manager.get(inode).unwrap();
            assert_eq!(entry.name(), name);
        }
    }

    #[test]
    fn test_single_file_torrent_structure() {
        use crate::api::types::{FileInfo, TorrentInfo};

        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a single-file torrent info
        let torrent_info = TorrentInfo {
            id: 1,
            info_hash: "abc123".to_string(),
            name: "Single File".to_string(),
            output_folder: "/tmp".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: "file.txt".to_string(),
                length: 1024,
                components: vec!["file.txt".to_string()],
            }],
            piece_length: Some(262144),
        };

        // Create structure
        fs.create_torrent_structure(&torrent_info).unwrap();

        // Verify file was added directly to root (no directory)
        let root_children = fs.inode_manager.get_children(1);
        assert_eq!(root_children.len(), 1);

        let (inode, entry) = &root_children[0];
        assert!(entry.is_file());
        assert_eq!(entry.name(), "file.txt");

        // Verify torrent mapping points to file
        let torrent_inode = fs.inode_manager.lookup_torrent(1).unwrap();
        assert_eq!(torrent_inode, *inode);
    }

    #[test]
    fn test_multi_file_torrent_structure() {
        use crate::api::types::{FileInfo, TorrentInfo};

        let config = Config::default();
        let metrics = Arc::new(crate::metrics::Metrics::new());
        let fs = TorrentFS::new(config, metrics).unwrap();

        // Create a multi-file torrent info
        let torrent_info = TorrentInfo {
            id: 2,
            info_hash: "def456".to_string(),
            name: "Multi File".to_string(),
            output_folder: "/tmp".to_string(),
            file_count: Some(2),
            files: vec![
                FileInfo {
                    name: "file1.txt".to_string(),
                    length: 1024,
                    components: vec!["file1.txt".to_string()],
                },
                FileInfo {
                    name: "file2.txt".to_string(),
                    length: 2048,
                    components: vec!["subdir".to_string(), "file2.txt".to_string()],
                },
            ],
            piece_length: Some(262144),
        };

        // Create structure
        fs.create_torrent_structure(&torrent_info).unwrap();

        // Verify directory was created
        let root_children = fs.inode_manager.get_children(1);
        assert_eq!(root_children.len(), 1);

        let (_dir_inode, entry) = &root_children[0];
        assert!(entry.is_directory());
        assert_eq!(entry.name(), "Multi File");
    }
}
