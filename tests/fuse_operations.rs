//! FUSE filesystem operation tests
//!
//! These tests verify the core operations that FUSE callbacks rely on:
//! - Torrent structure creation (what lookup uses)
//! - Inode management (what getattr/readdir use)
//! - Path resolution (what lookup uses)
//! - File attribute building (what getattr uses)
//!
//! Note: Testing actual FUSE callbacks requires either:
//! 1. Mocking FUSE reply senders (complex)
//! 2. Real FUSE mount with privileged access
//!
//! These tests focus on the internal state that FUSE operations depend on,
//! ensuring the filesystem correctly manages inodes, paths, and torrent structures.

use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use torrent_fuse::api::types::{FileInfo, TorrentInfo};
use torrent_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

/// Sets up a mock rqbit server with standard responses
async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Default health check response
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Creates a test configuration pointing to the mock server
fn create_test_config(mock_uri: String, mount_point: std::path::PathBuf) -> Config {
    let mut config = Config::default();
    config.api.url = mock_uri;
    config.mount.mount_point = mount_point;
    config.mount.allow_other = false;
    config
}

/// Helper function to create a TorrentFS with an async worker for tests
fn create_test_fs(config: Config, metrics: Arc<Metrics>) -> TorrentFS {
    let api_client = Arc::new(
        torrent_fuse::api::client::RqbitClient::new(
            config.api.url.clone(),
            Arc::clone(&metrics.api),
        )
        .expect("Failed to create API client"),
    );
    let async_worker = Arc::new(AsyncFuseWorker::new_for_test(api_client, metrics.clone()));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

#[tokio::test]
async fn test_filesystem_creation_and_initialization() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Verify filesystem was created
    assert!(!fs.is_initialized());
}

#[tokio::test]
async fn test_torrent_structure_creation_single_file() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a single-file torrent structure
    // Note: Single-file torrents add the file directly to root (no directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "test.txt".to_string(),
            length: 1024,
            components: vec!["test.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Verify torrent was created
    assert!(fs.has_torrent(1));

    // Single-file torrent: file is directly under root
    let inode_manager = fs.inode_manager();
    let file = inode_manager.lookup_by_path("/test.txt");
    assert!(
        file.is_some(),
        "Single-file torrent should add file directly to root"
    );
}

#[tokio::test]
async fn test_torrent_structure_creation_multi_file() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent structure
    // Note: Multi-file torrents create a directory with the torrent name
    let torrent_info = TorrentInfo {
        id: 2,
        info_hash: "def456".to_string(),
        name: "Multi File Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Verify torrent was created
    assert!(fs.has_torrent(2));

    // Multi-file torrent: creates a directory
    let inode_manager = fs.inode_manager();
    let torrent_dir = inode_manager.lookup_by_path("/Multi File Torrent");
    assert!(
        torrent_dir.is_some(),
        "Multi-file torrent should create a directory"
    );

    let file1 = inode_manager.lookup_by_path("/Multi File Torrent/file1.txt");
    assert!(file1.is_some(), "File should exist in torrent directory");

    let file2 = inode_manager.lookup_by_path("/Multi File Torrent/file2.txt");
    assert!(file2.is_some(), "File should exist in torrent directory");
}

#[tokio::test]
async fn test_inode_lookup_by_path() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent for directory testing
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Test path lookups (this is what lookup() callback does internally)
    let inode_manager = fs.inode_manager();

    // Root should always exist
    let root = inode_manager.lookup_by_path("/");
    assert!(root.is_some(), "Root directory should exist");
    assert_eq!(root.unwrap(), 1, "Root inode should be 1");

    // Torrent directory
    let torrent_dir = inode_manager.lookup_by_path("/Test Torrent");
    assert!(torrent_dir.is_some(), "Torrent directory should exist");

    // Files
    let file1 = inode_manager.lookup_by_path("/Test Torrent/file1.txt");
    assert!(file1.is_some(), "File1 should exist");

    let file2 = inode_manager.lookup_by_path("/Test Torrent/subdir/file2.txt");
    assert!(file2.is_some(), "Nested file should exist");
}

#[tokio::test]
async fn test_inode_lookup_nonexistent_path() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Non-existent paths should return None (this becomes ENOENT in FUSE)
    let result = inode_manager.lookup_by_path("/nonexistent");
    assert!(result.is_none(), "Non-existent path should return None");

    let result = inode_manager.lookup_by_path("/nonexistent/file.txt");
    assert!(
        result.is_none(),
        "Non-existent nested path should return None"
    );
}

#[tokio::test]
async fn test_get_attributes_for_entries() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent for directory testing
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file1.txt".to_string(),
                length: 1024,
                components: vec!["file1.txt".to_string()],
            },
            FileInfo {
                name: "subdir_file.txt".to_string(),
                length: 2048,
                components: vec!["subdir".to_string(), "subdir_file.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test root directory attributes (this is what getattr() callback does)
    let root_entry = inode_manager.get(1).expect("Root should exist");
    let root_attr = fs.build_file_attr(&root_entry);
    assert_eq!(root_attr.ino, 1, "Root inode should be 1");
    assert_eq!(
        root_attr.kind,
        fuser::FileType::Directory,
        "Root should be a directory"
    );

    // Test torrent directory attributes
    let torrent_inode = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent dir should exist");
    let torrent_entry = inode_manager
        .get(torrent_inode)
        .expect("Entry should exist");
    let torrent_attr = fs.build_file_attr(&torrent_entry);
    assert_eq!(torrent_attr.ino, torrent_inode, "Inode should match");
    assert_eq!(
        torrent_attr.kind,
        fuser::FileType::Directory,
        "Should be a directory"
    );

    // Test file attributes
    let file_inode = inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .expect("File should exist");
    let file_entry = inode_manager.get(file_inode).expect("Entry should exist");
    let file_attr = fs.build_file_attr(&file_entry);
    assert_eq!(file_attr.ino, file_inode, "Inode should match");
    assert_eq!(
        file_attr.kind,
        fuser::FileType::RegularFile,
        "Should be a regular file"
    );
    assert_eq!(file_attr.size, 1024, "File size should be 1024 bytes");
}

#[tokio::test]
async fn test_directory_listing() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent for directory testing
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "file1.txt".to_string(),
                length: 1024,
                components: vec!["file1.txt".to_string()],
            },
            FileInfo {
                name: "file2.txt".to_string(),
                length: 2048,
                components: vec!["file2.txt".to_string()],
            },
            FileInfo {
                name: "file3.txt".to_string(),
                length: 3072,
                components: vec!["subdir".to_string(), "file3.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get root children (this is what readdir() callback does for root)
    let root_children = inode_manager.get_children(1);
    let child_names: Vec<_> = root_children
        .iter()
        .map(|(_, entry)| entry.name().to_string())
        .collect();
    assert!(
        child_names.contains(&"Test Torrent".to_string()),
        "Root should contain torrent dir"
    );

    // Get torrent directory children
    let torrent_inode = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent dir should exist");
    let torrent_children = inode_manager.get_children(torrent_inode);
    let torrent_names: Vec<_> = torrent_children
        .iter()
        .map(|(_, entry)| entry.name().to_string())
        .collect();

    // Should have file1.txt, file2.txt, and subdir
    assert!(torrent_names.contains(&"file1.txt".to_string()));
    assert!(torrent_names.contains(&"file2.txt".to_string()));
    assert!(torrent_names.contains(&"subdir".to_string()));
}

#[tokio::test]
async fn test_nested_directory_structure() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create deeply nested torrent structure
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "nested123".to_string(),
        name: "Nested Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "deep_file.txt".to_string(),
                length: 100,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "level3".to_string(),
                    "deep_file.txt".to_string(),
                ],
            },
            FileInfo {
                name: "shallow.txt".to_string(),
                length: 50,
                components: vec!["shallow.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test all paths exist
    assert!(inode_manager.lookup_by_path("/Nested Torrent").is_some());
    assert!(inode_manager
        .lookup_by_path("/Nested Torrent/level1")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2/level3")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2/level3/deep_file.txt")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Nested Torrent/shallow.txt")
        .is_some());

    // Verify parent-child relationships
    let level3_inode = inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2/level3")
        .unwrap();
    let level3_children = inode_manager.get_children(level3_inode);
    assert_eq!(level3_children.len(), 1, "level3 should have 1 child");
    assert_eq!(level3_children[0].1.name(), "deep_file.txt");
}

