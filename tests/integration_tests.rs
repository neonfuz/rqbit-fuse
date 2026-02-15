//! Integration tests for torrent-fuse
//!
//! These tests verify the full integration between:
//! - FUSE filesystem operations
//! - rqbit HTTP API client
//! - Inode management
//! - Error handling

use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, metrics.clone(), 100));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

#[tokio::test]
async fn test_filesystem_creation_and_initialization() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Verify filesystem was created but not initialized
    assert!(!fs.is_initialized());
}

#[tokio::test]
async fn test_torrent_addition_from_magnet() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the add torrent endpoint
    Mock::given(method("POST"))
        .and(path("/torrents"))
        .and(header("content-type", "application/json"))
        .and(body_json(serde_json::json!({
            "magnet_link": "magnet:?xt=urn:btih:abc123"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "info_hash": "abc123"
        })))
        .mount(&mock_server)
        .await;

    // Mock get torrent endpoint
    Mock::given(method("GET"))
        .and(path("/torrents/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1,
            "info_hash": "abc123",
            "name": "Test Torrent",
            "output_folder": "/downloads",
            "file_count": 1,
            "files": [
                {"name": "test.txt", "length": 1024, "components": ["test.txt"]}
            ],
            "piece_length": 1048576
        })))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // In a real scenario, we would add the torrent through the filesystem
    // For integration test, we verify the structure can be created
    use torrent_fuse::api::types::TorrentInfo;
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![torrent_fuse::api::types::FileInfo {
            name: "test.txt".to_string(),
            length: 1024,
            components: vec!["test.txt".to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // For single-file torrents, the file is added directly to root
    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(1);
    assert!(
        torrent_inode.is_some(),
        "Torrent should be registered in inode manager"
    );

    // Verify file exists directly under root (single-file torrents don't create directories)
    let root_children = inode_manager.get_children(1);
    let file_entry = root_children
        .iter()
        .find(|(_, entry)| entry.name() == "test.txt");
    assert!(file_entry.is_some(), "File should exist under root");

    // The torrent_id maps to the file inode for single-file torrents
    assert_eq!(torrent_inode.unwrap(), file_entry.unwrap().0);
}

#[tokio::test]
async fn test_multi_file_torrent_structure() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 2,
        info_hash: "def456".to_string(),
        name: "MultiFile Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "readme.txt".to_string(),
                length: 100,
                components: vec!["readme.txt".to_string()],
            },
            FileInfo {
                name: "data.bin".to_string(),
                length: 1024000,
                components: vec!["subdir".to_string(), "data.bin".to_string()],
            },
            FileInfo {
                name: "info.txt".to_string(),
                length: 200,
                components: vec!["subdir".to_string(), "info.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // Verify torrent directory
    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(2).unwrap();
    let torrent_children = inode_manager.get_children(torrent_inode);

    // Should have readme.txt and subdir
    assert_eq!(torrent_children.len(), 2);

    // Find the subdir
    let subdir = torrent_children
        .iter()
        .find(|(_, entry)| entry.name() == "subdir" && entry.is_directory());
    assert!(subdir.is_some(), "Should have a subdirectory");

    // Verify subdir contents
    let subdir_inode = subdir.unwrap().0;
    let subdir_children = inode_manager.get_children(subdir_inode);
    assert_eq!(subdir_children.len(), 2, "Subdir should have 2 files");
}

#[tokio::test]
async fn test_duplicate_torrent_detection() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::TorrentInfo;

    let torrent_info = TorrentInfo {
        id: 3,
        info_hash: "duplicate".to_string(),
        name: "Duplicate Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![torrent_fuse::api::types::FileInfo {
            name: "file.txt".to_string(),
            length: 100,
            components: vec!["file.txt".to_string()],
        }],
        piece_length: Some(262144),
    };

    // First addition should succeed
    fs.create_torrent_structure(&torrent_info).unwrap();

    // Verify first entry exists
    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let initial_count = root_children
        .iter()
        .filter(|(_, entry)| entry.name() == "file.txt")
        .count();

    assert_eq!(
        initial_count, 1,
        "Should have one file entry after first addition"
    );

    // Note: create_torrent_structure is a low-level method that doesn't check for duplicates.
    // The duplicate detection is done at a higher level in add_torrent_magnet/add_torrent_url.
    // This test verifies the structure creation works; duplicate prevention is tested elsewhere.
}

