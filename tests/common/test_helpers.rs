//! Consolidated test helpers for rqbit-fuse
//!
//! This module provides a unified interface for common test operations,
//! reducing duplication across test files. It re-exports and extends
//! functionality from the more specific helper modules.
//!
//! # Usage
//!
//! ```rust
//! use rqbit_fuse_test::common::test_helpers::*;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     let (fs, _mock, _temp) = setup_test_environment().await;
//!     // Test code here
//! }
//! ```

use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use rqbit_fuse::api::types::{FileInfo, TorrentInfo};
use rqbit_fuse::types::handle::FileHandleManager;
use rqbit_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

/// Standard test environment containing all necessary components
///
/// This struct encapsulates a complete test environment with filesystem,
/// mock server, and temporary directory. It provides convenient methods
/// for common test operations and automatically cleans up resources.
///
/// # Example
/// ```rust
/// let env = TestEnvironment::new().await.unwrap();
/// // Use env.fs, env.mock_server, env.temp_dir
/// // Resources cleaned up automatically when env goes out of scope
/// ```
pub struct TestEnvironment {
    /// The configured TorrentFS instance
    pub fs: Arc<TorrentFS>,
    /// The mock rqbit server
    pub mock_server: MockServer,
    /// Temporary directory for mount point (auto-deleted on drop)
    pub temp_dir: TempDir,
    /// Metrics instance for monitoring
    pub metrics: Arc<Metrics>,
}

impl TestEnvironment {
    /// Create a new test environment with standard configuration
    ///
    /// Sets up:
    /// - Mock server with basic endpoints
    /// - Temporary directory for mount point
    /// - Configured TorrentFS with async worker
    ///
    /// # Returns
    /// A fully configured TestEnvironment or an error
    pub async fn new() -> anyhow::Result<Self> {
        let mock_server = setup_mock_server().await;
        let temp_dir = TempDir::new()?;
        let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
        let metrics = Arc::new(Metrics::new());
        let fs = Arc::new(create_test_fs(config, Arc::clone(&metrics)));

        Ok(Self {
            fs,
            mock_server,
            temp_dir,
            metrics,
        })
    }

    /// Create a test environment with a specific torrent pre-configured
    ///
    /// # Arguments
    /// * `torrent_id` - The ID for the torrent
    /// * `torrent_name` - Name of the torrent
    /// * `files` - Vector of FileInfo describing the torrent contents
    ///
    /// # Returns
    /// A TestEnvironment with the torrent structure already created
    pub async fn with_torrent(
        torrent_id: u64,
        torrent_name: &str,
        files: Vec<FileInfo>,
    ) -> anyhow::Result<Self> {
        let mut env = Self::new().await?;

        // Set up mock endpoints for this torrent
        setup_torrent_mocks(&env.mock_server, torrent_id, torrent_name, &files).await;

        // Create torrent structure
        let torrent_info = TorrentInfo {
            id: torrent_id,
            info_hash: format!("hash{}", torrent_id),
            name: torrent_name.to_string(),
            output_folder: "/downloads".to_string(),
            file_count: Some(files.len()),
            files,
            piece_length: Some(262144),
        };

        env.fs.create_torrent_structure(&torrent_info)?;

        Ok(env)
    }

    /// Get the mount point path
    pub fn mount_point(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Get the mock server URI
    pub fn mock_uri(&self) -> String {
        self.mock_server.uri()
    }

    /// Access the inode manager
    pub fn inode_manager(&self) -> &rqbit_fuse::fs::inode::InodeManager {
        self.fs.inode_manager()
    }

    /// Create a file handle manager for testing handle operations
    pub fn create_handle_manager(&self) -> FileHandleManager {
        FileHandleManager::default()
    }
}

/// Convenience function to quickly set up a test environment
///
/// This is the recommended way to start most tests. It returns a tuple
/// containing all necessary components for testing.
///
/// # Returns
/// A tuple of (TorrentFS, MockServer, TempDir)
///
/// # Example
/// ```rust
/// let (fs, mock, temp) = setup_test_environment().await.unwrap();
/// ```
pub async fn setup_test_environment() -> anyhow::Result<(Arc<TorrentFS>, MockServer, TempDir)> {
    let env = TestEnvironment::new().await?;
    Ok((env.fs, env.mock_server, env.temp_dir))
}

/// Set up a basic mock server with standard endpoints
///
/// Creates a mock server with:
/// - /torrents endpoint returning empty list
/// - /torrents/{id} endpoints for basic torrent info
///
/// # Returns
/// A configured MockServer instance
///
/// # Example
/// ```rust
/// let mock = setup_mock_server().await;
/// ```
pub async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Default torrent list response
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": []
        })))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Set up mock endpoints for a specific torrent
///
/// Adds mock endpoints for:
/// - GET /torrents/{id}
/// - GET /torrents/{id}/stream/{file_index}
///
/// # Arguments
/// * `mock_server` - The MockServer instance
/// * `torrent_id` - The torrent ID
/// * `torrent_name` - Name of the torrent
/// * `files` - List of files in the torrent
pub async fn setup_torrent_mocks(
    mock_server: &MockServer,
    torrent_id: u64,
    torrent_name: &str,
    files: &[FileInfo],
) {
    let torrent_path = format!("/torrents/{}", torrent_id);

    Mock::given(method("GET"))
        .and(path(torrent_path.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": torrent_id,
            "info_hash": format!("hash{}", torrent_id),
            "name": torrent_name,
            "output_folder": "/downloads",
            "file_count": files.len(),
            "files": files,
            "piece_length": 262144
        })))
        .mount(mock_server)
        .await;

    // Set up streaming endpoints for each file
    for (idx, file) in files.iter().enumerate() {
        let stream_path = format!("/torrents/{}/stream/{}", torrent_id, idx);
        Mock::given(method("GET"))
            .and(path(stream_path.as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("Test data for {}", file.name)),
            )
            .mount(mock_server)
            .await;
    }
}