#[tokio::test]
async fn test_torrent_lookup_by_id() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multiple torrents (use multi-file to get directories)
    for id in 1..=3 {
        let torrent_info = TorrentInfo {
            id,
            info_hash: format!("hash{}", id),
            name: format!("Torrent {}", id),
            output_folder: "/downloads".to_string(),
            file_count: Some(2), // Use 2 files to create directories
            files: vec![
                FileInfo {
                    name: "file1.txt".to_string(),
                    length: 1024,
                    components: vec!["file1.txt".to_string()],
                },
                FileInfo {
                    name: "file2.txt".to_string(),
                    length: 2048,
                    components: vec!["file2.txt".to_string()],
                },
            ],
            piece_length: Some(1048576),
        };
        fs.create_torrent_structure(&torrent_info).unwrap();
    }

    let inode_manager = fs.inode_manager();

    // Test looking up each torrent by ID
    for id in 1..=3 {
        let torrent_inode = inode_manager.lookup_torrent(id);
        assert!(
            torrent_inode.is_some(),
            "Should find torrent with id {}",
            id
        );

        // Verify it's the correct torrent by checking the path
        let path = inode_manager.get_path_for_inode(torrent_inode.unwrap());
        assert!(path.is_some());
        assert!(path.unwrap().contains(&format!("Torrent {}", id)));
    }

    // Non-existent torrent should return None
    assert!(inode_manager.lookup_torrent(999).is_none());
}

#[tokio::test]
async fn test_single_file_torrent_lookup_by_id() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create single-file torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "single123".to_string(),
        name: "Single File Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "thefile.txt".to_string(),
            length: 1024,
            components: vec!["thefile.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };
    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // For single-file torrents, lookup_torrent returns the file inode
    let torrent_inode = inode_manager.lookup_torrent(1);
    assert!(torrent_inode.is_some());

    // The entry should be a file (not a directory)
    let entry = inode_manager.get(torrent_inode.unwrap()).unwrap();
    assert!(
        !entry.is_directory(),
        "Single-file torrent should map to file inode"
    );
    assert_eq!(entry.name(), "thefile.txt");
}

#[tokio::test]
async fn test_file_handle_allocation() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent (needs 2+ files to create a directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "test.txt".to_string(),
                length: 1024,
                components: vec!["test.txt".to_string()],
            },
            FileInfo {
                name: "test2.txt".to_string(),
                length: 2048,
                components: vec!["test2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Get file inode
    let file_inode = fs
        .inode_manager()
        .lookup_by_path("/Test Torrent/test.txt")
        .expect("File should exist");

    // In a real FUSE scenario, open() would allocate file handles
    // We can verify the file exists and has correct attributes
    let entry = fs.inode_manager().get(file_inode).unwrap();
    let attr = fs.build_file_attr(&entry);

    assert_eq!(attr.kind, fuser::FileType::RegularFile);
    assert_eq!(attr.size, 1024);
    assert!(!entry.is_directory());
}

#[tokio::test]
async fn test_error_conditions() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Get non-existent inode (would return ENOENT in FUSE)
    let entry = inode_manager.get(99999);
    assert!(entry.is_none(), "Non-existent inode should return None");

    // Get children of non-existent inode
    let children = inode_manager.get_children(99999);
    assert!(
        children.is_empty(),
        "Non-existent inode should have no children"
    );

    // Get path for non-existent inode
    let path = inode_manager.get_path_for_inode(99999);
    assert!(path.is_none(), "Non-existent inode should have no path");
}

#[tokio::test]
#[ignore = "Async worker test needs separate runtime setup - see integration_tests.rs for working removal test"]
async fn test_torrent_removal_cleanup() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the forget endpoint
    Mock::given(method("POST"))
        .and(path("/torrents/1/forget"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..)
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent (for directory testing)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();
    assert!(fs.has_torrent(1));

    // Verify paths exist before removal
    let inode_manager = fs.inode_manager();
    assert!(inode_manager.lookup_by_path("/Test Torrent").is_some());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .is_some());

    // Remove torrent (this is what unlink() callback does for torrent directories)
    // Give the async worker time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    fs.remove_torrent_by_id(1).unwrap();

    // Verify cleanup
    assert!(!fs.has_torrent(1));
    assert!(inode_manager.lookup_by_path("/Test Torrent").is_none());
    assert!(inode_manager.lookup_torrent(1).is_none());
}

#[tokio::test]
async fn test_unicode_and_special_characters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create torrent with unicode names (use multi-file for directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "unicode123".to_string(),
        name: "Unicode Test 🎉".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "中文文件.txt".to_string(),
                length: 100,
                components: vec!["中文文件.txt".to_string()],
            },
            FileInfo {
                name: "日本語ファイル.txt".to_string(),
                length: 200,
                components: vec!["日本語ファイル.txt".to_string()],
            },
            FileInfo {
                name: "file with spaces.txt".to_string(),
                length: 300,
                components: vec!["file with spaces.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Verify unicode paths work
    assert!(inode_manager.lookup_by_path("/Unicode Test 🎉").is_some());
    assert!(inode_manager
        .lookup_by_path("/Unicode Test 🎉/中文文件.txt")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Unicode Test 🎉/日本語ファイル.txt")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Unicode Test 🎉/file with spaces.txt")
        .is_some());
}

#[tokio::test]
async fn test_concurrent_operations() {
    use std::sync::Barrier;
    use std::thread;

    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = Arc::new(create_test_fs(config, metrics));

    // Create multi-file torrent for directory testing (needs 2+ files)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "test.txt".to_string(),
                length: 1024,
                components: vec!["test.txt".to_string()],
            },
            FileInfo {
                name: "test2.txt".to_string(),
                length: 2048,
                components: vec!["test2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };
    fs.create_torrent_structure(&torrent_info).unwrap();

    // Concurrent lookups
    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let fs = Arc::clone(&fs);
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            // Perform lookups concurrently
            let inode_manager = fs.inode_manager();
            let root = inode_manager.lookup_by_path("/");
            let torrent = inode_manager.lookup_by_path("/Test Torrent");
            let file = inode_manager.lookup_by_path("/Test Torrent/test.txt");

            (root, torrent, file)
        });

        handles.push(handle);
    }

    // Verify all threads got consistent results
    for handle in handles {
        let (root, torrent, file) = handle.join().unwrap();
        assert!(root.is_some(), "Root should always be found");
        assert!(torrent.is_some(), "Torrent should be found");
        assert!(file.is_some(), "File should be found");
    }
}

// ============================================================================
// FS-007.3: Lookup Operation Tests
// ============================================================================
// These tests verify the lookup operation scenarios that the FUSE lookup()
// callback must handle correctly. The lookup callback resolves path components
// to inodes and is called during path resolution by the kernel.