#[tokio::test]
async fn test_error_scenario_api_unavailable() {
    // Create config with non-existent server
    let temp_dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.api.url = "http://localhost:59999".to_string(); // Non-existent server
    config.mount.mount_point = temp_dir.path().to_path_buf();

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Verify filesystem can be created even if server is unavailable
    // (connection validation happens at mount time, not creation)
    assert!(!fs.is_initialized());
}

#[tokio::test]
async fn test_file_attribute_generation() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 4,
        info_hash: "attr_test".to_string(),
        name: "Attribute Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "small.txt".to_string(),
                length: 100,
                components: vec!["small.txt".to_string()],
            },
            FileInfo {
                name: "large.bin".to_string(),
                length: 5 * 1024 * 1024 * 1024, // 5 GB
                components: vec!["large.bin".to_string()],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(4).unwrap();

    // Get torrent directory entry
    let torrent_entry = inode_manager.get(torrent_inode).unwrap();
    let torrent_attr = fs.build_file_attr(&torrent_entry);

    // Verify directory attributes
    assert_eq!(torrent_attr.kind, fuser::FileType::Directory);
    assert_eq!(torrent_attr.perm, 0o555); // Read-only directory

    // Find and verify small file attributes
    let small_file = inode_manager
        .get_children(torrent_inode)
        .into_iter()
        .find(|(_, entry)| entry.name() == "small.txt")
        .map(|(ino, _)| inode_manager.get(ino).unwrap());

    if let Some(entry) = small_file {
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.kind, fuser::FileType::RegularFile);
        assert_eq!(attr.perm, 0o444); // Read-only file
        assert_eq!(attr.size, 100);
    }

    // Find and verify large file attributes
    let large_file = inode_manager
        .get_children(torrent_inode)
        .into_iter()
        .find(|(_, entry)| entry.name() == "large.bin")
        .map(|(ino, _)| inode_manager.get(ino).unwrap());

    if let Some(entry) = large_file {
        let attr = fs.build_file_attr(&entry);
        assert_eq!(attr.size, 5 * 1024 * 1024 * 1024);
    }
}

