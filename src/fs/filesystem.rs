use crate::api::client::RqbitClient;
use crate::api::create_api_client;

use crate::config::Config;
use crate::fs::async_bridge::AsyncFuseWorker;
use crate::fs::inode::InodeEntry;
use crate::fs::inode::InodeManager;
use crate::fs::macros::{
    fuse_error, reply_ino_not_found, reply_no_permission, reply_not_directory, reply_not_file,
};
use crate::metrics::Metrics;
use crate::types::handle::FileHandleManager;
use anyhow::{Context, Result};
use dashmap::DashSet;
use fuser::Filesystem;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::interval;
use tracing::{debug, error, info, instrument, trace, warn};

// Platform-specific error code for "no attribute"
// ENOATTR is macOS-specific, ENODATA is the Linux equivalent
#[cfg(target_os = "macos")]
const ENOATTR: i32 = libc::ENOATTR;
#[cfg(not(target_os = "macos"))]
const ENOATTR: i32 = libc::ENODATA;

/// Statistics about concurrent operations
#[derive(Debug, Clone)]
pub struct ConcurrencyStats {
    /// Maximum concurrent read operations allowed
    pub max_concurrent_reads: usize,
    /// Number of available permits for reads
    pub available_permits: usize,
}

/// The main FUSE filesystem implementation for rqbit-fuse.
/// Implements the fuser::Filesystem trait to provide a FUSE interface
/// over the rqbit HTTP API.
///
/// Clone is cheap as it only increments Arc reference counts.
#[derive(Clone)]
pub struct TorrentFS {
    /// Configuration for the filesystem
    config: Config,
    /// HTTP client for rqbit API
    api_client: Arc<RqbitClient>,
    /// Inode manager for filesystem entries
    inode_manager: Arc<InodeManager>,
    /// Tracks whether the filesystem has been initialized
    initialized: bool,
    /// File handle manager for tracking open files
    file_handles: Arc<FileHandleManager>,
    /// Set of known torrent IDs for detecting removals
    known_torrents: Arc<DashSet<u64>>,
    /// Handle to the torrent discovery task
    discovery_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Metrics collection
    metrics: Arc<Metrics>,
    /// Timestamp of last discovery (ms since Unix epoch) to prevent too frequent scans
    /// Uses atomic operations for lock-free check-and-set
    last_discovery: Arc<AtomicU64>,
    /// Async worker for handling async operations in FUSE callbacks
    async_worker: Arc<AsyncFuseWorker>,
    /// Semaphore for limiting concurrent read operations
    read_semaphore: Arc<Semaphore>,
}

impl TorrentFS {
    /// Get the mount point path.
    pub fn mount_point(&self) -> &std::path::Path {
        &self.config.mount.mount_point
    }

    /// Get a reference to the async worker.
    pub fn async_worker(&self) -> &Arc<AsyncFuseWorker> {
        &self.async_worker
    }

    /// Get a reference to the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Creates a new TorrentFS instance with the given configuration.
    /// Note: This does not initialize the filesystem - call mount() to do so.
    ///
    /// # Arguments
    /// * `config` - Configuration for the filesystem
    /// * `metrics` - Metrics collection instance
    /// * `async_worker` - Async worker for handling async operations in FUSE callbacks
    ///
    /// # Returns
    /// * `Result<Self>` - A new TorrentFS instance or an error
    pub fn new(
        config: Config,
        metrics: Arc<Metrics>,
        async_worker: Arc<AsyncFuseWorker>,
    ) -> Result<Self> {
        let api_client = Arc::new(
            create_api_client(&config.api, Some(Arc::clone(&metrics)))
                .context("Failed to create API client")?,
        );
        let inode_manager = Arc::new(InodeManager::with_max_inodes(100000));
        let read_semaphore = Arc::new(Semaphore::new(config.performance.max_concurrent_reads));

        Ok(Self {
            config,
            api_client,
            inode_manager,
            initialized: false,
            file_handles: Arc::new(FileHandleManager::new()),
            known_torrents: Arc::new(DashSet::new()),
            discovery_handle: Arc::new(Mutex::new(None)),
            metrics,
            last_discovery: Arc::new(AtomicU64::new(0)),
            async_worker,
            read_semaphore,
        })
    }

    /// Get the read semaphore for limiting concurrent read operations.
    pub fn read_semaphore(&self) -> &Arc<Semaphore> {
        &self.read_semaphore
    }

    /// Get current concurrency statistics.
    pub fn concurrency_stats(&self) -> ConcurrencyStats {
        ConcurrencyStats {
            max_concurrent_reads: self.config.performance.max_concurrent_reads,
            available_permits: self.read_semaphore.available_permits(),
        }
    }

    /// Get access to the known_torrents set (for testing).
    #[cfg(test)]
    pub fn known_torrents(&self) -> &Arc<DashSet<u64>> {
        &self.known_torrents
    }

    /// Get access to the known_torrents set (for integration tests).
    #[doc(hidden)]
    pub fn __test_known_torrents(&self) -> &Arc<DashSet<u64>> {
        &self.known_torrents
    }

    /// Clear the list_torrents cache (for integration tests).
    #[doc(hidden)]
    pub async fn __test_clear_list_torrents_cache(&self) {
        self.api_client.__test_clear_cache().await;
    }