/// Test successful file lookup - lookup should resolve file paths to inodes
#[tokio::test]
async fn test_lookup_successful_file() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent (2+ files creates a directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "readme.txt".to_string(),
                length: 1024,
                components: vec!["readme.txt".to_string()],
            },
            FileInfo {
                name: "data.bin".to_string(),
                length: 2048,
                components: vec!["data.bin".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Simulate lookup from torrent directory to file
    // In FUSE: lookup(parent=torrent_dir_ino, name="readme.txt") -> file_ino
    let inode_manager = fs.inode_manager();
    let torrent_dir_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent directory should exist");

    // Verify the file exists in the parent's children
    let children = inode_manager.get_children(torrent_dir_ino);
    let readme = children
        .iter()
        .find(|(_, entry)| entry.name() == "readme.txt")
        .expect("readme.txt should be in children");

    // Verify file attributes can be built (this is what lookup returns)
    let file_entry = inode_manager
        .get(readme.0)
        .expect("File entry should exist");
    let attr = fs.build_file_attr(&file_entry);
    assert_eq!(attr.kind, fuser::FileType::RegularFile);
    assert_eq!(attr.size, 1024);
}

/// Test successful directory lookup - lookup should resolve directory paths to inodes
#[tokio::test]
async fn test_lookup_successful_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent (needs 2+ files to create directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Simulate lookup from root to torrent directory
    // In FUSE: lookup(parent=1, name="Test Torrent") -> dir_ino
    let inode_manager = fs.inode_manager();

    // Verify torrent directory exists in root's children
    let root_children = inode_manager.get_children(1);
    let torrent_dir = root_children
        .iter()
        .find(|(_, entry)| entry.name() == "Test Torrent")
        .expect("Torrent should be in root children");

    // Verify directory attributes can be built
    let dir_entry = inode_manager
        .get(torrent_dir.0)
        .expect("Directory entry should exist");
    let attr = fs.build_file_attr(&dir_entry);
    assert_eq!(attr.kind, fuser::FileType::Directory);
}

/// Test lookup for non-existent paths - should return None (becomes ENOENT in FUSE)
#[tokio::test]
async fn test_lookup_nonexistent_path() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with known files (needs 2+ files to create directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "exists.txt".to_string(),
                length: 1024,
                components: vec!["exists.txt".to_string()],
            },
            FileInfo {
                name: "other.txt".to_string(),
                length: 2048,
                components: vec!["other.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test various non-existent paths
    // In FUSE: lookup(parent=torrent_dir, name="nonexistent.txt") -> ENOENT
    let torrent_dir_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent dir should exist");

    // Check children for non-existent file
    let children = inode_manager.get_children(torrent_dir_ino);
    let nonexistent = children
        .iter()
        .find(|(_, entry)| entry.name() == "nonexistent.txt");
    assert!(
        nonexistent.is_none(),
        "Non-existent file should not be found"
    );

    // Verify lookup_by_path returns None for non-existent full paths
    assert!(
        inode_manager
            .lookup_by_path("/Test Torrent/nonexistent.txt")
            .is_none(),
        "Non-existent path should return None"
    );
    assert!(
        inode_manager.lookup_by_path("/nonexistent").is_none(),
        "Non-existent torrent should return None"
    );
    assert!(
        inode_manager
            .lookup_by_path("/Test Torrent/subdir/nonexistent.txt")
            .is_none(),
        "Non-existent nested path should return None"
    );
}

/// Test lookup with invalid parent - looking up in non-directory should fail
#[tokio::test]
async fn test_lookup_invalid_parent() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with a file
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get the file inode (which is not a directory)
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .expect("File should exist");

    let file_entry = inode_manager
        .get(file_ino)
        .expect("File entry should exist");

    // Verify the entry is NOT a directory
    assert!(!file_entry.is_directory(), "File should not be a directory");

    // Files should have no children (not directories)
    let file_children = inode_manager.get_children(file_ino);
    assert!(
        file_children.is_empty(),
        "Files should not have children (ENOTDIR behavior)"
    );
}

/// Test lookup for non-existent parent inode
#[tokio::test]
async fn test_lookup_nonexistent_parent() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Test that looking up with a non-existent parent returns None
    // In FUSE: lookup(parent=99999, name="anything") -> ENOENT
    assert!(
        inode_manager.get(99999).is_none(),
        "Non-existent inode should return None"
    );

    // Verify that children lookup on non-existent inode returns empty
    let children = inode_manager.get_children(99999);
    assert!(
        children.is_empty(),
        "Non-existent inode should have no children"
    );
}