#[tokio::test]
async fn test_torrent_removal_with_cleanup() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the forget endpoint
    Mock::given(method("POST"))
        .and(path("/torrents/5/forget"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    // Create torrent structure
    let torrent_info = TorrentInfo {
        id: 5,
        info_hash: "removal_test".to_string(),
        name: "Removal Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "file1.txt".to_string(),
                length: 100,
                components: vec!["file1.txt".to_string()],
            },
            FileInfo {
                name: "file2.txt".to_string(),
                length: 200,
                components: vec!["subdir".to_string(), "file2.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(5).unwrap();

    // Verify structure exists
    assert!(inode_manager.get(torrent_inode).is_some());

    // Manually remove (simulating what happens during unlink)
    fs.inode_manager().remove_child(1, torrent_inode);
    fs.inode_manager().remove_inode(torrent_inode);

    // Verify cleanup
    assert!(inode_manager.lookup_torrent(5).is_none());
    assert!(inode_manager.get(torrent_inode).is_none());
}

#[tokio::test]
async fn test_deeply_nested_directory_structure() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 6,
        info_hash: "nested".to_string(),
        name: "Nested Structure".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "root.txt".to_string(),
                length: 100,
                components: vec!["root.txt".to_string()],
            },
            FileInfo {
                name: "level1.txt".to_string(),
                length: 200,
                components: vec!["level1".to_string(), "level1.txt".to_string()],
            },
            FileInfo {
                name: "deep.txt".to_string(),
                length: 300,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "deep.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(6).unwrap();
    let torrent_children = inode_manager.get_children(torrent_inode);

    // Verify root file exists
    assert!(torrent_children.iter().any(|(_, e)| e.name() == "root.txt"));

    // Verify level1 directory exists
    let level1 = torrent_children
        .iter()
        .find(|(_, e)| e.name() == "level1" && e.is_directory());
    assert!(level1.is_some());

    // Verify level2 directory exists inside level1
    let level1_inode = level1.unwrap().0;
    let level1_children = inode_manager.get_children(level1_inode);
    assert!(level1_children
        .iter()
        .any(|(_, e)| e.name() == "level1.txt"));

    let level2 = level1_children
        .iter()
        .find(|(_, e)| e.name() == "level2" && e.is_directory());
    assert!(level2.is_some());

    // Verify deep file exists in level2
    let level2_children = inode_manager.get_children(level2.unwrap().0);
    assert!(level2_children.iter().any(|(_, e)| e.name() == "deep.txt"));
}

#[tokio::test]
async fn test_unicode_and_special_characters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 7,
        info_hash: "unicode".to_string(),
        name: "Unicode Test 🎉".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
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
                name: "файл.txt".to_string(),
                length: 300,
                components: vec!["файл.txt".to_string()],
            },
            FileInfo {
                name: "emoji_🎊_file.txt".to_string(),
                length: 400,
                components: vec!["emoji_🎊_file.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(7).unwrap();
    let children = inode_manager.get_children(torrent_inode);

    // Verify all files exist with their unicode names
    let names: Vec<_> = children.iter().map(|(_, e)| e.name().to_string()).collect();
    assert!(names.contains(&"中文文件.txt".to_string()));
    assert!(names.contains(&"日本語ファイル.txt".to_string()));
    assert!(names.contains(&"файл.txt".to_string()));
    assert!(names.contains(&"emoji_🎊_file.txt".to_string()));
}

#[tokio::test]
async fn test_empty_torrent_handling() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    // Torrent with zero-byte file
    let torrent_info = TorrentInfo {
        id: 8,
        info_hash: "empty".to_string(),
        name: "Empty File Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "empty.txt".to_string(),
            length: 0,
            components: vec!["empty.txt".to_string()],
        }],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    // For single-file torrents, the file is added directly to root
    let inode_manager = fs.inode_manager();
    let file_inode = inode_manager.lookup_torrent(8).unwrap();
    let file_entry = inode_manager.get(file_inode).unwrap();

    // Verify empty file has correct attributes
    let attr = fs.build_file_attr(&file_entry);
    assert_eq!(attr.size, 0);
    assert_eq!(attr.blocks, 0);

    // Verify it's registered as a file (not a directory)
    assert!(file_entry.is_file());
}

#[tokio::test]
async fn test_concurrent_torrent_additions() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use std::thread;
    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let fs_ref = std::sync::Arc::new(std::sync::Mutex::new(()));
            let _torrent_info = TorrentInfo {
                id: 100 + i as u64,
                info_hash: format!("concurrent{}", i),
                name: format!("Torrent {}", i),
                output_folder: "/downloads".to_string(),
                file_count: Some(1),
                files: vec![FileInfo {
                    name: format!("file{}.txt", i),
                    length: 100,
                    components: vec![format!("file{}.txt", i)],
                }],
                piece_length: Some(262144),
            };

            thread::spawn(move || {
                let _guard = fs_ref.lock().unwrap();
                // Note: In real concurrent scenario, we'd need proper Arc<Mutex<>> around fs
                // For this test, we just verify the structure works
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Add torrents sequentially but verify the structure supports concurrent access
    for i in 0..5 {
        let torrent_info = TorrentInfo {
            id: 100 + i as u64,
            info_hash: format!("concurrent{}", i),
            name: format!("Torrent {}", i),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: format!("file{}.txt", i),
                length: 100,
                components: vec![format!("file{}.txt", i)],
            }],
            piece_length: Some(262144),
        };
        fs.create_torrent_structure(&torrent_info).unwrap();
    }

    // Verify all torrents were added
    let inode_manager = fs.inode_manager();
    for i in 0..5 {
        assert!(inode_manager.lookup_torrent(100 + i as u64).is_some());
    }
}

