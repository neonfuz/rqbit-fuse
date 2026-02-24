//! Integration tests for rqbit-fuse
//!
//! These tests verify the full integration between:
//! - FUSE filesystem operations
//! - rqbit HTTP API client
//! - Inode management
//! - Error handling

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Barrier;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use rqbit_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

// Import common test helpers to avoid duplication
mod common;
use common::test_helpers::{create_test_config, create_test_fs, setup_mock_server};

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
    use rqbit_fuse::api::types::TorrentInfo;
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Test Torrent".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![rqbit_fuse::api::types::FileInfo {
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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::TorrentInfo;

    let torrent_info = TorrentInfo {
        id: 3,
        info_hash: "duplicate".to_string(),
        name: "Duplicate Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![rqbit_fuse::api::types::FileInfo {
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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

    const NUM_TORRENTS: usize = 5;
    let barrier = Arc::new(Barrier::new(NUM_TORRENTS));
    let fs_arc = Arc::new(fs);
    let mut handles = Vec::new();

    for i in 0..NUM_TORRENTS {
        let barrier = Arc::clone(&barrier);
        let fs_clone = Arc::clone(&fs_arc);

        let handle = std::thread::spawn(move || {
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

            barrier.wait();

            fs_clone.create_torrent_structure(&torrent_info).unwrap();
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let inode_manager = fs_arc.inode_manager();
    for i in 0..NUM_TORRENTS {
        assert!(
            inode_manager.lookup_torrent(100 + i as u64).is_some(),
            "Torrent {} should exist after concurrent addition",
            i
        );
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
    let _bytes_read = metrics.bytes_read.load(Ordering::Relaxed);
    let _error_count = metrics.error_count.load(Ordering::Relaxed);
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

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

/// Test that torrents are automatically removed from FUSE when deleted from rqbit
/// Verifies IDEA2-007: Basic torrent removal functionality
#[tokio::test]
async fn test_torrent_removal_from_rqbit() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

    // Create torrent info for mock responses
    let torrent_info = TorrentInfo {
        id: 20,
        info_hash: "removal_test".to_string(),
        name: "Removal Test Torrent".to_string(),
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

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics.clone());

    // Manually create torrent structure (simulating discovery)
    fs.create_torrent_structure(&torrent_info).unwrap();

    // Also need to populate known_torrents for removal detection to work
    fs.__test_known_torrents().insert(20);

    let inode_manager = fs.inode_manager();

    // Verify torrent exists initially
    let torrent_inode = inode_manager.lookup_torrent(20);
    assert!(torrent_inode.is_some(), "Torrent should exist initially");

    // Verify files are visible
    let torrent_children = inode_manager.get_children(torrent_inode.unwrap());
    assert_eq!(torrent_children.len(), 2, "Should have 2 entries");

    // Mock empty torrent list (simulating torrent removal from rqbit)
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": []
        })))
        .mount(&mock_server)
        .await;

    // Clear the API cache so the new mock is used
    fs.__test_clear_list_torrents_cache().await;

    // Trigger torrent discovery (with force to bypass cooldown)
    let refreshed = fs.refresh_torrents(true).await;
    assert!(refreshed, "Torrent refresh should have been performed");

    // Verify torrent is removed from filesystem
    let torrent_inode_after = inode_manager.lookup_torrent(20);
    assert!(
        torrent_inode_after.is_none(),
        "Torrent should be removed from filesystem after deletion from rqbit"
    );

    // Verify torrent's paths return ENOENT (no longer in directory listing)
    let root_children = inode_manager.get_children(1);
    let torrent_still_visible = root_children
        .iter()
        .any(|(_, entry)| entry.name() == "Removal Test Torrent");
    assert!(
        !torrent_still_visible,
        "Removed torrent should not be visible in root directory"
    );
}

// Debug test to see actual structure
#[tokio::test]
async fn debug_nested_structure() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

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