/// Create a test configuration pointing to a mock server
///
/// # Arguments
/// * `mock_uri` - The URI of the mock server
/// * `mount_point` - The filesystem mount point
///
/// # Returns
/// A Config instance configured for testing
pub fn create_test_config(mock_uri: String, mount_point: std::path::PathBuf) -> Config {
    let mut config = Config::default();
    config.api.url = mock_uri;
    config.mount.mount_point = mount_point;
    config.mount.allow_other = false;
    config.mount.auto_unmount = true;
    config
}

/// Create a TorrentFS with an async worker for tests
///
/// # Arguments
/// * `config` - The configuration for the filesystem
/// * `metrics` - The metrics instance to use
///
/// # Returns
/// A TorrentFS instance ready for testing
pub fn create_test_fs(config: Config, metrics: Arc<Metrics>) -> TorrentFS {
    let api_client = Arc::new(
        rqbit_fuse::api::client::RqbitClient::new(config.api.url.clone(), Arc::clone(&metrics.api))
            .expect("Failed to create API client"),
    );
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, metrics.clone(), 100));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

/// Create a TorrentFS with custom configuration
///
/// Similar to create_test_fs but allows for more control over the configuration.
///
/// # Arguments
/// * `config` - The configuration for the filesystem
/// * `metrics` - The metrics instance to use
///
/// # Returns
/// A TorrentFS instance ready for testing
pub fn create_test_fs_with_config(config: Config, metrics: Arc<Metrics>) -> TorrentFS {
    create_test_fs(config, metrics)
}

/// Helper functions for file handle operations in tests
pub mod handle_helpers {
    use super::*;
    use rqbit_fuse::types::handle::FileHandleManager;

    /// Create a new FileHandleManager with default settings
    pub fn create_handle_manager() -> FileHandleManager {
        FileHandleManager::default()
    }

    /// Allocate a file handle for testing
    ///
    /// # Arguments
    /// * `manager` - The FileHandleManager
    /// * `inode` - The inode number
    /// * `torrent_id` - The torrent ID
    /// * `flags` - Open flags (default: 0)
    ///
    /// # Returns
    /// The allocated file handle
    pub fn allocate_test_handle(
        manager: &FileHandleManager,
        inode: u64,
        torrent_id: u64,
        flags: i32,
    ) -> u64 {
        manager.allocate(inode, torrent_id, flags)
    }

    /// Allocate multiple file handles at once
    ///
    /// # Arguments
    /// * `manager` - The FileHandleManager
    /// * `handles` - Vector of (inode, torrent_id, flags) tuples
    ///
    /// # Returns
    /// Vector of allocated file handles
    pub fn allocate_test_handles(
        manager: &FileHandleManager,
        handles: Vec<(u64, u64, i32)>,
    ) -> Vec<u64> {
        handles
            .into_iter()
            .map(|(inode, torrent_id, flags)| manager.allocate(inode, torrent_id, flags))
            .collect()
    }

    /// Release a file handle
    ///
    /// # Arguments
    /// * `manager` - The FileHandleManager
    /// * `handle` - The file handle to release
    pub fn release_test_handle(manager: &FileHandleManager, handle: u64) {
        manager.remove(handle);
    }

    /// Test helper to exhaust handle limit
    ///
    /// Allocates handles until the limit is reached
    ///
    /// # Arguments
    /// * `manager` - The FileHandleManager
    /// * `max_handles` - Maximum number of handles to allocate
    /// * `inode` - Inode to use for allocations
    /// * `torrent_id` - Torrent ID to use for allocations
    ///
    /// # Returns
    /// Vector of successfully allocated handles
    pub fn exhaust_handle_limit(
        manager: &FileHandleManager,
        max_handles: usize,
        inode: u64,
        torrent_id: u64,
    ) -> Vec<u64> {
        let mut handles = Vec::new();

        for _ in 0..max_handles {
            let fh = manager.allocate(inode, torrent_id, 0);
            if fh > 0 {
                handles.push(fh);
            } else {
                break;
            }
        }

        handles
    }
}

/// Helper functions for creating test torrents
pub mod torrent_helpers {
    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

    /// Create a minimal single-file torrent
    pub fn single_file(id: u64, filename: &str, size: u64) -> TorrentInfo {
        TorrentInfo {
            id,
            info_hash: format!("hash{}", id),
            name: filename.to_string(),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: size,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(262144),
        }
    }

    /// Create a multi-file torrent
    pub fn multi_file(id: u64, name: &str, files: Vec<(String, u64)>) -> TorrentInfo {
        let file_infos: Vec<FileInfo> = files
            .into_iter()
            .map(|(name, length)| FileInfo {
                name: name.clone(),
                length,
                components: vec![name],
            })
            .collect();

        TorrentInfo {
            id,
            info_hash: format!("hash{}", id),
            name: name.to_string(),
            output_folder: "/downloads".to_string(),
            file_count: Some(file_infos.len()),
            files: file_infos,
            piece_length: Some(262144),
        }
    }
}

// Types are imported directly where needed