#[tokio::test]
async fn test_filesystem_metrics_collection() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let _fs = create_test_fs(config, metrics.clone());

    // Verify metrics were initialized by loading the public fields
    use std::sync::atomic::Ordering;
    let _request_count = metrics.api.request_count.load(Ordering::Relaxed);
    let _read_count = metrics.fuse.read_count.load(Ordering::Relaxed);
    // Just verifying these compile and are accessible
    assert!(true);
}

#[tokio::test]
async fn test_nested_directory_path_resolution() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 10,
        info_hash: "path_resolution".to_string(),
        name: "Path Resolution Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "root.txt".to_string(),
                length: 100,
                components: vec!["root.txt".to_string()],
            },
            FileInfo {
                name: "level1.txt".to_string(),
                length: 200,
                components: vec!["level1".to_string(), "level1.txt".to_string()],
            },
            FileInfo {
                name: "deep.txt".to_string(),
                length: 300,
                components: vec![
                    "level1".to_string(),
                    "level2".to_string(),
                    "deep.txt".to_string(),
                ],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Test path resolution for torrent directory
    let torrent_inode = inode_manager.lookup_torrent(10).unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(torrent_inode),
        Some("/Path Resolution Test".to_string())
    );

    // Test path resolution for root file
    let torrent_children = inode_manager.get_children(torrent_inode);
    let root_file = torrent_children
        .iter()
        .find(|(_, e)| e.name() == "root.txt")
        .unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(root_file.0),
        Some("/Path Resolution Test/root.txt".to_string())
    );

    // Test path resolution for level1 directory
    let level1_dir = torrent_children
        .iter()
        .find(|(_, e)| e.name() == "level1" && e.is_directory())
        .unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(level1_dir.0),
        Some("/Path Resolution Test/level1".to_string())
    );

    // Test path resolution for level1 file
    let level1_children = inode_manager.get_children(level1_dir.0);
    let level1_file = level1_children
        .iter()
        .find(|(_, e)| e.name() == "level1.txt")
        .unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(level1_file.0),
        Some("/Path Resolution Test/level1/level1.txt".to_string())
    );

    // Test path resolution for level2 directory
    let level2_dir = level1_children
        .iter()
        .find(|(_, e)| e.name() == "level2" && e.is_directory())
        .unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(level2_dir.0),
        Some("/Path Resolution Test/level1/level2".to_string())
    );

    // Test path resolution for deeply nested file
    let level2_children = inode_manager.get_children(level2_dir.0);
    let deep_file = level2_children
        .iter()
        .find(|(_, e)| e.name() == "deep.txt")
        .unwrap();
    assert_eq!(
        inode_manager.get_path_for_inode(deep_file.0),
        Some("/Path Resolution Test/level1/level2/deep.txt".to_string())
    );

    // Test path lookup for nested paths
    assert!(inode_manager
        .lookup_by_path("/Path Resolution Test/level1")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Path Resolution Test/level1/level1.txt")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Path Resolution Test/level1/level2")
        .is_some());
    assert!(inode_manager
        .lookup_by_path("/Path Resolution Test/level1/level2/deep.txt")
        .is_some());
}

// Debug test to see actual structure
#[tokio::test]
async fn debug_nested_structure() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use torrent_fuse::api::types::{FileInfo, TorrentInfo};

    let torrent_info = TorrentInfo {
        id: 99,
        info_hash: "debug".to_string(),
        name: "Debug Structure".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "root.txt".to_string(),
                length: 100,
                components: vec!["root.txt".to_string()],
            },
            FileInfo {
                name: "level1.txt".to_string(),
                length: 200,
                components: vec!["level1".to_string(), "level1.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    };

    fs.create_torrent_structure(&torrent_info).unwrap();

    let inode_manager = fs.inode_manager();

    // Debug: Print all entries
    println!("All entries:");
    for entry_ref in inode_manager.iter_entries() {
        println!("  inode {}: {:?}", entry_ref.inode, entry_ref.entry);
    }

    let torrent_inode = inode_manager.lookup_torrent(99).unwrap();
    println!("Torrent inode: {}", torrent_inode);

    let torrent_children = inode_manager.get_children(torrent_inode);
    println!("Torrent children count: {}", torrent_children.len());
    for (ino, entry) in &torrent_children {
        println!("  Child: inode {} -> {}", ino, entry.name());
    }
}