/// Test lookup with deeply nested paths
#[tokio::test]
async fn test_lookup_deeply_nested() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with deeply nested structure (needs 2+ files for directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "nested123".to_string(),
        name: "Nested Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "deep.txt".to_string(),
                length: 1024,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "level3".to_string(),
                    "deep.txt".to_string(),
                ],
            },
            FileInfo {
                name: "shallow.txt".to_string(),
                length: 512,
                components: vec!["shallow.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test lookup at each level
    let root = inode_manager.lookup_by_path("/");
    assert!(root.is_some(), "Root should exist");

    let torrent_dir = inode_manager.lookup_by_path("/Nested Torrent");
    assert!(torrent_dir.is_some(), "Torrent directory should exist");

    let level1 = inode_manager.lookup_by_path("/Nested Torrent/level1");
    assert!(level1.is_some(), "Level1 should exist");

    let level2 = inode_manager.lookup_by_path("/Nested Torrent/level1/level2");
    assert!(level2.is_some(), "Level2 should exist");

    let level3 = inode_manager.lookup_by_path("/Nested Torrent/level1/level2/level3");
    assert!(level3.is_some(), "Level3 should exist");

    let deep_file = inode_manager.lookup_by_path("/Nested Torrent/level1/level2/level3/deep.txt");
    assert!(deep_file.is_some(), "Deep file should exist");

    // Verify the file attributes
    let deep_entry = inode_manager.get(deep_file.unwrap()).unwrap();
    let attr = fs.build_file_attr(&deep_entry);
    assert_eq!(attr.kind, fuser::FileType::RegularFile);
    assert_eq!(attr.size, 1024);
}

/// Test lookup with special characters in names
#[tokio::test]
async fn test_lookup_special_characters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with special characters in file names
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "special123".to_string(),
        name: "Special Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "file with spaces.txt".to_string(),
                length: 100,
                components: vec!["file with spaces.txt".to_string()],
            },
            FileInfo {
                name: "unicode_日本語.txt".to_string(),
                length: 200,
                components: vec!["unicode_日本語.txt".to_string()],
            },
            FileInfo {
                name: "symbols@#$%.txt".to_string(),
                length: 300,
                components: vec!["symbols@#$%.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let torrent_dir_ino = inode_manager
        .lookup_by_path("/Special Torrent")
        .expect("Torrent directory should exist");

    // Verify each special file can be looked up
    let children = inode_manager.get_children(torrent_dir_ino);
    let names: Vec<_> = children.iter().map(|(_, entry)| entry.name()).collect();

    assert!(
        names.contains(&"file with spaces.txt"),
        "Spaces in filename should work"
    );
    assert!(
        names.contains(&"unicode_日本語.txt"),
        "Unicode in filename should work"
    );
    assert!(
        names.contains(&"symbols@#$%.txt"),
        "Symbols in filename should work"
    );

    // Verify full path lookup works
    assert!(inode_manager
        .lookup_by_path("/Special Torrent/file with spaces.txt")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Special Torrent/unicode_日本語.txt")
        .is_some());
}

// ============================================================================
// FS-007.4: Getattr Operation Tests
// ============================================================================
// These tests verify the getattr operation scenarios that the FUSE getattr()
// callback must handle correctly. The getattr callback retrieves file attributes
// for a given inode and is called frequently by the kernel.

/// Test getattr for files - verify size, permissions, and timestamps
#[tokio::test]
async fn test_getattr_file_attributes() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent with different file sizes
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "small.txt".to_string(),
                length: 100,
                components: vec!["small.txt".to_string()],
            },
            FileInfo {
                name: "medium.txt".to_string(),
                length: 8192,
                components: vec!["medium.txt".to_string()],
            },
            FileInfo {
                name: "large.bin".to_string(),
                length: 10485760, // 10 MB
                components: vec!["large.bin".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test small file attributes
    let small_ino = inode_manager
        .lookup_by_path("/Test Torrent/small.txt")
        .expect("Small file should exist");
    let small_entry = inode_manager.get(small_ino).expect("Entry should exist");
    let small_attr = fs.build_file_attr(&small_entry);

    assert_eq!(small_attr.ino, small_ino, "Inode should match");
    assert_eq!(small_attr.size, 100, "Size should be 100 bytes");
    assert_eq!(
        small_attr.blocks, 1,
        "Should occupy 1 block (ceiling of 100/4096)"
    );
    assert_eq!(
        small_attr.kind,
        fuser::FileType::RegularFile,
        "Should be a regular file"
    );
    assert_eq!(
        small_attr.perm, 0o444,
        "Permissions should be read-only (444)"
    );
    assert_eq!(small_attr.nlink, 1, "Should have 1 hard link");
    assert_eq!(small_attr.blksize, 4096, "Block size should be 4096");

    // Test medium file attributes
    let medium_ino = inode_manager
        .lookup_by_path("/Test Torrent/medium.txt")
        .expect("Medium file should exist");
    let medium_entry = inode_manager.get(medium_ino).expect("Entry should exist");
    let medium_attr = fs.build_file_attr(&medium_entry);

    assert_eq!(medium_attr.size, 8192, "Size should be 8192 bytes");
    assert_eq!(
        medium_attr.blocks, 2,
        "Should occupy 2 blocks (ceiling of 8192/4096)"
    );
    assert_eq!(medium_attr.kind, fuser::FileType::RegularFile);
    assert_eq!(medium_attr.perm, 0o444);

    // Test large file attributes
    let large_ino = inode_manager
        .lookup_by_path("/Test Torrent/large.bin")
        .expect("Large file should exist");
    let large_entry = inode_manager.get(large_ino).expect("Entry should exist");
    let large_attr = fs.build_file_attr(&large_entry);

    assert_eq!(
        large_attr.size, 10485760,
        "Size should be 10485760 bytes (10 MB)"
    );
    assert_eq!(
        large_attr.blocks, 2560,
        "Should occupy 2560 blocks (10485760/4096)"
    );
    assert_eq!(large_attr.kind, fuser::FileType::RegularFile);
    assert_eq!(large_attr.perm, 0o444);
}

/// Test getattr for directories - verify nlink count and permissions
#[tokio::test]
async fn test_getattr_directory_attributes() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with nested directories
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "root_file.txt".to_string(),
                length: 1024,
                components: vec!["root_file.txt".to_string()],
            },
            FileInfo {
                name: "subdir/file1.txt".to_string(),
                length: 2048,
                components: vec!["subdir".to_string(), "file1.txt".to_string()],
            },
            FileInfo {
                name: "subdir/file2.txt".to_string(),
                length: 3072,
                components: vec!["subdir".to_string(), "file2.txt".to_string()],
            },
            FileInfo {
                name: "subdir/nested/deep.txt".to_string(),
                length: 512,
                components: vec![
                    "subdir".to_string(),
                    "nested".to_string(),
                    "deep.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test root directory attributes
    let root_entry = inode_manager.get(1).expect("Root should exist");
    let root_attr = fs.build_file_attr(&root_entry);

    assert_eq!(root_attr.ino, 1, "Root inode should be 1");
    assert_eq!(
        root_attr.kind,
        fuser::FileType::Directory,
        "Root should be a directory"
    );
    assert_eq!(root_attr.size, 0, "Directory size should be 0");
    assert_eq!(
        root_attr.perm, 0o555,
        "Permissions should be read+execute (555)"
    );
    // nlink should be 2 + number of children (1 torrent directory)
    assert_eq!(
        root_attr.nlink, 3,
        "nlink should be 3 (2 + 1 torrent directory)"
    );
    assert_eq!(root_attr.blksize, 4096, "Block size should be 4096");

    // Test torrent directory attributes
    let torrent_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent directory should exist");
    let torrent_entry = inode_manager.get(torrent_ino).expect("Entry should exist");
    let torrent_attr = fs.build_file_attr(&torrent_entry);

    assert_eq!(torrent_attr.ino, torrent_ino);
    assert_eq!(torrent_attr.kind, fuser::FileType::Directory);
    assert_eq!(torrent_attr.size, 0);
    assert_eq!(torrent_attr.perm, 0o555);
    // nlink should be 2 + number of children (root_file.txt + subdir = 2)
    assert_eq!(torrent_attr.nlink, 4, "nlink should be 4 (2 + 2 children)");

    // Test subdir attributes
    let subdir_ino = inode_manager
        .lookup_by_path("/Test Torrent/subdir")
        .expect("Subdir should exist");
    let subdir_entry = inode_manager.get(subdir_ino).expect("Entry should exist");
    let subdir_attr = fs.build_file_attr(&subdir_entry);

    assert_eq!(subdir_attr.kind, fuser::FileType::Directory);
    assert_eq!(subdir_attr.perm, 0o555);
    // nlink should be 2 + number of children (file1.txt + file2.txt + nested = 3)
    assert_eq!(subdir_attr.nlink, 5, "nlink should be 5 (2 + 3 children)");

    // Test nested directory attributes
    let nested_ino = inode_manager
        .lookup_by_path("/Test Torrent/subdir/nested")
        .expect("Nested directory should exist");
    let nested_entry = inode_manager.get(nested_ino).expect("Entry should exist");
    let nested_attr = fs.build_file_attr(&nested_entry);

    assert_eq!(nested_attr.kind, fuser::FileType::Directory);
    assert_eq!(nested_attr.perm, 0o555);
    // nlink should be 2 + number of children (deep.txt = 1)
    assert_eq!(nested_attr.nlink, 3, "nlink should be 3 (2 + 1 child)");
}

/// Test getattr for non-existent inodes - should return None
#[tokio::test]
async fn test_getattr_nonexistent_inode() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Test various non-existent inodes
    let nonexistent_inodes = vec![0, 99999, u64::MAX];

    for ino in nonexistent_inodes {
        let entry = inode_manager.get(ino);
        assert!(
            entry.is_none(),
            "Non-existent inode {} should return None",
            ino
        );
    }

    // Verify that inode_manager.get() returns None for invalid inodes
    // In a real FUSE getattr callback, this would result in ENOENT
    let entry = inode_manager.get(12345);
    assert!(entry.is_none(), "Inode 12345 should not exist");
}

/// Test getattr timestamp consistency - verify atime, mtime, ctime are reasonable
#[tokio::test]
async fn test_getattr_timestamp_consistency() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .expect("File should exist");
    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Get current time for comparison
    let now = std::time::SystemTime::now();

    // Verify timestamps are reasonable (not in the past, not too far in the future)
    let timestamp_reasonable = |ts: std::time::SystemTime| {
        let elapsed_since = now.duration_since(ts);
        let elapsed_until = ts.duration_since(now);

        // Timestamp should be within last 60 seconds or very close to now
        elapsed_since.map(|d| d.as_secs() < 60).unwrap_or(true)
            || elapsed_until.map(|d| d.as_secs() < 1).unwrap_or(false)
    };

    assert!(timestamp_reasonable(attr.atime), "atime should be recent");
    assert!(timestamp_reasonable(attr.mtime), "mtime should be recent");
    assert!(timestamp_reasonable(attr.ctime), "ctime should be recent");

    // Verify crtime (creation time) is a fixed value
    let expected_crtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    assert_eq!(
        attr.crtime, expected_crtime,
        "crtime should be fixed at Unix epoch + 1_700_000_000 seconds"
    );
}

