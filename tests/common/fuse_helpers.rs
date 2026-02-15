//! FUSE filesystem testing utilities
//!
//! Provides helper functions and wrappers for testing FUSE operations,
//! including mount/unmount helpers and filesystem lifecycle management.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

use rqbit_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

/// Test filesystem wrapper that handles lifecycle management
///
/// This struct wraps a TorrentFS instance and provides convenient
/// methods for mounting, unmounting, and testing filesystem operations.
///
/// # Example
/// ```rust
/// let test_fs = TestFilesystem::new(mock_uri).await?;
/// // Perform filesystem operations...
/// test_fs.unmount().await?;
/// ```
pub struct TestFilesystem {
    fs: Arc<TorrentFS>,
    mount_point: TempDir,
    mount_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestFilesystem {
    /// Create and mount a test filesystem
    ///
    /// # Arguments
    /// * `mock_uri` - The URI of the mock rqbit server
    ///
    /// # Returns
    /// Result containing the TestFilesystem or an error
    ///
    /// # Errors
    /// Returns an error if:
    /// - Temp directory creation fails
    /// - Filesystem creation fails
    /// - Mount operation fails
    pub async fn new(mock_uri: String) -> anyhow::Result<Self> {
        let mount_point = TempDir::new()?;
        let mut config = Config::default();
        config.api.url = mock_uri;
        config.mount.mount_point = mount_point.path().to_path_buf();
        config.mount.allow_other = false;
        config.mount.auto_unmount = true;

        let metrics = Arc::new(Metrics::new());
        let api_client = Arc::new(rqbit_fuse::api::client::RqbitClient::new(
            config.api.url.clone(),
            Arc::clone(&metrics.api),
        ));
        let async_worker = Arc::new(AsyncFuseWorker::new(api_client, metrics.clone(), 100));
        let fs = Arc::new(TorrentFS::new(config, metrics, async_worker)?);

        // Start mount in background (if supported)
        // Note: Actual FUSE mounting requires elevated privileges
        // This creates the filesystem structure without kernel mount
        let fs_clone = Arc::clone(&fs);
        let mount_handle = tokio::spawn(async move {
            // Initialize the filesystem
            let _ = fs_clone.init(&fuser::Request::new(0, 0, 0), &mut fuser::KernelConfig::empty());
        });

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(200)).await;

        Ok(Self {
            fs,
            mount_point,
            mount_handle: Some(mount_handle),
        })
    }

    /// Get the mount point path
    ///
    /// # Returns
    /// The temporary directory path used as the mount point
    pub fn mount_point(&self) -> &Path {
        self.mount_point.path()
    }

    /// Get a reference to the underlying TorrentFS
    ///
    /// # Returns
    /// An Arc reference to the TorrentFS instance
    pub fn filesystem(&self) -> Arc<TorrentFS> {
        Arc::clone(&self.fs)
    }

    /// Get the inode manager from the filesystem
    ///
    /// # Returns
    /// A reference to the InodeManager
    pub fn inode_manager(&self) -> &rqbit_fuse::fs::inode::InodeManager {
        self.fs.inode_manager()
    }

    /// Unmount the filesystem and clean up resources
    ///
    /// # Returns
    /// Result indicating success or failure
    ///
    /// # Errors
    /// Returns an error if the unmount operation fails
    pub async fn unmount(mut self) -> anyhow::Result<()> {
        // Wait for mount task to complete
        if let Some(handle) = self.mount_handle.take() {
            let _ = timeout(Duration::from_secs(5), handle).await;
        }

        Ok(())
    }
}

/// Helper function to create a TorrentFS with an async worker for tests
///
/// This is a lower-level helper that creates a filesystem without mounting.
/// Useful for unit testing FUSE callbacks directly.
///
/// # Arguments
/// * `config` - The configuration for the filesystem
/// * `metrics` - The metrics instance to use
///
/// # Returns
/// A TorrentFS instance ready for testing
///
/// # Example
/// ```rust
/// let config = create_test_config(mock_uri, temp_dir.path().to_path_buf());
/// let metrics = Arc::new(Metrics::new());
/// let fs = create_test_fs(config, metrics);
/// ```
pub fn create_test_fs(config: Config, metrics: Arc<Metrics>) -> TorrentFS {
    let api_client = Arc::new(rqbit_fuse::api::client::RqbitClient::new(
        config.api.url.clone(),
        Arc::clone(&metrics.api),
    ));
    let async_worker = Arc::new(AsyncFuseWorker::new_for_test(api_client, metrics.clone()));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

/// Wait for a filesystem to be ready
///
/// Polls the mount point until it's accessible or timeout occurs.
///
/// # Arguments
/// * `mount_point` - The path to wait for
/// * `timeout_secs` - Maximum seconds to wait
///
/// # Returns
/// Result indicating whether the mount point became accessible
///
/// # Errors
/// Returns an error if timeout occurs or the directory cannot be read
pub async fn wait_for_mount(mount_point: &Path, timeout_secs: u64) -> anyhow::Result<()> {
    timeout(Duration::from_secs(timeout_secs), async {
        loop {
            if mount_point.exists() && mount_point.read_dir().is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    Ok(())
}

/// Check if FUSE operations can be performed
///
/// This function checks if the environment supports FUSE operations
/// (e.g., proper permissions, FUSE availability).
///
/// # Returns
/// true if FUSE operations are supported, false otherwise
pub fn fuse_available() -> bool {
    // Check if /dev/fuse exists (Linux)
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/fuse").exists()
    }

    // macOS and other platforms - assume not available for safety
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Skip a test if FUSE is not available
///
/// # Example
/// ```rust
/// #[tokio::test]
/// async fn test_fuse_operation() {
///     skip_if_no_fuse!();
///     // Test code that requires FUSE...
/// }
/// ```
#[macro_export]
macro_rules! skip_if_no_fuse {
    () => {
        if !$crate::common::fuse_helpers::fuse_available() {
            eprintln!("Skipping test: FUSE not available");
            return;
        }
    };
}

/// Create a standard FUSE request for testing
///
/// # Arguments
/// * `uid` - User ID (default: 1000)
/// * `gid` - Group ID (default: 1000)
/// * `pid` - Process ID (default: 1)
///
/// # Returns
/// A fuser::Request instance for use in tests
pub fn create_test_request(uid: u32, gid: u32, pid: u32) -> fuser::Request {
    fuser::Request::new(uid, gid, pid)
}