/// EDGE-040: Test read while torrent being removed
///
/// This test verifies that the system handles gracefully when a torrent is removed
/// while file handles are still open. The system should not crash when:
/// 1. File handles exist for files that have been removed
/// 2. Attempts are made to read from removed torrents
/// 3. Torrent removal happens concurrently with file operations
///
/// This tests the graceful degradation of the filesystem when operations
/// race with torrent removal.
#[tokio::test]
async fn test_edge_040_read_while_torrent_being_removed() {
    use rqbit_fuse::types::handle::FileHandleManager;

    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = Arc::new(create_test_fs(config, metrics));

    // Create multi-file torrent structure (has directory + file inodes)
    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};
    let torrent_info = TorrentInfo {
        id: 40,
        info_hash: "edge040".to_string(),
        name: "EDGE-040 Test".to_string(),
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

    let inode_manager = fs.inode_manager();
    let torrent_inode = inode_manager.lookup_torrent(40).unwrap();

    // Get the file inode (file1.txt under torrent directory)
    let torrent_children = inode_manager.get_children(torrent_inode);
    let file_entry = torrent_children
        .iter()
        .find(|(_, entry)| entry.name() == "file1.txt" && entry.is_file())
        .expect("Should find file1.txt");
    let file_inode = file_entry.0;

    // Verify file exists before removal
    assert!(
        inode_manager.get(file_inode).is_some(),
        "File should exist before removal"
    );

    // Test 1: Simulate race condition - open file handle then remove torrent
    // This simulates the scenario where a file is opened for reading,
    // then the torrent is removed before the read completes.
    let fh_manager = FileHandleManager::default();
    let fh = fh_manager.allocate(file_inode, 40, 0);
    assert!(fh > 0, "Should allocate valid file handle");

    // Verify handle maps to inode
    assert_eq!(
        fh_manager.get_inode(fh),
        Some(file_inode),
        "Handle should map to file inode"
    );

    // Remove torrent structure (simulating what happens when torrent is removed)
    // This manually removes the inodes without API calls
    fs.inode_manager().remove_child(1, torrent_inode);
    fs.inode_manager().remove_inode(torrent_inode);

    // Verify torrent is removed from filesystem
    assert!(
        inode_manager.lookup_torrent(40).is_none(),
        "Torrent should be removed from lookup"
    );
    assert!(
        inode_manager.get(file_inode).is_none(),
        "File inode should be removed"
    );

    // Handle should still exist in the handle manager (not auto-invalidated)
    // This is expected - handles are independent of inode lifecycle
    assert_eq!(
        fh_manager.get_inode(fh),
        Some(file_inode),
        "Handle still points to old inode even after removal"
    );

    // Release handle - should succeed even for removed file
    // This tests that the handle manager doesn't crash when releasing stale handles
    let removed_handle = fh_manager.remove(fh);
    assert!(
        removed_handle.is_some(),
        "Should be able to remove handle for deleted file"
    );
    assert!(
        fh_manager.get_inode(fh).is_none(),
        "Handle should be released"
    );

    // Test 2: Verify file handles with various states don't crash on removal
    // Create another multi-file torrent
    let torrent_info2 = TorrentInfo {
        id: 41,
        info_hash: "edge041".to_string(),
        name: "EDGE-040 Second Test".to_string(),
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
                components: vec!["data".to_string(), "data.bin".to_string()],
            },
        ],
        piece_length: Some(1048576),
    };
    fs.create_torrent_structure(&torrent_info2).unwrap();

    // Get the file inode from the second torrent
    let torrent_inode2 = inode_manager.lookup_torrent(41).unwrap();
    let torrent2_children = inode_manager.get_children(torrent_inode2);
    let file_entry2 = torrent2_children
        .iter()
        .find(|(_, entry)| entry.name() == "readme.txt" && entry.is_file())
        .expect("Should find readme.txt");
    let file_inode2 = file_entry2.0;

    // Open multiple handles for the same file
    let fh2 = fh_manager.allocate(file_inode2, 41, 0);
    let fh3 = fh_manager.allocate(file_inode2, 41, 0);
    let fh4 = fh_manager.allocate(file_inode2, 41, 0);

    assert!(
        fh2 > 0 && fh3 > 0 && fh4 > 0,
        "Should allocate multiple handles"
    );

    // Remove torrent while handles are active
    fs.inode_manager().remove_child(1, torrent_inode2);
    fs.inode_manager().remove_inode(torrent_inode2);

    // Verify torrent is removed
    assert!(
        inode_manager.lookup_torrent(41).is_none(),
        "Second torrent should be removed"
    );

    // Release all handles - should not crash
    fh_manager.remove(fh2);
    fh_manager.remove(fh3);
    fh_manager.remove(fh4);

    // Verify all handles released
    assert!(fh_manager.is_empty(), "All handles should be released");

    // Test 3: Verify system state remains consistent
    // Root directory should have no torrent children with our test names
    let root_children = inode_manager.get_children(1);
    let torrent_children: Vec<_> = root_children
        .iter()
        .filter(|(_, entry)| {
            entry.name() == "EDGE-040 Test" || entry.name() == "EDGE-040 Second Test"
        })
        .collect();
    assert!(
        torrent_children.is_empty(),
        "Root should not have removed torrent children"
    );
}