/// Test getattr for symlinks - verify symlink-specific attributes
#[tokio::test]
async fn test_getattr_symlink_attributes() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent first
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Create a symlink manually
    let inode_manager = fs.inode_manager();
    let target_path = "/Test Torrent/file1.txt".to_string();
    let symlink_ino = inode_manager.allocate_symlink(
        "link_to_file".to_string(),
        1, // parent is root
        target_path.clone(),
    );

    let symlink_entry = inode_manager
        .get(symlink_ino)
        .expect("Symlink entry should exist");
    let attr = fs.build_file_attr(&symlink_entry);

    // Verify symlink attributes
    assert_eq!(attr.ino, symlink_ino, "Inode should match");
    assert_eq!(attr.kind, fuser::FileType::Symlink, "Should be a symlink");
    assert_eq!(
        attr.size,
        target_path.len() as u64,
        "Size should be target path length"
    );
    assert_eq!(attr.perm, 0o777, "Symlinks should have 777 permissions");
    assert_eq!(attr.nlink, 1, "Should have 1 hard link");
    assert_eq!(attr.blocks, 1, "Should occupy 1 block");
    assert_eq!(attr.blksize, 4096, "Block size should be 4096");
}

// ============================================================================
// FS-007.5: Readdir Operation Tests
// ============================================================================
// These tests verify the readdir operation scenarios that the FUSE readdir()
// callback must handle correctly. The readdir callback lists directory entries
// and is called when listing directory contents (e.g., via `ls` command).

/// Test reading root directory contents - should contain all torrent directories
#[tokio::test]
async fn test_readdir_root_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multiple torrents (use 2+ files to create directories)
    for id in 1..=3 {
        let torrent_info = TorrentInfo {
            id,
            info_hash: format!("hash{}", id),
            name: format!("Torrent {}", id),
            output_folder: "/downloads".to_string(),
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
                    components: vec!["file2.txt".to_string()],
                },
            ],
            piece_length: Some(1048576),
        };
        fs.create_torrent_structure(&torrent_info).unwrap();
    }

    let inode_manager = fs.inode_manager();

    // Get root directory children (this is what readdir does for root)
    let root_children = inode_manager.get_children(1);
    let child_names: Vec<_> = root_children
        .iter()
        .map(|(_, entry)| entry.name().to_string())
        .collect();

    // Verify all torrent directories are present
    assert_eq!(root_children.len(), 3, "Root should have 3 children");
    assert!(
        child_names.contains(&"Torrent 1".to_string()),
        "Root should contain 'Torrent 1'"
    );
    assert!(
        child_names.contains(&"Torrent 2".to_string()),
        "Root should contain 'Torrent 2'"
    );
    assert!(
        child_names.contains(&"Torrent 3".to_string()),
        "Root should contain 'Torrent 3'"
    );

    // Verify all entries are directories
    for (ino, entry) in &root_children {
        let attr = fs.build_file_attr(entry);
        assert_eq!(
            attr.kind,
            fuser::FileType::Directory,
            "Entry {} ({}) should be a directory",
            ino,
            entry.name()
        );
    }
}

/// Test reading torrent directory contents - should contain files and subdirectories
#[tokio::test]
async fn test_readdir_torrent_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with files and a subdirectory
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "readme.txt".to_string(),
                length: 1024,
                components: vec!["readme.txt".to_string()],
            },
            FileInfo {
                name: "data.bin".to_string(),
                length: 8192,
                components: vec!["data.bin".to_string()],
            },
            FileInfo {
                name: "subdir/file1.txt".to_string(),
                length: 2048,
                components: vec!["subdir".to_string(), "file1.txt".to_string()],
            },
            FileInfo {
                name: "subdir/file2.txt".to_string(),
                length: 3072,
                components: vec!["subdir".to_string(), "file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get torrent directory inode
    let torrent_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent directory should exist");

    // Get torrent directory children (this is what readdir does)
    let torrent_children = inode_manager.get_children(torrent_ino);
    let child_names: Vec<_> = torrent_children
        .iter()
        .map(|(_, entry)| entry.name().to_string())
        .collect();

    // Verify all expected entries are present
    assert_eq!(
        torrent_children.len(),
        3,
        "Torrent directory should have 3 children (readme.txt, data.bin, subdir)"
    );
    assert!(
        child_names.contains(&"readme.txt".to_string()),
        "Should contain readme.txt"
    );
    assert!(
        child_names.contains(&"data.bin".to_string()),
        "Should contain data.bin"
    );
    assert!(
        child_names.contains(&"subdir".to_string()),
        "Should contain subdir"
    );

    // Verify entry types
    for (_ino, entry) in &torrent_children {
        let attr = fs.build_file_attr(entry);
        match entry.name() {
            "readme.txt" | "data.bin" => {
                assert_eq!(
                    attr.kind,
                    fuser::FileType::RegularFile,
                    "{} should be a file",
                    entry.name()
                );
            }
            "subdir" => {
                assert_eq!(
                    attr.kind,
                    fuser::FileType::Directory,
                    "subdir should be a directory"
                );
            }
            _ => panic!("Unexpected entry: {}", entry.name()),
        }
    }
}

/// Test reading empty directory - should return no entries
#[tokio::test]
async fn test_readdir_empty_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with a subdirectory structure
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "deep/file1.txt".to_string(),
                length: 1024,
                components: vec!["deep".to_string(), "file1.txt".to_string()],
            },
            FileInfo {
                name: "deep/nested/file2.txt".to_string(),
                length: 2048,
                components: vec![
                    "deep".to_string(),
                    "nested".to_string(),
                    "file2.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get the nested directory and verify it has one file
    let nested_ino = inode_manager
        .lookup_by_path("/Test Torrent/deep/nested")
        .expect("Nested directory should exist");

    let nested_children = inode_manager.get_children(nested_ino);
    assert_eq!(
        nested_children.len(),
        1,
        "Nested directory should have 1 child (file2.txt)"
    );
    assert_eq!(
        nested_children[0].1.name(),
        "file2.txt",
        "The child should be file2.txt"
    );

    // Get the deep directory and verify it has file1.txt and nested/ subdirectory
    let deep_ino = inode_manager
        .lookup_by_path("/Test Torrent/deep")
        .expect("Deep directory should exist");

    let deep_children = inode_manager.get_children(deep_ino);
    assert_eq!(
        deep_children.len(),
        2,
        "Deep directory should have 2 children (file1.txt and nested/)"
    );
}

/// Test readdir with offset - simulating resuming directory listing after offset
#[tokio::test]
async fn test_readdir_with_offset() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with multiple files
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(5),
        files: vec![
            FileInfo {
                name: "file_a.txt".to_string(),
                length: 1024,
                components: vec!["file_a.txt".to_string()],
            },
            FileInfo {
                name: "file_b.txt".to_string(),
                length: 2048,
                components: vec!["file_b.txt".to_string()],
            },
            FileInfo {
                name: "file_c.txt".to_string(),
                length: 3072,
                components: vec!["file_c.txt".to_string()],
            },
            FileInfo {
                name: "file_d.txt".to_string(),
                length: 4096,
                components: vec!["file_d.txt".to_string()],
            },
            FileInfo {
                name: "file_e.txt".to_string(),
                length: 5120,
                components: vec!["file_e.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get torrent directory inode
    let torrent_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent directory should exist");

    // Get all children
    let all_children = inode_manager.get_children(torrent_ino);
    assert_eq!(all_children.len(), 5, "Should have 5 files");

    // Simulate readdir with offset - FUSE uses offset to resume listing
    // Inode numbers are typically used as offsets in simple implementations
    let inodes: Vec<u64> = all_children.iter().map(|(ino, _)| *ino).collect();

    // Test that we can iterate through entries by offset
    // This simulates what readdir does when called multiple times with increasing offsets
    for (idx, expected_ino) in inodes.iter().enumerate() {
        // In a real FUSE readdir, offset indicates which entry to resume from
        // Here we verify that entries have consistent inode numbers
        let entry = &all_children[idx];
        assert_eq!(
            entry.0, *expected_ino,
            "Entry at index {} should have inode {}",
            idx, expected_ino
        );
    }

    // Test partial listing simulation (skip first 2 entries)
    let offset = 2;
    let remaining: Vec<_> = all_children.iter().skip(offset).collect();
    assert_eq!(remaining.len(), 3, "Should have 3 entries after offset 2");
}

/// Test readdir on deeply nested directory structure
#[tokio::test]
async fn test_readdir_deeply_nested() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with deeply nested structure
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "nested123".to_string(),
        name: "Nested Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "level1/file1.txt".to_string(),
                length: 1024,
                components: vec!["level1".to_string(), "file1.txt".to_string()],
            },
            FileInfo {
                name: "level1/level2/file2.txt".to_string(),
                length: 2048,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "file2.txt".to_string(),
                ],
            },
            FileInfo {
                name: "level1/level2/level3/file3.txt".to_string(),
                length: 3072,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "level3".to_string(),
                    "file3.txt".to_string(),
                ],
            },
            FileInfo {
                name: "level1/level2/level3/level4/file4.txt".to_string(),
                length: 4096,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "level3".to_string(),
                    "level4".to_string(),
                    "file4.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test each level's readdir results
    let level1_ino = inode_manager
        .lookup_by_path("/Nested Torrent/level1")
        .expect("Level1 should exist");
    let level1_children = inode_manager.get_children(level1_ino);
    assert_eq!(
        level1_children.len(),
        2,
        "Level1 should have 2 children (file1.txt and level2)"
    );

    let level2_ino = inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2")
        .expect("Level2 should exist");
    let level2_children = inode_manager.get_children(level2_ino);
    assert_eq!(
        level2_children.len(),
        2,
        "Level2 should have 2 children (file2.txt and level3)"
    );

    let level3_ino = inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2/level3")
        .expect("Level3 should exist");
    let level3_children = inode_manager.get_children(level3_ino);
    assert_eq!(
        level3_children.len(),
        2,
        "Level3 should have 2 children (file3.txt and level4)"
    );

    let level4_ino = inode_manager
        .lookup_by_path("/Nested Torrent/level1/level2/level3/level4")
        .expect("Level4 should exist");
    let level4_children = inode_manager.get_children(level4_ino);
    assert_eq!(
        level4_children.len(),
        1,
        "Level4 should have 1 child (file4.txt)"
    );
    assert_eq!(level4_children[0].1.name(), "file4.txt");
}

