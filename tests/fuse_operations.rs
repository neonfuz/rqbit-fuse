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
    let api_client = Arc::new(torrent_fuse::api::client::RqbitClient::new(
        config.api.url.clone(),
        Arc::clone(&metrics.api),
    ));
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
async fn test_torrent_removal_cleanup() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the forget endpoint
    Mock::given(method("POST"))
        .and(path("/torrents/1/forget"))
        .respond_with(ResponseTemplate::new(200))
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
    let file_entry = inode_manager.get(readme.0).expect("File entry should exist");
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
    let dir_entry = inode_manager.get(torrent_dir.0).expect("Directory entry should exist");
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
    assert!(nonexistent.is_none(), "Non-existent file should not be found");

    // Verify lookup_by_path returns None for non-existent full paths
    assert!(
        inode_manager.lookup_by_path("/Test Torrent/nonexistent.txt").is_none(),
        "Non-existent path should return None"
    );
    assert!(
        inode_manager.lookup_by_path("/nonexistent").is_none(),
        "Non-existent torrent should return None"
    );
    assert!(
        inode_manager.lookup_by_path("/Test Torrent/subdir/nonexistent.txt").is_none(),
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

    let file_entry = inode_manager.get(file_ino).expect("File entry should exist");

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

    assert!(names.contains(&"file with spaces.txt"), "Spaces in filename should work");
    assert!(names.contains(&"unicode_日本語.txt"), "Unicode in filename should work");
    assert!(names.contains(&"symbols@#$%.txt"), "Symbols in filename should work");

    // Verify full path lookup works
    assert!(
        inode_manager.lookup_by_path("/Special Torrent/file with spaces.txt").is_some()
    );
    assert!(
        inode_manager.lookup_by_path("/Special Torrent/unicode_日本語.txt").is_some()
    );
}