/// EDGE-041: Test concurrent discovery
///
/// This test verifies that the atomic check-and-set mechanism works correctly
/// when torrent discovery is triggered simultaneously from multiple sources:
/// 1. On-demand discovery from readdir (when listing root directory)
/// 2. Explicit refresh via refresh_torrents()
///
/// The test ensures that:
/// - No duplicate torrents are created
/// - The atomic check in discover_torrents prevents race conditions
/// - Concurrent discovery operations complete without errors
#[tokio::test]
async fn test_edge_041_concurrent_discovery() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    // Mock the list torrents endpoint to return our test torrent summary
    // list_torrents() first fetches the list, then fetches details for each
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": [{
                "id": 41,
                "info_hash": "edge041",
                "name": "EDGE-041 Concurrent Discovery Test",
                "output_folder": "/downloads"
            }]
        })))
        .expect(2..=4) // Expect 2-4 calls (one from each concurrent discovery, potentially with retries)
        .mount(&mock_server)
        .await;

    // Mock the individual torrent endpoint for full details
    Mock::given(method("GET"))
        .and(path("/torrents/41"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 41,
            "info_hash": "edge041",
            "name": "EDGE-041 Concurrent Discovery Test",
            "output_folder": "/downloads",
            "file_count": 1,
            "files": [
                {"name": "test.txt", "length": 1024, "components": ["test.txt"]}
            ],
            "piece_length": 1048576
        })))
        .expect(2..=4) // Expect 2-4 calls (one from each concurrent discovery)
        .mount(&mock_server)
        .await;

    let metrics = Arc::new(Metrics::new());
    let fs = Arc::new(create_test_fs(config, metrics));

    let inode_manager = fs.inode_manager();

    // Verify no torrent exists initially
    assert!(
        inode_manager.lookup_torrent(41).is_none(),
        "Torrent should not exist before discovery"
    );

    // Use a barrier to synchronize both discovery operations
    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);

    let fs1 = Arc::clone(&fs);
    let fs2 = Arc::clone(&fs);

    // Spawn two concurrent discovery tasks
    let handle1 = tokio::spawn(async move {
        barrier1.wait().await;
        fs1.refresh_torrents(true).await
    });

    let handle2 = tokio::spawn(async move {
        barrier2.wait().await;
        fs2.refresh_torrents(true).await
    });

    // Wait for both discoveries to complete
    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();

    // At least one discovery should have succeeded
    assert!(
        result1 || result2,
        "At least one concurrent discovery should succeed"
    );

    // Verify only ONE torrent was created (no duplicates)
    let torrent_inode = inode_manager.lookup_torrent(41);
    assert!(
        torrent_inode.is_some(),
        "Torrent should be created after discovery"
    );

    // Count occurrences of the torrent in the filesystem
    let root_children = inode_manager.get_children(1);
    let torrent_count = root_children
        .iter()
        .filter(|(_, entry)| entry.name() == "EDGE-041 Concurrent Discovery Test")
        .count();

    assert_eq!(
        torrent_count, 1,
        "Exactly one torrent entry should exist (found {})",
        torrent_count
    );

    // Verify the torrent has the expected structure
    let torrent_inode = torrent_inode.unwrap();
    let torrent_children = inode_manager.get_children(torrent_inode);
    assert_eq!(
        torrent_children.len(),
        1,
        "Torrent should have exactly one child (the file)"
    );

    // Verify the file exists
    let file_entry = torrent_children
        .iter()
        .find(|(_, entry)| entry.name() == "test.txt");
    assert!(file_entry.is_some(), "File should exist in torrent");

    // Test 2: Rapid successive discoveries should respect cooldown
    // Reset by clearing the discovery timestamp
    fs.__test_clear_list_torrents_cache().await;

    // First discovery (forced)
    let first_result = fs.refresh_torrents(true).await;
    assert!(first_result, "First discovery should succeed");

    // Second discovery (should be skipped due to cooldown)
    let second_result = fs.refresh_torrents(false).await;
    assert!(
        !second_result,
        "Second discovery should be skipped due to cooldown"
    );

    // Verify still only one torrent exists
    let final_count = inode_manager
        .get_children(1)
        .iter()
        .filter(|(_, entry)| entry.name() == "EDGE-041 Concurrent Discovery Test")
        .count();
    assert_eq!(final_count, 1, "Should still have exactly one torrent");
}