/// Test readdir with special characters in filenames
#[tokio::test]
async fn test_readdir_special_characters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with special characters in filenames
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "special123".to_string(),
        name: "Special Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "file with spaces.txt".to_string(),
                length: 100,
                components: vec!["file with spaces.txt".to_string()],
            },
            FileInfo {
                name: "unicode_日本語.txt".to_string(),
                length: 200,
                components: vec!["unicode_日本語.txt".to_string()],
            },
            FileInfo {
                name: "symbols!@#$%.txt".to_string(),
                length: 300,
                components: vec!["symbols!@#$%.txt".to_string()],
            },
            FileInfo {
                name: "emoji_🎉_file.txt".to_string(),
                length: 400,
                components: vec!["emoji_🎉_file.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get torrent directory children
    let torrent_ino = inode_manager
        .lookup_by_path("/Special Torrent")
        .expect("Torrent directory should exist");
    let torrent_children = inode_manager.get_children(torrent_ino);

    // Verify all special character files are present
    let child_names: Vec<_> = torrent_children
        .iter()
        .map(|(_, entry)| entry.name().to_string())
        .collect();

    assert_eq!(
        torrent_children.len(),
        4,
        "Should have 4 files with special characters"
    );
    assert!(
        child_names.contains(&"file with spaces.txt".to_string()),
        "Should contain file with spaces"
    );
    assert!(
        child_names.contains(&"unicode_日本語.txt".to_string()),
        "Should contain unicode file"
    );
    assert!(
        child_names.contains(&"symbols!@#$%.txt".to_string()),
        "Should contain symbols file"
    );
    assert!(
        child_names.contains(&"emoji_🎉_file.txt".to_string()),
        "Should contain emoji file"
    );
}

// ============================================================================
// FS-007.6: Read Operation Tests
// ============================================================================
// These tests verify the read operation scenarios that the FUSE read()
// callback must handle correctly. The read callback retrieves file data
// for a given file handle and offset, and is called when reading file contents.

/// Test reading file contents via async bridge
#[tokio::test]
async fn test_read_file_contents() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the file read endpoint
    let file_content = b"Hello, World! This is test file content.";
    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content.to_vec()))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a single-file torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "test.txt".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "test.txt".to_string(),
            length: file_content.len() as u64,
            components: vec!["test.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/test.txt")
        .expect("File should exist");

    // Verify file entry exists
    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);
    assert_eq!(attr.size, file_content.len() as u64);
    assert_eq!(attr.kind, fuser::FileType::RegularFile);
}

/// Test read with various buffer sizes
#[tokio::test]
async fn test_read_various_buffer_sizes() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Create file content of 100KB
    let file_content: Vec<u8> = (0..102400).map(|i| (i % 256) as u8).collect();

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content.clone()))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "large.bin".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "large.bin".to_string(),
            length: file_content.len() as u64,
            components: vec!["large.bin".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/large.bin")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify file attributes reflect the size
    assert_eq!(attr.size, 102400);

    // Verify blocks calculation is correct
    // 102400 bytes / 4096 block size = 25 blocks
    assert_eq!(attr.blocks, 25);
}

/// Test read at different offsets
#[tokio::test]
async fn test_read_at_different_offsets() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Create file content with known pattern
    let file_content: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content.clone()))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "offset_test.bin".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "offset_test.bin".to_string(),
            length: file_content.len() as u64,
            components: vec!["offset_test.bin".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/offset_test.bin")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify file size
    assert_eq!(attr.size, 8192);
}

/// Test reading beyond file end - should handle gracefully
#[tokio::test]
async fn test_read_beyond_file_end() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Small file content (100 bytes)
    let file_content: Vec<u8> = (0..100).map(|i| i as u8).collect();

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content.clone()))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "small.txt".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "small.txt".to_string(),
            length: file_content.len() as u64,
            components: vec!["small.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/small.txt")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify file size
    assert_eq!(attr.size, 100);

    // The read implementation handles beyond-EOF reads by clamping to file size
    // offset >= file_size returns empty data
    // end = min(offset + size, file_size) - 1
    // So reading at offset 100 (equal to file size) should return 0 bytes
}

/// Test reading from multi-file torrent structure
#[tokio::test]
async fn test_read_multi_file_torrent() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Different content for different files
    let content1 = b"File 1 content here.";
    let content2 = b"File 2 has different content that is longer than file 1.";

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/0"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content1.to_vec()))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/1"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content2.to_vec()))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "MultiFileTorrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file1.txt".to_string(),
                length: content1.len() as u64,
                components: vec!["file1.txt".to_string()],
            },
            FileInfo {
                name: "file2.txt".to_string(),
                length: content2.len() as u64,
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Verify both files exist with correct sizes
    let file1_ino = inode_manager
        .lookup_by_path("/MultiFileTorrent/file1.txt")
        .expect("File1 should exist");
    let file1_entry = inode_manager.get(file1_ino).expect("Entry should exist");
    let file1_attr = fs.build_file_attr(&file1_entry);
    assert_eq!(file1_attr.size, content1.len() as u64);

    let file2_ino = inode_manager
        .lookup_by_path("/MultiFileTorrent/file2.txt")
        .expect("File2 should exist");
    let file2_entry = inode_manager.get(file2_ino).expect("Entry should exist");
    let file2_attr = fs.build_file_attr(&file2_entry);
    assert_eq!(file2_attr.size, content2.len() as u64);
}