    /// Start the background torrent discovery task
    fn start_torrent_discovery(&self) {
        let api_client = Arc::clone(&self.api_client);
        let inode_manager = Arc::clone(&self.inode_manager);
        let last_discovery = Arc::clone(&self.last_discovery);
        let known_torrents = Arc::clone(&self.known_torrents);
        let file_handles = Arc::clone(&self.file_handles);
        let poll_interval = Duration::from_secs(30);

        let handle = tokio::spawn(async move {
            let mut ticker = interval(poll_interval);

            loop {
                ticker.tick().await;

                match Self::discover_torrents(&api_client, &inode_manager).await {
                    Ok(current_torrent_ids) => {
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        last_discovery.store(now_ms, Ordering::SeqCst);

                        // Update known_torrents with current torrent IDs
                        for torrent_id in &current_torrent_ids {
                            known_torrents.insert(*torrent_id);
                        }

                        // Detect and remove torrents that were deleted from rqbit
                        let current: std::collections::HashSet<u64> =
                            current_torrent_ids.iter().copied().collect();
                        let known: std::collections::HashSet<u64> =
                            known_torrents.iter().map(|e| *e).collect();
                        let removed: Vec<u64> = known.difference(&current).copied().collect();

                        for torrent_id in removed {
                            info!("Removing torrent {} from filesystem", torrent_id);

                            // Get the torrent's root inode
                            if let Some(inode) = inode_manager.lookup_torrent(torrent_id) {
                                // Close all file handles for this torrent
                                let removed_handles = file_handles.remove_by_torrent(torrent_id);
                                if removed_handles > 0 {
                                    debug!(
                                        "Closed {} file handles for torrent {}",
                                        removed_handles, torrent_id
                                    );
                                }

                                // Remove the inode tree for this torrent
                                if !inode_manager.remove_inode(inode) {
                                    warn!(
                                        "Failed to remove inode {} for torrent {}",
                                        inode, torrent_id
                                    );
                                }

                                // Remove from known torrents
                                known_torrents.remove(&torrent_id);

                                // Record metric

                                info!(
                                    "Successfully removed torrent {} from filesystem",
                                    torrent_id
                                );
                            } else {
                                warn!(
                                    "Torrent {} not found in filesystem, skipping removal",
                                    torrent_id
                                );
                                known_torrents.remove(&torrent_id);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Background torrent discovery failed: {}", e);
                    }
                }
            }
        });

        if let Ok(mut h) = self.discovery_handle.try_lock() {
            *h = Some(handle);
        }

        info!("Started background torrent discovery with 30 second interval");
    }

    /// Stop the torrent discovery task
    fn stop_torrent_discovery(&self) {
        if let Ok(handle) = self.discovery_handle.try_lock() {
            if let Some(h) = handle.as_ref() {
                h.abort();
                info!("Stopped torrent discovery");
            }
        }
    }

    /// Discover new torrents from rqbit and create filesystem structures.
    ///
    /// This is the core discovery logic used by:
    /// - `start_torrent_discovery()` - background polling
    /// - `refresh_torrents()` - explicit refresh
    /// - `readdir()` - on-demand discovery when listing root
    ///
    /// # Arguments
    /// * `api_client` - Reference to the API client for listing torrents
    /// * `inode_manager` - Reference to the inode manager for structure creation
    ///
    /// # Returns
    /// * `Result<Vec<u64>, anyhow::Error>` - List of current torrent IDs, or error
    async fn discover_torrents(
        api_client: &Arc<RqbitClient>,
        inode_manager: &Arc<InodeManager>,
    ) -> Result<Vec<u64>> {
        let result = api_client.list_torrents().await?;

        // Log any partial failures
        if result.is_partial() {
            warn!(
                "Partial torrent discovery: {} succeeded, {} failed",
                result.torrents.len(),
                result.errors.len()
            );
            for (id, name, err) in &result.errors {
                warn!("Failed to load torrent {} ({}): {}", id, name, err);
            }
        }

        // Collect all current torrent IDs
        let current_torrent_ids: Vec<u64> = result.torrents.iter().map(|t| t.id).collect();

        for torrent_info in result.torrents {
            // Check if we already have this torrent
            if inode_manager.lookup_torrent(torrent_info.id).is_none() {
                // New torrent found - create filesystem structure
                if let Err(e) = Self::create_torrent_structure_static(inode_manager, &torrent_info)
                {
                    warn!(
                        "Failed to create structure for torrent {}: {}",
                        torrent_info.id, e
                    );
                } else {
                    info!(
                        "Discovered new torrent {}: {}",
                        torrent_info.id, torrent_info.name
                    );
                }
            }
        }

        Ok(current_torrent_ids)
    }

    /// Detect torrents that have been removed from rqbit.
    /// Compares current torrent list with known torrents to find removed ones.
    ///
    /// # Arguments
    /// * `current_torrent_ids` - List of torrent IDs currently in rqbit
    ///
    /// # Returns
    /// * `Vec<u64>` - List of torrent IDs that have been removed
    fn detect_removed_torrents(&self, current_torrent_ids: &[u64]) -> Vec<u64> {
        let current: std::collections::HashSet<u64> = current_torrent_ids.iter().copied().collect();
        let known: std::collections::HashSet<u64> =
            self.known_torrents.iter().map(|e| *e).collect();

        // Torrents that were known but not in current list
        let removed: Vec<u64> = known.difference(&current).copied().collect();

        if !removed.is_empty() {
            debug!("Detected {} removed torrent(s)", removed.len());
        }

        removed
    }

    /// Remove a torrent and all its associated data from the filesystem.
    /// Closes streams, removes file handles, and cleans up inodes.
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent to remove
    async fn remove_torrent_from_fs(&self, torrent_id: u64) {
        info!("Removing torrent {} from filesystem", torrent_id);

        // Get the torrent's root inode
        if let Some(inode) = self.inode_manager.lookup_torrent(torrent_id) {
            // Close all file handles for this torrent
            let removed_handles = self.file_handles.remove_by_torrent(torrent_id);
            if removed_handles > 0 {
                debug!(
                    "Closed {} file handles for torrent {}",
                    removed_handles, torrent_id
                );
            }

            // Remove the inode tree for this torrent
            if !self.inode_manager.remove_inode(inode) {
                warn!(
                    "Failed to remove inode {} for torrent {}",
                    inode, torrent_id
                );
            }

            // Remove from known torrents
            self.known_torrents.remove(&torrent_id);

            // Record metric

            info!(
                "Successfully removed torrent {} from filesystem",
                torrent_id
            );
        } else {
            warn!(
                "Torrent {} not found in filesystem, skipping removal",
                torrent_id
            );
            // Still remove from known_torrents to keep state consistent
            self.known_torrents.remove(&torrent_id);
        }
    }

    /// Gracefully shut down the filesystem.
    ///
    /// This stops all background tasks:
    /// - Status monitoring
    /// - Torrent discovery
    ///
    /// It also shuts down the async worker to complete pending operations.
    pub fn shutdown(&self) {
        info!("Initiating graceful shutdown...");

        self.stop_torrent_discovery();

        info!("Graceful shutdown complete");
    }

    /// Refresh torrent list from rqbit with cooldown protection.
    /// Returns true if a refresh was performed, false if skipped due to cooldown.
    ///
    /// # Arguments
    /// * `force` - If true, bypass the cooldown check
    ///
    /// # Returns
    /// * `bool` - True if refresh was performed, false if skipped
    pub async fn refresh_torrents(&self, force: bool) -> bool {
        const COOLDOWN_MS: u64 = 5000;

        // Check cooldown unless forced
        if !force {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let last_ms = self.last_discovery.load(Ordering::SeqCst);

            if last_ms != 0 && now_ms.saturating_sub(last_ms) < COOLDOWN_MS {
                let remaining_secs = (COOLDOWN_MS - (now_ms - last_ms)) / 1000;
                trace!(
                    "Skipping torrent discovery - cooldown in effect ({}s remaining)",
                    remaining_secs
                );
                return false;
            }
        }

        // Perform discovery
        match Self::discover_torrents(&self.api_client, &self.inode_manager).await {
            Ok(current_torrent_ids) => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                self.last_discovery.store(now_ms, Ordering::SeqCst);

                // Update known_torrents with current torrent IDs
                for torrent_id in &current_torrent_ids {
                    self.known_torrents.insert(*torrent_id);
                }

                // Detect and remove torrents that were deleted from rqbit
                let removed = self.detect_removed_torrents(&current_torrent_ids);
                for torrent_id in removed {
                    self.remove_torrent_from_fs(torrent_id).await;
                }

                true
            }
            Err(e) => {
                warn!("Failed to refresh torrents: {}", e);
                false
            }
        }
    }

    /// Static version of create_torrent_structure for use in background tasks
    fn create_torrent_structure_static(
        inode_manager: &Arc<InodeManager>,
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

        // Create torrent directory for all torrents (both single and multi-file)
        // This ensures consistent torrent_id -> directory_inode mapping
        let torrent_dir_inode =
            inode_manager.allocate_torrent_directory(torrent_id, torrent_name.clone(), 1);

        inode_manager.add_child(1, torrent_dir_inode);

        // Handle single-file torrents - place file directly in torrent directory
        if torrent_info.files.len() == 1 {
            let file_info = &torrent_info.files[0];
            let file_name = if file_info.components.is_empty() {
                torrent_name.clone()
            } else {
                sanitize_filename(file_info.components.last().unwrap())
            };

            let file_inode = inode_manager.allocate_file(
                file_name.clone(),
                torrent_dir_inode,
                torrent_id,
                0,
                file_info.length,
            );

            inode_manager.add_child(torrent_dir_inode, file_inode);

            debug!(
                "Created single-file torrent entry {} -> {} (size: {})",
                file_name, file_inode, file_info.length
            );
        } else {
            // Multi-file torrent - directory already created above
            let mut created_dirs: HashMap<String, u64> = HashMap::new();
            created_dirs.insert("".to_string(), torrent_dir_inode);

            for (file_idx, file_info) in torrent_info.files.iter().enumerate() {
                Self::create_file_entry_static(
                    inode_manager,
                    file_info,
                    file_idx,
                    torrent_id,
                    torrent_dir_inode,
                    &mut created_dirs,
                )?;
            }
        }

        Ok(())
    }

    /// Static version of create_file_entry for use in background tasks
    fn create_file_entry_static(
        inode_manager: &Arc<InodeManager>,
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

        let mut current_dir_inode = torrent_dir_inode;
        let mut current_path = String::new();

        // Get the torrent directory's canonical path to build full paths
        let torrent_dir_path = inode_manager
            .get(torrent_dir_inode)
            .map(|e| e.canonical_path().to_string())
            .unwrap_or_else(|| "/".to_string());

        for dir_component in components.iter().take(components.len().saturating_sub(1)) {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(dir_component);

            if let Some(&inode) = created_dirs.get(&current_path) {
                current_dir_inode = inode;
            } else {
                let dir_name = sanitize_filename(dir_component);
                // Build full canonical path including torrent directory
                let full_path = format!("{}/{}", torrent_dir_path, current_path);
                let new_dir_inode = inode_manager.allocate(InodeEntry::Directory {
                    ino: 0,
                    name: dir_name.clone(),
                    parent: current_dir_inode,
                    children: DashSet::new(),
                    canonical_path: full_path,
                });

                inode_manager.add_child(current_dir_inode, new_dir_inode);
                created_dirs.insert(current_path.clone(), new_dir_inode);
                current_dir_inode = new_dir_inode;

                debug!(
                    "Created directory {} at inode {}",
                    current_path, new_dir_inode
                );
            }
        }

        let file_name = components.last().unwrap();
        let sanitized_name = sanitize_filename(file_name);

        let file_inode = inode_manager.allocate_file(
            sanitized_name,
            current_dir_inode,
            torrent_id,
            file_idx as u64,
            file_info.length,
        );

        inode_manager.add_child(current_dir_inode, file_inode);

        debug!(
            "Created file {} at inode {} (size: {})",
            file_name, file_inode, file_info.length
        );

        Ok(())
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

        info!("Mounting rqbit-fuse at: {}", mount_point.display());

        // Mount the filesystem
        fuser::mount2(self, &mount_point, &options)
            .with_context(|| format!("Failed to mount filesystem at: {}", mount_point.display()))
    }

    /// Check if the requested data range has all pieces available.
    /// Returns true if all pieces in the range are downloaded.
    #[allow(dead_code)]
    fn check_pieces_available(
        &self,
        _torrent_id: u64,
        _offset: u64,
        _size: u64,
        _piece_length: u64,
    ) -> bool {
        // Status monitoring has been removed. Piece availability is checked
        // directly via the API client's bitfield cache when needed.
        false
    }

    /// Builds FUSE mount options based on configuration.
    fn build_mount_options(&self) -> Vec<fuser::MountOption> {
        let mut options = vec![
            fuser::MountOption::RO,     // Read-only (torrents are read-only)
            fuser::MountOption::NoSuid, // No setuid/setgid
            fuser::MountOption::NoDev,  // No special device files
            fuser::MountOption::NoAtime, // Don't update access times
                                        // NOTE: Sync option removed - causes hangs on macOS due to blocking
                                        // on unmount. Since this is a read-only filesystem, data integrity
                                        // is not a concern. This fix was needed after macOS system updates
                                        // broke FUSE mounting with Sync option enabled.
        ];

        options.push(fuser::MountOption::AutoUnmount);

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
    pub fn build_file_attr(&self, entry: &crate::fs::inode::InodeEntry) -> fuser::FileAttr {
        use crate::fs::inode::InodeEntry;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let now = SystemTime::now();
        let creation_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000); // Fixed creation time
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };

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
                uid,
                gid,
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
                uid,
                gid,
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
                uid,
                gid,
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
    #[instrument(skip(self, reply), fields(fh))]
    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        let start_time = Instant::now();

        // Clamp read size to FUSE maximum to prevent "Too much data" panic
        let size = std::cmp::min(size, Self::FUSE_MAX_READ);

        tracing::debug!(fuse_op = "read", fh = fh, offset = offset, size = size);

        // Validate offset is non-negative
        if offset < 0 {
            self.metrics.record_error();
            tracing::debug!(
                fuse_op = "read",
                result = "error",
                error = "EINVAL",
                reason = "negative_offset"
            );
            reply.error(libc::EINVAL);
            return;
        }

        let offset = offset as u64;

        // Look up the inode from the file handle
        let ino = match self.file_handles.get_inode(fh) {
            Some(inode) => inode,
            None => {
                self.metrics.record_error();
                tracing::debug!(
                    fuse_op = "read",
                    result = "error",
                    error = "EBADF",
                    fh = fh,
                    reason = "invalid_file_handle"
                );
                reply.error(libc::EBADF);
                return;
            }
        };

        // Get the file entry
        let (torrent_id, file_index, file_size) = match self.inode_manager.get(ino) {
            Some(entry) => match entry {
                crate::types::InodeEntry::File {
                    torrent_id,
                    file_index,
                    size,
                    ..
                } => (torrent_id, file_index, size),
                _ => {
                    reply_not_file!(self.metrics, reply, "read", ino);
                    return;
                }
            },
            None => {
                reply_ino_not_found!(self.metrics, reply, "read", ino);
                return;
            }
        };

        // Handle zero-byte reads
        if size == 0 || offset >= file_size {
            tracing::debug!(
                fuse_op = "read",
                result = "success",
                fh = fh,
                ino = ino,
                bytes_read = 0,
                reason = "empty_read"
            );
            reply.data(&[]);
            return;
        }

        // Calculate actual read range (don't read past EOF)
        // Use saturating_sub to prevent underflow when offset == file_size
        let end = std::cmp::min(offset + size as u64, file_size).saturating_sub(1);

        tracing::debug!(
            fuse_op = "read",
            fh = fh,
            ino = ino,
            torrent_id = torrent_id,
            file_index = file_index,
            range_start = offset,
            range_end = end
        );

        // Perform the read using the async worker to avoid blocking async in sync callbacks
        // This eliminates the deadlock risk from block_in_place + block_on pattern
        let timeout_duration = Duration::from_secs(self.config.performance.read_timeout);
        let result = self.async_worker.read_file(
            torrent_id,
            file_index,
            offset,
            size as usize,
            timeout_duration,
        );

        let latency = start_time.elapsed();

        match result {
            Ok(data) => {
                let bytes_read = data.len() as u64;
                self.metrics.record_read(bytes_read);

                // Log slow reads at debug level only
                if latency > std::time::Duration::from_secs(1) {
                    debug!(
                        fuse_op = "read",
                        fh = fh,
                        ino = ino,
                        torrent_id = torrent_id,
                        latency_ms = latency.as_millis() as u64,
                        "Slow read detected"
                    );
                }

                tracing::debug!(
                    fuse_op = "read",
                    result = "success",
                    fh = fh,
                    ino = ino,
                    bytes_read = bytes_read,
                    latency_ms = latency.as_millis() as u64
                );

                // Truncate data to requested size to prevent "Too much data" FUSE panic
                // The API might return more data than requested (e.g., entire piece)
                let data_slice = if data.len() > size as usize {
                    warn!(
                        fuse_op = "read",
                        fh = fh,
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
            Err(e) => {
                self.metrics.record_error();

                // Map the error appropriately
                let error_code = e.to_errno();
                let error_msg = e.to_string();

                error!(
                    fuse_op = "read",
                    fh = fh,
                    ino = ino,
                    torrent_id = torrent_id,
                    file_index = file_index,
                    error = %error_msg,
                    "Failed to read file"
                );

                reply.error(error_code);
            }
        }
    }

    /// Release an open file.
    /// Called when a file is closed. Cleans up file handle state.
    #[instrument(skip(self, reply), fields(fh))]
    fn release(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        // Clean up the file handle
        if let Some(handle) = self.file_handles.remove(fh) {
            tracing::debug!(
                fuse_op = "release",
                result = "success",
                fh = fh,
                ino = handle.inode
            );
        } else {
            warn!(
                fuse_op = "release",
                fh = fh,
                result = "warning",
                reason = "handle_not_found"
            );
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
        let name_str = name.to_string_lossy();

        tracing::debug!(
            fuse_op = "lookup",
            parent = parent,
            name = name_str.to_string()
        );

        // Get the parent directory entry
        let parent_entry = match self.inode_manager.get(parent) {
            Some(entry) => entry,
            None => {
                reply_ino_not_found!(self.metrics, reply, "lookup", parent);
                return;
            }
        };

        // Check if parent is a directory
        if !parent_entry.is_directory() {
            reply_not_directory!(self.metrics, reply, "lookup", parent);
            return;
        }

        // Handle special entries: "." and ".."
        let target_ino = match name_str.as_ref() {
            "." => {
                // "." refers to the current directory (parent)
                Some(parent)
            }
            ".." => {
                // ".." refers to the parent directory
                // Root directory's parent is itself
                Some(parent_entry.parent())
            }
            _ => None,
        };

        if let Some(ino) = target_ino {
            if let Some(entry) = self.inode_manager.get(ino) {
                let attr = self.build_file_attr(&entry);
                reply.entry(&std::time::Duration::from_secs(1), &attr, 0);
                tracing::debug!(
                    fuse_op = "lookup",
                    result = "success",
                    parent = parent,
                    name = name_str.to_string(),
                    ino = ino,
                    special = true
                );
            } else {
                // This shouldn't happen - special entry maps to non-existent inode
                error!(
                    fuse_op = "lookup",
                    parent = parent,
                    name = %name_str,
                    target_ino = ino,
                    "Special entry maps to missing inode"
                );
                reply.error(libc::EIO);
            }
            return;
        }

        // Build the full path for this entry
        let path = if parent == 1 {
            format!("/{}", name_str)
        } else {
            match self.inode_manager.get_path_for_inode(parent) {
                Some(parent_path) => format!("{}/{}", parent_path, name_str),
                None => {
                    error!(
                        fuse_op = "lookup",
                        parent = parent,
                        "Failed to build path for parent inode"
                    );
                    reply.error(libc::EIO);
                    return;
                }
            }
        };

        // Look up the inode by path
        match self.inode_manager.lookup_by_path(&path) {
            Some(ino) => {
                match self.inode_manager.get(ino) {
                    Some(entry) => {
                        let attr = self.build_file_attr(&entry);
                        reply.entry(&std::time::Duration::from_secs(1), &attr, 0);
                        tracing::debug!(
                            fuse_op = "lookup",
                            result = "success",
                            parent = parent,
                            name = name_str.to_string(),
                            ino = ino
                        );
                    }
                    None => {
                        // This shouldn't happen - path maps to non-existent inode
                        self.metrics.record_error();

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
                tracing::debug!(
                    fuse_op = "lookup",
                    parent = parent,
                    name = name_str.to_string(),
                    result = "not_found"
                );
                reply.error(libc::ENOENT);
            }
        }
    }

    /// Get file attributes.
    /// Called when the kernel needs to get attributes for a file or directory.
    /// This is a fundamental operation used by ls, stat, and most file operations.
    #[instrument(skip(self, reply), fields(ino))]
    fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
        tracing::debug!(fuse_op = "getattr", ino = ino);

        // Get the inode entry
        match self.inode_manager.get(ino) {
            Some(entry) => {
                let attr = self.build_file_attr(&entry);
                let ttl = std::time::Duration::from_secs(1);

                tracing::debug!(
                    fuse_op = "getattr",
                    result = "success",
                    ino = ino,
                    kind = format!("{:?}", attr.kind),
                    size = attr.size
                );
                reply.attr(&ttl, &attr);
            }
            None => {
                reply_ino_not_found!(self.metrics, reply, "getattr", ino);
            }
        }
    }

    /// Open a file.
    /// Called when the kernel needs to open a file for reading.
    /// Returns a file handle that will be used in subsequent read operations.
    #[instrument(skip(self, reply), fields(ino))]
    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
        tracing::debug!(fuse_op = "open", ino = ino, flags = flags);

        // Check if the inode exists
        match self.inode_manager.get(ino) {
            Some(entry) => {
                // Check if it's a file (not a directory)
                if entry.is_directory() {
                    reply_not_file!(self.metrics, reply, "open", ino);
                    return;
                }

                // Check if it's a symlink (symlinks should be resolved before open)
                if entry.is_symlink() {
                    self.metrics.record_error();
                    tracing::debug!(fuse_op = "open", result = "error", error = "ELOOP");
                    reply.error(libc::ELOOP);
                    return;
                }

                // Check write access - this is a read-only filesystem
                let access_mode = flags & libc::O_ACCMODE;
                if access_mode != libc::O_RDONLY {
                    reply_no_permission!(
                        self.metrics,
                        reply,
                        "open",
                        ino,
                        "write_access_requested"
                    );
                    return;
                }

                // Get torrent_id from the entry
                let torrent_id = entry.torrent_id().unwrap_or(0);

                // Allocate a unique file handle
                let fh = self.file_handles.allocate(ino, torrent_id, flags);

                // Check if handle allocation failed (limit reached)
                if fh == 0 {
                    self.metrics.record_error();
                    tracing::debug!(
                        fuse_op = "open",
                        result = "error",
                        error = "EMFILE",
                        reason = "handle_limit_reached"
                    );
                    reply.error(libc::EMFILE);
                    return;
                }

                tracing::debug!(fuse_op = "open", result = "success", ino = ino, fh = fh);
                reply.opened(fh, 0);
            }
            None => {
                reply_ino_not_found!(self.metrics, reply, "open", ino);
            }
        }
    }

    /// Read the target of a symbolic link.
    /// Called when the kernel needs to resolve a symlink target.
    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!("readlink: ino={}", ino);

        match self.inode_manager.get(ino) {
            Some(entry) => {
                if let crate::types::InodeEntry::Symlink { target, .. } = entry {
                    reply.data(target.as_bytes());
                    debug!("readlink: resolved symlink to {}", target);
                } else {
                    debug!("readlink: inode {} is not a symlink", ino);
                    reply.error(libc::EINVAL);
                }
            }
            None => {
                reply_ino_not_found!(self.metrics, reply, "readlink", ino);
            }
        }
    }

    /// Read directory entries.
    /// Called when the kernel needs to list the contents of a directory.
    /// For the root directory, this will also trigger a torrent discovery check.
    #[instrument(skip(self, reply), fields(ino))]
    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        tracing::debug!(fuse_op = "readdir", ino = ino, offset = offset);

        // Trigger torrent discovery when listing root directory (with cooldown)
        if ino == 1 {
            let api_client = Arc::clone(&self.api_client);
            let inode_manager = Arc::clone(&self.inode_manager);
            let last_discovery = Arc::clone(&self.last_discovery);
            let known_torrents = Arc::clone(&self.known_torrents);
            let file_handles = Arc::clone(&self.file_handles);

            tokio::spawn(async move {
                const COOLDOWN_MS: u64 = 5000;

                // Atomically check and claim discovery slot
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let last_ms = last_discovery.load(Ordering::SeqCst);

                let should_run = last_ms == 0 || now_ms.saturating_sub(last_ms) >= COOLDOWN_MS;

                if should_run {
                    match Self::discover_torrents(&api_client, &inode_manager).await {
                        Ok(current_torrent_ids) => {
                            last_discovery.store(now_ms, Ordering::SeqCst);

                            // Update known_torrents with current torrent IDs
                            for torrent_id in &current_torrent_ids {
                                known_torrents.insert(*torrent_id);
                            }

                            // Detect and remove torrents that were deleted from rqbit
                            let current: std::collections::HashSet<u64> =
                                current_torrent_ids.iter().copied().collect();
                            let known: std::collections::HashSet<u64> =
                                known_torrents.iter().map(|e| *e).collect();
                            let removed: Vec<u64> = known.difference(&current).copied().collect();

                            for torrent_id in removed {
                                info!("Removing torrent {} from filesystem", torrent_id);

                                // Get the torrent's root inode
                                if let Some(inode) = inode_manager.lookup_torrent(torrent_id) {
                                    // Close all file handles for this torrent
                                    let removed_handles =
                                        file_handles.remove_by_torrent(torrent_id);
                                    if removed_handles > 0 {
                                        debug!(
                                            "Closed {} file handles for torrent {}",
                                            removed_handles, torrent_id
                                        );
                                    }

                                    // Remove the inode tree for this torrent
                                    if !inode_manager.remove_inode(inode) {
                                        warn!(
                                            "Failed to remove inode {} for torrent {}",
                                            inode, torrent_id
                                        );
                                    }

                                    // Remove from known torrents
                                    known_torrents.remove(&torrent_id);

                                    // Record metric

                                    info!(
                                        "Successfully removed torrent {} from filesystem",
                                        torrent_id
                                    );
                                } else {
                                    warn!(
                                        "Torrent {} not found in filesystem, skipping removal",
                                        torrent_id
                                    );
                                    known_torrents.remove(&torrent_id);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("On-demand torrent discovery failed: {}", e);
                        }
                    }
                }
            });
        }

        // Get the directory entry
        let entry = match self.inode_manager.get(ino) {
            Some(e) => e,
            None => {
                reply_ino_not_found!(self.metrics, reply, "readdir", ino);
                return;
            }
        };

        // Check if it's a directory
        if !entry.is_directory() {
            reply_not_directory!(self.metrics, reply, "readdir", ino);
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
            // Get all file inodes in this torrent directory
            let file_inodes: Vec<u64> = self
                .inode_manager
                .get_children(ino)
                .iter()
                .filter(|(_, entry)| entry.is_file())
                .map(|(inode, _)| *inode)
                .collect();

            // Check if any file handle points to these inodes
            file_inodes.iter().any(|file_inode| {
                !self
                    .file_handles
                    .get_handles_for_inode(*file_inode)
                    .is_empty()
            })
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
            let error_code = if let Some(api_err) = e.downcast_ref::<crate::error::RqbitFuseError>()
            {
                api_err.to_errno()
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
        _size: u32,
        reply: fuser::ReplyXattr,
    ) {
        let name_str = name.to_string_lossy();
        debug!("getxattr: ino={}, name={}", ino, name_str);

        // Only support the "user.torrent.status" attribute
        if name_str != "user.torrent.status" {
            reply.error(ENOATTR);
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
                    reply.error(ENOATTR);
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
            reply.error(ENOATTR);
            return;
        }

        // Status monitoring has been removed, return attribute not found
        reply.error(ENOATTR);
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
        info!("Initializing rqbit-fuse filesystem");

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

        // Start the background torrent discovery task
        self.start_torrent_discovery();

        self.initialized = true;
        info!("rqbit-fuse filesystem initialized successfully");

        Ok(())
    }

    /// Clean up filesystem.
    /// Called on unmount.
    fn destroy(&mut self) {
        info!("Shutting down rqbit-fuse filesystem");
        self.initialized = false;
        // Stop the torrent discovery task
        self.stop_torrent_discovery();
        // Clean up any resources
    }

    /// Get filesystem statistics.
    /// Returns information about the filesystem such as total space, free space, etc.
    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        let inode_count = self.inode_manager.len() as u64;

        reply.statfs(0, 0, 0, inode_count, inode_count, 4096, 255, 4096);
    }

    /// Check file access permissions.
    /// This is called for the access() system call.
    /// Since this is a read-only filesystem:
    /// - R_OK (read) is allowed if the inode exists
    /// - W_OK (write) is always denied (read-only filesystem)
    /// - X_OK (execute) is allowed for directories (to traverse), denied for files
    fn access(&mut self, _req: &fuser::Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        debug!("access: ino={}, mask={}", ino, mask);

        const W_OK: i32 = 2;
        const X_OK: i32 = 1;
        const F_OK: i32 = 0;

        if mask == F_OK {
            if self.inode_manager.contains(ino) {
                reply.ok();
            } else {
                reply.error(libc::ENOENT);
            }
            return;
        }

        if mask & W_OK != 0 {
            debug!("access: denying write access for ino={}", ino);
            reply.error(libc::EACCES);
            return;
        }

        match self.inode_manager.get(ino) {
            Some(entry) => {
                if entry.is_directory() {
                    reply.ok();
                } else if mask & X_OK != 0 {
                    debug!("access: denying execute on file ino={}", ino);
                    reply.error(libc::EACCES);
                } else {
                    reply.ok();
                }
            }
            None => {
                debug!("access: inode not found ino={}", ino);
                reply.error(libc::ENOENT);
            }
        }
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
    let result = fs
        .api_client
        .list_torrents()
        .await
        .context("Failed to list torrents from rqbit")?;

    // Log any partial failures
    if result.is_partial() {
        warn!(
            "Partial torrent discovery: {} succeeded, {} failed",
            result.torrents.len(),
            result.errors.len()
        );
        for (id, name, err) in &result.errors {
            warn!("Failed to load torrent {} ({}): {}", id, name, err);
        }
    }

    if !result.has_successes() {
        info!("No existing torrents found in rqbit");
        return Ok(());
    }

    info!(
        "Found {} existing torrents, populating filesystem...",
        result.torrents.len()
    );

    let mut success_count = 0;
    let mut error_count = 0;

    for torrent_info in result.torrents {
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
            info!(
                torrent_id = torrent_id,
                file_count = torrent_info.files.len(),
                "About to process files"
            );
            for (file_idx, file_info) in torrent_info.files.iter().enumerate() {
                info!(torrent_id = torrent_id, file_idx = file_idx, file_name = %file_info.name, "Processing file");
                self.create_file_entry(
                    file_info,
                    file_idx,
                    torrent_id,
                    torrent_dir_inode,
                    &mut created_dirs,
                    &torrent_name,
                )?;
            }
            info!(torrent_id = torrent_id, "Finished processing all files");
        }

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
        _torrent_name: &str,
    ) -> Result<()> {
        let components = &file_info.components;

        if components.is_empty() {
            debug!(
                torrent_id = torrent_id,
                file_idx = file_idx,
                file_name = %file_info.name,
                "create_file_entry: empty components, using file name as fallback"
            );
            // Use file_info.name as fallback when components is empty
            let file_name = sanitize_filename(&file_info.name);
            let file_inode = self.inode_manager.allocate_file(
                file_name.clone(),
                torrent_dir_inode,
                torrent_id,
                file_idx as u64,
                file_info.length,
            );
            self.inode_manager.add_child(torrent_dir_inode, file_inode);
            debug!(
                torrent_id = torrent_id,
                file_idx = file_idx,
                file_name = %file_name,
                inode = file_inode,
                "Created file from empty components"
            );
            return Ok(());
        }

        // Get torrent directory's canonical path for building full paths
        let torrent_dir_path = self
            .inode_manager
            .get_path_for_inode(torrent_dir_inode)
            .unwrap_or_else(|| "/".to_string());

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
                // Create new directory with full canonical path
                let dir_name = sanitize_filename(dir_component);
                let full_canonical_path = if torrent_dir_path == "/" {
                    format!("/{}", current_path)
                } else {
                    format!("{}/{}", torrent_dir_path, current_path)
                };
                let new_dir_inode = self.inode_manager.allocate(InodeEntry::Directory {
                    ino: 0,
                    name: dir_name.clone(),
                    parent: current_dir_inode,
                    children: DashSet::new(),
                    canonical_path: full_canonical_path,
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
                    ino: 0,
                    name: dir_name.clone(),
                    parent: current_dir_inode,
                    children: DashSet::new(),
                    canonical_path: format!("/{}", current_path),
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
            file_idx as u64,
            file_info.length,
        );

        // Add to parent directory
        self.inode_manager.add_child(current_dir_inode, file_inode);

        info!(
            torrent_id = torrent_id,
            file_idx = file_idx,
            file_name = %file_name,
            inode = file_inode,
            parent_inode = current_dir_inode,
            size = file_info.length,
            "Created file entry"
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
    /// 1. Removes the torrent from rqbit (forget - keeps files)
    /// 2. Removes all inodes associated with the torrent
    /// 3. Removes the torrent directory from root's children
    fn remove_torrent(&self, torrent_id: u64, torrent_inode: u64) -> Result<()> {
        debug!("Removing torrent {} (inode {})", torrent_id, torrent_inode);

        // Remove from rqbit (forget - keeps downloaded files) using async worker
        // This avoids the dangerous block_in_place + block_on pattern
        let timeout = Duration::from_secs(30);
        if let Err(e) = self.async_worker.forget_torrent(torrent_id, timeout) {
            return Err(anyhow::anyhow!(
                "Failed to remove torrent {} from rqbit: {}",
                torrent_id,
                e
            ));
        }

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

    /// Helper function to create a test AsyncFuseWorker
    fn create_test_async_worker() -> Arc<AsyncFuseWorker> {
        let api_config = crate::config::ApiConfig::default();
        let api_client =
            Arc::new(create_api_client(&api_config, None).expect("Failed to create API client"));
        Arc::new(AsyncFuseWorker::new(
            api_client,
            Arc::new(crate::metrics::Metrics::new()),
            100,
        ))
    }

    #[tokio::test]
    async fn test_torrent_fs_creation() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

        assert!(!fs.is_initialized());
        assert_eq!(fs.inode_manager().get(1).unwrap().ino(), 1);
    }

    #[tokio::test]
    async fn test_validate_mount_point_success() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.mount.mount_point = temp_dir.path().to_path_buf();

        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();
        assert!(fs.validate_mount_point().is_ok());
    }

    #[tokio::test]
    async fn test_validate_mount_point_nonexistent() {
        let mut config = Config::default();
        config.mount.mount_point = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();
        let result = fs.validate_mount_point();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_validate_mount_point_is_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not_a_directory.txt");
        std::fs::write(&file_path, "This is a file, not a directory").unwrap();

        let mut config = Config::default();
        config.mount.mount_point = file_path;

        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();
        let result = fs.validate_mount_point();

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("is not a directory") || error_msg.contains("Not a directory"),
            "Expected error message about mount point not being a directory, got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_build_mount_options() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

        let options = fs.build_mount_options();

        // Check that required options are present
        assert!(options.contains(&fuser::MountOption::RO));
        assert!(options.contains(&fuser::MountOption::NoSuid));
        assert!(options.contains(&fuser::MountOption::AutoUnmount));
    }

    #[tokio::test]
    async fn test_remove_torrent_cleans_up_inodes() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_symlink_creation() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_zero_byte_file() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_large_file() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_unicode_filename() {
        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_single_file_torrent_structure() {
        use crate::api::types::{FileInfo, TorrentInfo};

        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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

    #[tokio::test]
    async fn test_multi_file_torrent_structure() {
        use crate::api::types::{FileInfo, TorrentInfo};

        let config = Config::default();
        let async_worker = create_test_async_worker();
        let fs = TorrentFS::new(
            config,
            Arc::new(crate::metrics::Metrics::new()),
            async_worker,
        )
        .unwrap();

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