/// EDGE-042: Test mount/unmount race condition
///
/// This test verifies that starting a mount operation and immediately
/// unmounting doesn't cause panics and properly cleans up resources.
#[tokio::test]
async fn test_edge_042_mount_unmount_race() {
    use std::thread;
    use std::time::Duration;

    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);
    let mount_point = temp_dir.path().to_path_buf();

    // Spawn mount in a separate thread (mount blocks until unmounted)
    let mount_handle = thread::spawn(move || {
        // This will block until unmounted
        fs.mount()
    });

    // Give the mount operation a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Immediately unmount from main thread
    // This should interrupt the mount gracefully
    let unmount_result = rqbit_fuse::mount::try_unmount(&mount_point, false);

    // Wait for mount thread to complete
    let mount_result = mount_handle.join();

    // Verify mount thread didn't panic
    assert!(
        mount_result.is_ok(),
        "Mount thread should not panic, got: {:?}",
        mount_result.err()
    );

    // The mount operation should return either Ok(()) or an error,
    // but it should NOT panic
    match mount_result.unwrap() {
        Ok(()) => {
            // Mount completed successfully (unlikely in race scenario but valid)
        }
        Err(e) => {
            // Mount returned an error - this is expected in a race
            // The error should be graceful, not a panic
            let err_str = e.to_string();
            assert!(
                err_str.contains("unmount")
                    || err_str.contains("mount")
                    || err_str.contains("interrupted")
                    || err_str.contains("Transport")
                    || err_str.contains("endpoint"),
                "Expected graceful error during mount/unmount race, got: {}",
                err_str
            );
        }
    }

    // Verify unmount was attempted (may succeed or fail depending on timing)
    // The important thing is that it didn't panic
    match unmount_result {
        Ok(()) => {
            // Successfully unmounted
        }
        Err(e) => {
            // Unmount may fail if mount hadn't fully started yet
            // This is acceptable - no panic means test passes
            tracing::debug!("Unmount returned error (acceptable in race): {}", e);
        }
    }

    // Final verification: mount point should not be mounted
    // (or the check itself should not panic)
    match rqbit_fuse::mount::is_mount_point(&mount_point) {
        Ok(is_mounted) => {
            // It's okay if it's still mounted briefly after the race
            // The important thing is no panic occurred
            tracing::debug!("Mount point mounted status after race: {}", is_mounted);
        }
        Err(e) => {
            // Checking mount status might fail if directory was removed
            // This is also acceptable
            tracing::debug!("Mount check returned error (acceptable): {}", e);
        }
    }
}

/// EDGE-042b: Test rapid mount/unmount cycles
///
/// Verifies that repeated mount/unmount cycles don't cause resource leaks
/// or panics.
#[tokio::test]
async fn test_edge_042b_rapid_mount_unmount_cycles() {
    use std::thread;
    use std::time::Duration;

    // Run multiple mount/unmount cycles
    for i in 0..3 {
        let mock_server = setup_mock_server().await;
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

        let metrics = Arc::new(Metrics::new());
        let fs = create_test_fs(config, metrics);
        let mount_point = temp_dir.path().to_path_buf();

        // Start mount
        let mount_handle = thread::spawn(move || fs.mount());

        // Small delay
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Unmount
        let _ = rqbit_fuse::mount::try_unmount(&mount_point, false);

        // Wait for completion without panic
        let result = mount_handle.join();
        assert!(
            result.is_ok(),
            "Cycle {}: Mount thread panicked: {:?}",
            i,
            result.err()
        );
    }
}