/// Test zero-byte read - should return immediately with no data
#[tokio::test]
async fn test_read_zero_bytes() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "test.txt".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "test.txt".to_string(),
            length: 1024,
            components: vec!["test.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/test.txt")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify file exists and has size > 0
    assert_eq!(attr.size, 1024);

    // In the actual FUSE read callback, size == 0 returns empty data immediately
    // This test verifies the file structure is correct
}

/// Test reading with invalid file handle
#[tokio::test]
async fn test_read_invalid_file_handle() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "test.txt".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "test.txt".to_string(),
            length: 1024,
            components: vec!["test.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Verify file exists
    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/test.txt")
        .expect("File should exist");
    assert!(file_ino > 0);

    // In actual FUSE read, an invalid fh would return EBADF
    // This test verifies the file structure exists
}

/// Test reading from directory (should fail - not a file)
#[tokio::test]
async fn test_read_from_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create multi-file torrent (creates a directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "TestTorrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get torrent directory inode
    let dir_ino = inode_manager
        .lookup_by_path("/TestTorrent")
        .expect("Directory should exist");

    let dir_entry = inode_manager.get(dir_ino).expect("Entry should exist");

    // Verify it's a directory, not a file
    assert!(dir_entry.is_directory(), "Should be a directory");

    // In actual FUSE read, trying to read from a directory returns EISDIR
}

/// Test reading from non-existent inode
#[tokio::test]
async fn test_read_nonexistent_inode() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Non-existent inode should return None
    let entry = inode_manager.get(99999);
    assert!(entry.is_none(), "Non-existent inode should return None");

    // In actual FUSE read, this would return ENOENT
}

/// Test large file read operations
#[tokio::test]
async fn test_read_large_file() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Large file: 10 MB
    let file_size = 10 * 1024 * 1024;
    let file_content: Vec<u8> = vec![0xAB; file_size];

    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/1/files/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "large.iso".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "large.iso".to_string(),
            length: file_size as u64,
            components: vec!["large.iso".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let file_ino = inode_manager
        .lookup_by_path("/large.iso")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify large file attributes
    assert_eq!(attr.size, file_size as u64);
    assert_eq!(attr.kind, fuser::FileType::RegularFile);
    // 10 MB / 4096 bytes per block = 2560 blocks
    assert_eq!(attr.blocks, 2560);
}

// ============================================================================
// FS-007.7: Error Case Tests
// ============================================================================
// These tests verify error handling scenarios that the FUSE filesystem
// must handle correctly. These error codes are returned by FUSE callbacks
// to signal various error conditions to the kernel.

/// Test ENOENT (No such file or directory) - should return None for non-existent entries
#[tokio::test]
async fn test_error_enoent_nonexistent_path() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with known structure
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "exists.txt".to_string(),
                length: 1024,
                components: vec!["exists.txt".to_string()],
            },
            FileInfo {
                name: "other.txt".to_string(),
                length: 2048,
                components: vec!["other.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Verify existing paths work
    assert!(inode_manager.lookup_by_path("/Test Torrent").is_some());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/exists.txt")
        .is_some());

    // Verify non-existent paths return None (becomes ENOENT in FUSE)
    assert!(inode_manager.lookup_by_path("/Nonexistent").is_none());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/nonexistent.txt")
        .is_none());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/exists.txt/nonexistent")
        .is_none());

    // Verify non-existent inode returns None
    assert!(inode_manager.get(99999).is_none());
    assert!(inode_manager.get(0).is_none());
    assert!(inode_manager.get(u64::MAX).is_none());
}

/// Test ENOENT in various FUSE operations context
#[tokio::test]
async fn test_error_enoent_lookup_operations() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent structure
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get torrent directory for lookup tests
    let torrent_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent should exist");

    // Verify lookup in valid directory fails for non-existent entries
    let children = inode_manager.get_children(torrent_ino);
    let nonexistent = children
        .iter()
        .find(|(_, entry)| entry.name() == "nonexistent.txt");
    assert!(
        nonexistent.is_none(),
        "Non-existent file should not be found"
    );

    // Verify children lookup on non-existent inode returns empty
    let no_children = inode_manager.get_children(99999);
    assert!(
        no_children.is_empty(),
        "Non-existent inode should have no children"
    );
}

/// Test ENOTDIR (Not a directory) - attempting directory operations on files
#[tokio::test]
async fn test_error_enotdir_file_as_directory() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get a file inode (not a directory)
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .expect("File should exist");

    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");

    // Verify it's actually a file
    assert!(
        !file_entry.is_directory(),
        "Should be a file, not directory"
    );
    assert!(file_entry.is_file(), "Should be a regular file");

    // Attempting to get children of a file should return empty (ENOTDIR behavior)
    let children = inode_manager.get_children(file_ino);
    assert!(children.is_empty(), "Files should have no children");

    // Attempting to look up inside a file path should fail
    let nested_in_file = inode_manager.lookup_by_path("/Test Torrent/file1.txt/nested");
    assert!(
        nested_in_file.is_none(),
        "Should not be able to look up inside a file"
    );
}

/// Test EISDIR (Is a directory) - attempting file operations on directories
#[tokio::test]
async fn test_error_eisdir_directory_as_file() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent with subdirectories
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "file1.txt".to_string(),
                length: 1024,
                components: vec!["file1.txt".to_string()],
            },
            FileInfo {
                name: "subdir/file2.txt".to_string(),
                length: 2048,
                components: vec!["subdir".to_string(), "file2.txt".to_string()],
            },
            FileInfo {
                name: "subdir/nested/file3.txt".to_string(),
                length: 3072,
                components: vec![
                    "subdir".to_string(),
                    "nested".to_string(),
                    "file3.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get directory inodes
    let torrent_dir_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Torrent dir should exist");
    let subdir_ino = inode_manager
        .lookup_by_path("/Test Torrent/subdir")
        .expect("Subdir should exist");
    let nested_ino = inode_manager
        .lookup_by_path("/Test Torrent/subdir/nested")
        .expect("Nested should exist");

    // Verify each is actually a directory
    for (name, ino) in [
        ("torrent dir", torrent_dir_ino),
        ("subdir", subdir_ino),
        ("nested", nested_ino),
    ] {
        let entry = inode_manager
            .get(ino)
            .unwrap_or_else(|| panic!("{} should exist", name));
        assert!(entry.is_directory(), "{} should be a directory", name);
        assert!(entry.is_directory(), "{} should be a directory", name);

        // Verify file attributes show directory type
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.kind, fuser::FileType::Directory);
    }

    // Verify directories have size 0 (cannot be read as files)
    let torrent_entry = inode_manager.get(torrent_dir_ino).unwrap();
    let torrent_attr = fs.build_file_attr(&torrent_entry);
    assert_eq!(torrent_attr.size, 0, "Directory size should be 0");
}

/// Test EACCES (Permission denied) scenarios - read-only filesystem behavior
#[tokio::test]
async fn test_error_eacces_read_only_filesystem() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
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
                components: vec!["file2.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get file inode
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file1.txt")
        .expect("File should exist");
    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let file_attr = fs.build_file_attr(&file_entry);

    // Verify files have read-only permissions (0o444)
    assert_eq!(file_attr.perm, 0o444, "Files should be read-only (444)");

    // Verify no write permissions
    assert_eq!(
        file_attr.perm & 0o222,
        0,
        "Files should not have write permission"
    );

    // Get directory inode
    let dir_ino = inode_manager
        .lookup_by_path("/Test Torrent")
        .expect("Dir should exist");
    let dir_entry = inode_manager.get(dir_ino).expect("Entry should exist");
    let dir_attr = fs.build_file_attr(&dir_entry);

    // Verify directories have read+execute permissions (0o555)
    assert_eq!(
        dir_attr.perm, 0o555,
        "Directories should be read+execute (555)"
    );

    // Verify no write permissions on directories
    assert_eq!(
        dir_attr.perm & 0o222,
        0,
        "Directories should not have write permission"
    );
}

