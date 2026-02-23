//! Unicode and path edge case tests
//!
//! These tests verify handling of various Unicode scenarios, filename edge cases,
//! and path length boundaries. Tests cover:
//! - Maximum filename lengths (255 chars)
//! - Null bytes and control characters in filenames
//! - UTF-8 edge cases (emoji, CJK, RTL text)
//! - Path normalization (NFD vs NFC)
//! - Maximum path lengths (4096 chars)

use std::sync::Arc;
use tempfile::TempDir;

use rqbit_fuse::api::types::{FileInfo, TorrentInfo};
use rqbit_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

/// Sets up a mock rqbit server with standard responses
async fn setup_mock_server() -> wiremock::MockServer {
    let mock_server = wiremock::MockServer::start().await;

    // Default health check response
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/torrents"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"torrents": []})),
        )
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
        rqbit_fuse::api::client::RqbitClient::new(config.api.url.clone(), Arc::clone(&metrics.api))
            .expect("Failed to create API client"),
    );
    let async_worker = Arc::new(AsyncFuseWorker::new_for_test(api_client, metrics.clone()));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

// ============================================================================
// EDGE-048: Test maximum filename length
// ============================================================================

/// Test that filenames at the 255 character boundary work correctly
///
/// Linux ext4 filesystems support filenames up to 255 bytes (not characters).
/// This test verifies that filenames at this boundary are handled correctly.
#[tokio::test]
async fn test_edge_048_maximum_filename_length_255_chars() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a filename with exactly 255 characters
    let filename_255 = "a".repeat(255);
    assert_eq!(filename_255.len(), 255, "Filename should be 255 characters");

    // Create a torrent with the 255-char filename
    let torrent_info = TorrentInfo {
        id: 1,
        info_hash: "maxlen255".to_string(),
        name: "Max Length Test 255".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_255.clone(),
            length: 1024,
            components: vec![filename_255.clone()],
        }],
        piece_length: Some(1048576),
    };

    // Should succeed - 255 char filename is valid
    fs.create_torrent_structure(&torrent_info)
        .expect("Should create torrent with 255-char filename");

    // Verify the file was created and can be looked up
    let inode_manager = fs.inode_manager();

    // For single-file torrent, the file should be directly under root
    let root_children = inode_manager.get_children(1);
    let found_file = root_children
        .iter()
        .find(|(_, entry)| entry.name() == filename_255);

    assert!(
        found_file.is_some(),
        "255-char filename should exist in filesystem"
    );

    // Verify file attributes are correct
    let file_inode = found_file.unwrap().0;
    let file_entry = inode_manager
        .get(file_inode)
        .expect("Should get entry for 255-char file");
    assert_eq!(
        file_entry.file_size(),
        1024,
        "File size should be 1024 bytes"
    );
}

/// Test that filenames with 256 characters are handled gracefully
///
/// While Linux technically allows 255 bytes (which can be fewer than 255
/// characters for multi-byte UTF-8), this test verifies that the system
/// handles filenames at or beyond the limit gracefully without crashing.
#[tokio::test]
async fn test_edge_048_filename_length_256_chars_handling() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create a filename with 256 characters
    let filename_256 = "b".repeat(256);
    assert_eq!(filename_256.len(), 256, "Filename should be 256 characters");

    // Create a torrent with the 256-char filename
    let torrent_info = TorrentInfo {
        id: 2,
        info_hash: "maxlen256".to_string(),
        name: "Max Length Test 256".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_256.clone(),
            length: 2048,
            components: vec![filename_256.clone()],
        }],
        piece_length: Some(1048576),
    };

    // The system should handle this gracefully - either succeed (if underlying
    // filesystem supports it) or fail gracefully without panic
    let result = fs.create_torrent_structure(&torrent_info);

    // We accept either success or a graceful failure (no panic)
    match result {
        Ok(_) => {
            // If it succeeds, verify the file exists
            let inode_manager = fs.inode_manager();
            let root_children = inode_manager.get_children(1);
            let found_file = root_children
                .iter()
                .find(|(_, entry)| entry.name() == filename_256);
            assert!(
                found_file.is_some(),
                "256-char filename should exist if creation succeeded"
            );
        }
        Err(e) => {
            // If it fails, verify it's a graceful error (not a panic)
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("filename")
                    || error_msg.contains("name")
                    || error_msg.contains("too long")
                    || error_msg.contains("invalid"),
                "Error should indicate filename issue: {}",
                error_msg
            );
        }
    }
}

/// Test boundary around 255 characters with various lengths
#[tokio::test]
async fn test_edge_048_filename_length_boundary_variations() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various lengths around the 255 boundary
    let test_lengths = [253, 254, 255, 256, 257];

    for (idx, length) in test_lengths.iter().enumerate() {
        let filename = format!("file_{:03}_{}", length, "x".repeat(*length - 9));
        assert_eq!(
            filename.len(),
            *length,
            "Filename should be {} characters",
            length
        );

        let torrent_info = TorrentInfo {
            id: 10 + idx as u64,
            info_hash: format!("boundary{}", length),
            name: format!("Boundary Test {}", length),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.clone(),
                length: 512,
                components: vec![filename.clone()],
            }],
            piece_length: Some(1048576),
        };

        // Each should be handled without panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == filename);
                assert!(
                    found,
                    "File with {} chars should exist after successful creation",
                    length
                );
            }
            Err(_) => {
                // Graceful failure is acceptable for lengths >= 256
                if *length <= 255 {
                    panic!("Filename with {} chars should succeed", length);
                }
            }
        }
    }
}

/// Test maximum filename length with multi-byte UTF-8 characters
///
/// Linux uses byte limits, not character limits. So 255 bytes of UTF-8
/// could be fewer than 255 characters for multi-byte sequences.
#[tokio::test]
async fn test_edge_048_maximum_filename_with_multibyte_utf8() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Japanese character "あ" is 3 bytes in UTF-8
    // 85 characters * 3 bytes = 255 bytes (exact limit)
    let japanese_char = "あ";
    let filename_jp = japanese_char.repeat(85);
    assert_eq!(
        filename_jp.len(),
        255,
        "85 Japanese chars = 255 bytes in UTF-8"
    );

    let torrent_info = TorrentInfo {
        id: 3,
        info_hash: "utf8boundary".to_string(),
        name: "UTF-8 Boundary Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_jp.clone(),
            length: 1024,
            components: vec![filename_jp.clone()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info)
        .expect("Should create torrent with 255-byte UTF-8 filename");

    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let found = root_children
        .iter()
        .any(|(_, entry)| entry.name() == filename_jp);

    assert!(found, "255-byte UTF-8 filename should exist in filesystem");
}