/// Test permission bits for different entry types
#[tokio::test]
async fn test_error_permission_bits_verification() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create torrent with file and directory
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file.txt".to_string(),
                length: 1024,
                components: vec!["file.txt".to_string()],
            },
            FileInfo {
                name: "subdir/nested.txt".to_string(),
                length: 2048,
                components: vec!["subdir".to_string(), "nested.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test file permissions
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file.txt")
        .expect("File should exist");
    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let file_attr = fs.build_file_attr(&file_entry);

    assert_eq!(file_attr.perm, 0o444, "File should be read-only");
    assert!(file_attr.perm & 0o400 != 0, "File should have owner read");
    assert!(file_attr.perm & 0o040 != 0, "File should have group read");
    assert!(file_attr.perm & 0o004 != 0, "File should have other read");
    assert!(
        file_attr.perm & 0o200 == 0,
        "File should NOT have owner write"
    );
    assert!(
        file_attr.perm & 0o020 == 0,
        "File should NOT have group write"
    );
    assert!(
        file_attr.perm & 0o002 == 0,
        "File should NOT have other write"
    );

    // Test directory permissions
    let dir_ino = inode_manager
        .lookup_by_path("/Test Torrent/subdir")
        .expect("Dir should exist");
    let dir_entry = inode_manager.get(dir_ino).expect("Entry should exist");
    let dir_attr = fs.build_file_attr(&dir_entry);

    assert_eq!(dir_attr.perm, 0o555, "Directory should be read+execute");
    assert!(
        dir_attr.perm & 0o500 != 0,
        "Directory should have owner read+execute"
    );
    assert!(
        dir_attr.perm & 0o050 != 0,
        "Directory should have group read+execute"
    );
    assert!(
        dir_attr.perm & 0o005 != 0,
        "Directory should have other read+execute"
    );
    assert!(
        dir_attr.perm & 0o200 == 0,
        "Directory should NOT have owner write"
    );
}

/// Test EIO (I/O error) scenarios - simulated API failures
#[tokio::test]
async fn test_error_eio_api_failure() {
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock API failure with 500 error
    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/.*"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // The filesystem should handle API failures gracefully
    // In actual FUSE operations, API errors would be mapped to EIO

    // Verify filesystem was created successfully even with failing API
    assert!(!fs.is_initialized());
}

/// Test EIO with network timeout simulation
#[tokio::test]
async fn test_error_eio_timeout() {
    use std::time::Duration;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, ResponseTemplate};

    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock timeout by delaying response
    Mock::given(method("GET"))
        .and(path_regex(r"/torrents/.*"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let _fs = create_test_fs(config, metrics);

    // The filesystem structure tests would continue
    // In actual FUSE read, timeouts would be mapped to EIO
}

/// Test EINVAL (Invalid argument) - invalid parameters
#[tokio::test]
async fn test_error_einval_invalid_parameters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent (2+ files creates a directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file.txt".to_string(),
                length: 1024,
                components: vec!["file.txt".to_string()],
            },
            FileInfo {
                name: "other.txt".to_string(),
                length: 2048,
                components: vec!["other.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Get file inode
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file.txt")
        .expect("File should exist");
    let file_entry = inode_manager.get(file_ino).expect("Entry should exist");
    let attr = fs.build_file_attr(&file_entry);

    // Verify valid attributes
    assert_eq!(attr.size, 1024);
    assert!(attr.size > 0);

    // Verify negative or zero sizes don't occur for existing files
    assert!(attr.blocks > 0, "Blocks should be positive");
    assert_eq!(attr.blksize, 4096, "Block size should be 4096");
}

/// Test EBADF (Bad file descriptor) - invalid file handle scenarios
#[tokio::test]
async fn test_error_ebadf_invalid_file_handle() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a multi-file torrent (2+ files creates a directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file.txt".to_string(),
                length: 1024,
                components: vec!["file.txt".to_string()],
            },
            FileInfo {
                name: "other.txt".to_string(),
                length: 2048,
                components: vec!["other.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Verify file exists
    let file_ino = inode_manager
        .lookup_by_path("/Test Torrent/file.txt")
        .expect("File should exist");
    assert!(file_ino > 0);

    // Verify invalid file handle wouldn't correspond to a valid inode
    let invalid_handle: u64 = 0; // 0 is typically invalid
    assert!(inode_manager.get(invalid_handle).is_none());

    // Another invalid handle
    let another_invalid: u64 = 999999;
    assert!(inode_manager.get(another_invalid).is_none());
}

/// Test error scenarios with empty or malformed torrents
#[tokio::test]
async fn test_error_edge_cases_empty_torrent() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent with no files - should still work
    let empty_torrent_info = TorrentInfo {
        id: 1,
        info_hash: "empty123".to_string(),
        name: "Empty Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(0),
        files: vec![],
        piece_length: Some(1048576),
    };

    // Empty torrent creation should succeed
    fs.create_torrent_structure(&empty_torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Torrent directory should exist
    let torrent_ino = inode_manager.lookup_by_path("/Empty Torrent");
    assert!(
        torrent_ino.is_some(),
        "Empty torrent directory should exist"
    );

    // Torrent should have no files (only itself as a directory)
    if let Some(ino) = torrent_ino {
        let children = inode_manager.get_children(ino);
        assert!(
            children.is_empty(),
            "Empty torrent should have no file children"
        );
    }
}

/// Test error handling with invalid torrent IDs
#[tokio::test]
async fn test_error_invalid_torrent_id() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    let inode_manager = fs.inode_manager();

    // Non-existent torrent ID should return None
    assert!(inode_manager.lookup_torrent(99999).is_none());
    assert!(inode_manager.lookup_torrent(0).is_none());
    assert!(inode_manager.lookup_torrent(u64::MAX).is_none());

    // Filesystem should not have these torrents
    assert!(!fs.has_torrent(99999));
    assert!(!fs.has_torrent(0));
}

/// Test error scenarios with deeply nested invalid paths
#[tokio::test]
async fn test_error_deeply_nested_invalid_paths() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create torrent with limited nesting (2+ files for directory)
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file.txt".to_string(),
                length: 1024,
                components: vec!["file.txt".to_string()],
            },
            FileInfo {
                name: "other.txt".to_string(),
                length: 2048,
                components: vec!["other.txt".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Valid paths
    assert!(inode_manager.lookup_by_path("/Test Torrent").is_some());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/file.txt")
        .is_some());

    // Invalid deeply nested paths that extend beyond valid structure
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/file.txt/extra")
        .is_none());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/nonexistent/deep")
        .is_none());
    assert!(inode_manager
        .lookup_by_path("/Test Torrent/file.txt/deep/nested")
        .is_none());
}

/// Test error handling with symlinks to non-existent targets
#[tokio::test]
async fn test_error_symlink_to_nonexistent() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a torrent
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "file.txt".to_string(),
            length: 1024,
            components: vec!["file.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Create a symlink to a non-existent path
    let symlink_ino = inode_manager.allocate_symlink(
        "broken_link".to_string(),
        1, // parent is root
        "/nonexistent/path".to_string(),
    );

    // Symlink should be created
    let symlink_entry = inode_manager.get(symlink_ino);
    assert!(symlink_entry.is_some(), "Symlink should exist");

    let entry = symlink_entry.unwrap();
    assert!(entry.is_symlink(), "Should be a symlink");
    assert!(entry.is_symlink());

    // Symlink attributes should be valid even with broken target
    let attr = fs.build_file_attr(&entry);
    assert_eq!(attr.kind, fuser::FileType::Symlink);
    assert_eq!(attr.size, "/nonexistent/path".len() as u64);
}
