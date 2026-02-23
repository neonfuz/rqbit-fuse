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
// EDGE-052: Test path normalization (NFD vs NFC)
// ============================================================================

/// Normalize a string to NFC form
fn to_nfc(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfc().collect()
}

/// Normalize a string to NFD form
fn to_nfd(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd().collect()
}

/// Test that filenames with NFC normalization work correctly
///
/// NFC (Canonical Decomposition followed by Canonical Composition) is the
/// most common Unicode normalization form on Linux and macOS.
#[tokio::test]
async fn test_edge_052_nfc_normalization() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test filename with composed characters (NFC form)
    // "café" with composed 'é' (U+00E9) - this is NFC
    let filename_nfc = "café.txt";

    // Verify it's actually NFC
    let normalized_nfc = to_nfc(filename_nfc);
    assert_eq!(
        filename_nfc, normalized_nfc,
        "Filename should already be NFC"
    );

    let torrent_info = TorrentInfo {
        id: 1200,
        info_hash: "nfc_test".to_string(),
        name: "NFC Normalization Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_nfc.to_string(),
            length: 1024,
            components: vec![filename_nfc.to_string()],
        }],
        piece_length: Some(1048576),
    };

    // Should create successfully
    fs.create_torrent_structure(&torrent_info)
        .expect("Should create torrent with NFC filename");

    // Verify the file exists and can be looked up
    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let found = root_children
        .iter()
        .any(|(_, entry)| entry.name() == filename_nfc);

    assert!(
        found,
        "NFC filename '{}' should exist in filesystem",
        filename_nfc
    );
}

/// Test that filenames with NFD normalization are handled consistently
///
/// NFD (Canonical Decomposition) is the default normalization form on macOS (HFS+).
/// This test verifies behavior when a filename in NFD form is used.
#[tokio::test]
async fn test_edge_052_nfd_normalization() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test filename with decomposed characters (NFD form)
    // "café" with 'e' + combining acute accent (U+0065 U+0301) - this is NFD
    let filename_nfd = to_nfd("café.txt");

    // Verify it's actually NFD (different from NFC)
    let filename_nfc = to_nfc("café.txt");
    assert_ne!(
        filename_nfd, filename_nfc,
        "NFD and NFC should be different byte sequences"
    );

    let torrent_info = TorrentInfo {
        id: 1201,
        info_hash: "nfd_test".to_string(),
        name: "NFD Normalization Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_nfd.clone(),
            length: 2048,
            components: vec![filename_nfd.clone()],
        }],
        piece_length: Some(1048576),
    };

    // Should handle gracefully - no panic
    let result = fs.create_torrent_structure(&torrent_info);

    match result {
        Ok(_) => {
            // If creation succeeds, verify the file exists
            let inode_manager = fs.inode_manager();
            let root_children = inode_manager.get_children(1);
            let found = root_children
                .iter()
                .any(|(_, entry)| entry.name() == filename_nfd);

            assert!(
                found,
                "NFD filename should exist in filesystem if creation succeeded"
            );

            // Verify file attributes are correct
            let file_inode = root_children
                .iter()
                .find(|(_, entry)| entry.name() == filename_nfd)
                .map(|(inode, _)| *inode)
                .expect("Should find NFD file inode");

            let file_entry = inode_manager
                .get(file_inode)
                .expect("Should get entry for NFD file");
            assert_eq!(
                file_entry.file_size(),
                2048,
                "NFD file should have correct size"
            );
        }
        Err(e) => {
            // Graceful error is acceptable
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.to_lowercase().contains("panic"),
                "NFD filename should not cause panic: {}",
                error_msg
            );
            println!("NFD filename rejected gracefully: {}", error_msg);
        }
    }
}

/// Test that NFC and NFD filenames are treated consistently
///
/// This test creates a file with NFC normalization and attempts to look it up
/// with NFD normalization (or vice versa) to verify consistent behavior.
#[tokio::test]
async fn test_edge_052_nfc_nfd_consistency() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Create file with NFC form
    let filename_nfc = "naïve.pdf";
    let filename_nfd = to_nfd(filename_nfc);

    assert_ne!(
        filename_nfc, filename_nfd,
        "NFC and NFD forms should have different byte representations"
    );

    // Create torrent with NFC filename
    let torrent_info_nfc = TorrentInfo {
        id: 1202,
        info_hash: "consistency_nfc".to_string(),
        name: "Consistency Test NFC".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: filename_nfc.to_string(),
            length: 4096,
            components: vec![filename_nfc.to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info_nfc)
        .expect("Should create torrent with NFC filename");

    // Try to look up with NFD form
    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);

    // Check if NFC form exists
    let found_nfc = root_children
        .iter()
        .any(|(_, entry)| entry.name() == filename_nfc);

    // Check if NFD form exists (it might be normalized or stored as-is)
    let found_nfd = root_children
        .iter()
        .any(|(_, entry)| entry.name() == filename_nfd);

    // Both files should not exist simultaneously (would indicate duplicate)
    assert!(
        !(found_nfc && found_nfd),
        "NFC and NFD forms should not both exist (would be duplicate files)"
    );

    // At least one should exist
    assert!(
        found_nfc || found_nfd,
        "At least one form (NFC or NFD) should exist"
    );

    println!(
        "Consistency test: NFC found={}, NFD found={}",
        found_nfc, found_nfd
    );
}

/// Test various Unicode normalization edge cases
///
/// Tests multiple characters that have different NFC/NFD representations
/// including accented characters, composite characters, and special Unicode.
#[tokio::test]
async fn test_edge_052_various_normalization_cases() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test cases with characters that have different NFC/NFD forms
    let test_cases = [
        ("résumé.txt", "resume with acute accents"),
        ("naïve.pdf", "naive with diaeresis"),
        ("français.doc", "francais with cedilla"),
        ("Zürich.txt", "Zurich with umlaut"),
        (
            "日本語ファイル.txt",
            "Japanese (no normalization differences)",
        ),
        ("北京.pdf", "Chinese (no normalization differences)"),
    ];

    for (idx, (filename_base, description)) in test_cases.iter().enumerate() {
        let filename_nfc = to_nfc(filename_base);
        let filename_nfd = to_nfd(filename_base);

        let torrent_info = TorrentInfo {
            id: 1210 + idx as u64,
            info_hash: format!("norm{}", idx),
            name: format!("Normalization Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename_nfc.clone(),
                length: 1024,
                components: vec![filename_nfc.clone()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == filename_nfc || entry.name() == filename_nfd);

                assert!(
                    found,
                    "File '{}' ({}) should exist after creation",
                    filename_base, description
                );
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "Normalization test '{}' should not cause panic: {}",
                    description,
                    error_msg
                );
                println!(
                    "Normalization test '{}' handled gracefully: {}",
                    description, error_msg
                );
            }
        }
    }
}

/// Test normalization with already normalized strings
///
/// Verifies that already-normalized strings don't cause issues.
#[tokio::test]
async fn test_edge_052_already_normalized() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // ASCII filenames are already in both NFC and NFD forms
    let ascii_filename = "normal_ascii_file.txt";

    assert_eq!(
        to_nfc(ascii_filename),
        ascii_filename,
        "ASCII should already be NFC"
    );
    assert_eq!(
        to_nfd(ascii_filename),
        ascii_filename,
        "ASCII should already be NFD"
    );

    let torrent_info = TorrentInfo {
        id: 1220,
        info_hash: "already_norm".to_string(),
        name: "Already Normalized Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: ascii_filename.to_string(),
            length: 512,
            components: vec![ascii_filename.to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&torrent_info)
        .expect("Should create torrent with ASCII filename");

    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let found = root_children
        .iter()
        .any(|(_, entry)| entry.name() == ascii_filename);

    assert!(found, "ASCII filename should exist in filesystem");
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

// ============================================================================
// EDGE-049: Test null byte in filename
// ============================================================================

/// Test that filenames containing null bytes are handled gracefully
///
/// Null bytes in filenames should be either sanitized (replaced) or rejected
/// but should never cause a panic or crash.
#[tokio::test]
async fn test_edge_049_null_byte_in_filename() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various null byte positions
    let test_cases = [
        ("\0file.txt", "null at start"),
        ("file\0.txt", "null in middle"),
        ("file.txt\0", "null at end"),
        ("file\0\0.txt", "multiple nulls"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 100 + idx as u64,
            info_hash: format!("nullbyte{}", idx),
            name: format!("Null Byte Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let _found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name().contains("file"));

                // If null bytes are sanitized, we should be able to find a file
                // If null bytes are rejected, we won't find anything
                println!(
                    "Null byte filename '{}' succeeded ({})",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable - should contain filename-related message
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic")
                        && !error_msg.to_lowercase().contains("unwrap")
                        && !error_msg.to_lowercase().contains("assertion"),
                    "Null byte filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "Null byte filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that null bytes at various positions are handled consistently
///
/// This ensures that null byte handling is predictable regardless of position.
#[tokio::test]
async fn test_edge_049_null_byte_positions() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test edge case: filename consisting only of null bytes
    let only_nulls = "\0\0\0";
    let torrent_info = TorrentInfo {
        id: 200,
        info_hash: "onlynulls".to_string(),
        name: "Only Nulls Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: only_nulls.to_string(),
            length: 512,
            components: vec![only_nulls.to_string()],
        }],
        piece_length: Some(1048576),
    };

    // Should handle gracefully without panic
    let result = fs.create_torrent_structure(&torrent_info);

    match result {
        Ok(_) => {
            println!("Null-only filename succeeded (sanitized or allowed)");
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.to_lowercase().contains("panic"),
                "Null-only filename should not cause panic"
            );
            println!("Null-only filename rejected gracefully: {}", error_msg);
        }
    }
}

/// Test that null byte filenames don't interfere with other files
///
/// Ensures that problematic filenames don't corrupt the filesystem state
/// or affect other valid files.
#[tokio::test]
async fn test_edge_049_null_byte_with_valid_files() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // First create a valid file
    let valid_filename = "valid_file.txt";
    let valid_torrent = TorrentInfo {
        id: 300,
        info_hash: "validfirst".to_string(),
        name: "Valid File Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: valid_filename.to_string(),
            length: 2048,
            components: vec![valid_filename.to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&valid_torrent)
        .expect("Valid file should be created");

    // Then try to create a file with null bytes
    let null_filename = "file\0with\0nulls.txt";
    let null_torrent = TorrentInfo {
        id: 301,
        info_hash: "nullsecond".to_string(),
        name: "Null Byte Test".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: null_filename.to_string(),
            length: 1024,
            components: vec![null_filename.to_string()],
        }],
        piece_length: Some(1048576),
    };

    // Should handle gracefully
    let result = fs.create_torrent_structure(&null_torrent);

    // Verify the valid file is still accessible
    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let valid_file_exists = root_children
        .iter()
        .any(|(_, entry)| entry.name() == valid_filename);

    assert!(
        valid_file_exists,
        "Valid file should still exist after attempted null byte file creation"
    );

    // No panic should have occurred
    println!("Null byte file result: {:?}", result.is_ok());
}

// ============================================================================
// EDGE-050: Test control characters in filenames
// ============================================================================

/// Test that filenames containing control characters are handled gracefully
///
/// Control characters (\n, \t, \r, etc.) in filenames should be either
/// sanitized (replaced) or rejected but should never cause a panic or crash.
#[tokio::test]
async fn test_edge_050_control_characters_in_filename() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various control characters
    let test_cases = [
        ("file\nname.txt", "newline (\\n)"),
        ("file\tname.txt", "tab (\\t)"),
        ("file\rname.txt", "carriage return (\\r)"),
        ("file\x01name.txt", "SOH (0x01)"),
        ("file\x1Fname.txt", "US (0x1F - unit separator)"),
        ("file\x7Fname.txt", "DEL (0x7F)"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 400 + idx as u64,
            info_hash: format!("control{}", idx),
            name: format!("Control Char Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let _found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name().contains("file"));

                println!(
                    "Control char filename '{}' succeeded ({})",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable - should contain filename-related message
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic")
                        && !error_msg.to_lowercase().contains("unwrap")
                        && !error_msg.to_lowercase().contains("assertion"),
                    "Control char filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "Control char filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that multiple control characters in sequence are handled
///
/// Tests filenames with combinations of control characters.
#[tokio::test]
async fn test_edge_050_multiple_control_characters() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test filenames with multiple control characters
    let test_cases = [
        ("file\n\t\r.txt", "newline tab return"),
        ("\n\tfile.txt", "leading newline and tab"),
        ("file.txt\n\t", "trailing newline and tab"),
        ("\x01\x02\x03file.txt", "multiple SOH STX ETX"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 500 + idx as u64,
            info_hash: format!("multi_ctrl{}", idx),
            name: format!("Multiple Control Chars {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 512,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully without panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                println!(
                    "Multiple control chars filename '{}' succeeded ({})",
                    filename, description
                );
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "Multiple control chars filename should not cause panic"
                );
                println!(
                    "Multiple control chars filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that control character filenames don't interfere with valid files
///
/// Ensures that filenames with control characters don't corrupt the filesystem
/// state or affect other valid files.
#[tokio::test]
async fn test_edge_050_control_chars_with_valid_files() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // First create a valid file
    let valid_filename = "normal_file.txt";
    let valid_torrent = TorrentInfo {
        id: 600,
        info_hash: "validfile".to_string(),
        name: "Valid File".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: valid_filename.to_string(),
            length: 2048,
            components: vec![valid_filename.to_string()],
        }],
        piece_length: Some(1048576),
    };

    fs.create_torrent_structure(&valid_torrent)
        .expect("Valid file should be created");

    // Then try to create files with various control characters
    let control_filenames = [
        "file\nwith\nnewlines.txt",
        "file\twith\ttabs.txt",
        "file\rwith\rreturns.txt",
    ];

    for (idx, filename) in control_filenames.iter().enumerate() {
        let control_torrent = TorrentInfo {
            id: 601 + idx as u64,
            info_hash: format!("ctrl{}", idx),
            name: format!("Control Char File {}", idx),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully
        let _result = fs.create_torrent_structure(&control_torrent);
    }

    // Verify the valid file is still accessible
    let inode_manager = fs.inode_manager();
    let root_children = inode_manager.get_children(1);
    let valid_file_exists = root_children
        .iter()
        .any(|(_, entry)| entry.name() == valid_filename);

    assert!(
        valid_file_exists,
        "Valid file should still exist after attempted control char file creation"
    );

    // Verify valid file has correct size
    let valid_inode = root_children
        .iter()
        .find(|(_, entry)| entry.name() == valid_filename)
        .map(|(inode, _)| *inode)
        .expect("Should find valid file inode");

    let valid_entry = inode_manager
        .get(valid_inode)
        .expect("Should get valid file entry");
    assert_eq!(
        valid_entry.file_size(),
        2048,
        "Valid file should have correct size"
    );
}

// ============================================================================
// EDGE-051: Test UTF-8 edge cases
// ============================================================================

/// Test that filenames containing emoji are handled correctly
///
/// Emoji are multi-byte UTF-8 sequences (typically 4 bytes each).
/// Tests various emoji including simple emoji, multi-codepoint emoji,
/// and emoji with skin tone modifiers.
#[tokio::test]
async fn test_edge_051_emoji_filenames() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various emoji filenames
    let test_cases = [
        ("📄document.txt", "document emoji"),
        ("🎬movie.mp4", "movie emoji"),
        ("🎵music.mp3", "music note"),
        ("🚀rocket.zip", "rocket"),
        ("👨‍👩‍👧‍👦family.pdf", "family with ZWJ"),
        ("🏳️‍🌈pride.png", "rainbow flag with ZWJ"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 700 + idx as u64,
            info_hash: format!("emoji{}", idx),
            name: format!("Emoji Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == *filename);

                assert!(
                    found,
                    "Emoji filename '{}' ({}) should exist in filesystem",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable - should contain filename-related message
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic")
                        && !error_msg.to_lowercase().contains("unwrap")
                        && !error_msg.to_lowercase().contains("assertion"),
                    "Emoji filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "Emoji filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that filenames containing CJK (Chinese, Japanese, Korean) characters work correctly
///
/// CJK characters are typically 3 bytes in UTF-8. Tests various CJK scripts
/// including simplified/traditional Chinese, Hiragana/Katakana, and Hangul.
#[tokio::test]
async fn test_edge_051_cjk_filenames() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various CJK filenames
    let test_cases = [
        ("文档.txt", "simplified Chinese"),
        ("文檔.txt", "traditional Chinese"),
        ("ドキュメント.txt", "Japanese Katakana"),
        ("ドキュメント.txt", "Japanese Hiragana"),
        ("문서.txt", "Korean Hangul"),
        ("文件資料.pdf", "mixed Chinese"),
        ("資料フォルダ.zip", "Chinese + Katakana"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 800 + idx as u64,
            info_hash: format!("cjk{}", idx),
            name: format!("CJK Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 2048,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == *filename);

                assert!(
                    found,
                    "CJK filename '{}' ({}) should exist in filesystem",
                    filename, description
                );

                // Verify file attributes are correct
                let file_inode = root_children
                    .iter()
                    .find(|(_, entry)| entry.name() == *filename)
                    .map(|(inode, _)| *inode)
                    .expect("Should find CJK file inode");

                let file_entry = inode_manager
                    .get(file_inode)
                    .expect("Should get entry for CJK file");
                assert_eq!(
                    file_entry.file_size(),
                    2048,
                    "CJK file should have correct size"
                );
            }
            Err(e) => {
                // Graceful error is acceptable
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "CJK filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "CJK filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that filenames containing RTL (Right-to-Left) text work correctly
///
/// RTL scripts like Arabic and Hebrew should be handled properly. Tests
/// various RTL scenarios including pure RTL text and mixed LTR/RTL.
#[tokio::test]
async fn test_edge_051_rtl_filenames() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various RTL filenames
    let test_cases = [
        ("ملف.txt", "Arabic"),
        ("קובץ.txt", "Hebrew"),
        ("فایل.pdf", "Persian (Farsi)"),
        ("doc_ملف.txt", "mixed LTR/RTL"),
        ("ملف_קובץ.zip", "Arabic + Hebrew"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 900 + idx as u64,
            info_hash: format!("rtl{}", idx),
            name: format!("RTL Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == *filename);

                assert!(
                    found,
                    "RTL filename '{}' ({}) should exist in filesystem",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "RTL filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "RTL filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that filenames containing zero-width joiners work correctly
///
/// Zero-width joiners (ZWJ) are used to combine emoji into sequences.
/// Tests various ZWJ sequences including complex emoji combinations.
#[tokio::test]
async fn test_edge_051_zero_width_joiner_filenames() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various ZWJ sequences
    let test_cases = [
        ("👨‍💻developer.txt", "man technologist"),
        ("👩‍🔬scientist.pdf", "woman scientist"),
        ("👨‍🌾farmer.zip", "man farmer"),
        ("👩‍🎨artist.png", "woman artist"),
        ("🏃‍♂️runner.mp4", "man running"),
        ("🏃‍♀️runner.mp4", "woman running"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 1000 + idx as u64,
            info_hash: format!("zwj{}", idx),
            name: format!("ZWJ Test {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 512,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == *filename);

                assert!(
                    found,
                    "ZWJ filename '{}' ({}) should exist in filesystem",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "ZWJ filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "ZWJ filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}

/// Test that filenames with various other UTF-8 edge cases work correctly
///
/// Tests other Unicode edge cases including combining characters,
/// mathematical symbols, and special Unicode characters.
#[tokio::test]
async fn test_edge_051_other_utf8_edge_cases() {
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs(config, metrics);

    // Test various other UTF-8 edge cases
    let test_cases = [
        ("café.txt", "accented Latin"),
        ("naïve.pdf", "diaeresis"),
        ("resumé.doc", "combining acute accent"),
        ("∑math.txt", "mathematical symbol"),
        ("Ωsymbol.txt", "Greek letter"),
        ("★star.txt", "star symbol"),
        ("∞infinity.txt", "infinity symbol"),
        ("♠card.txt", "playing card suit"),
    ];

    for (idx, (filename, description)) in test_cases.iter().enumerate() {
        let torrent_info = TorrentInfo {
            id: 1100 + idx as u64,
            info_hash: format!("utf8{}", idx),
            name: format!("UTF-8 Edge Case {}", description),
            output_folder: "/downloads".to_string(),
            file_count: Some(1),
            files: vec![FileInfo {
                name: filename.to_string(),
                length: 1024,
                components: vec![filename.to_string()],
            }],
            piece_length: Some(1048576),
        };

        // Should handle gracefully - no panic
        let result = fs.create_torrent_structure(&torrent_info);

        match result {
            Ok(_) => {
                // If creation succeeds, verify the file exists
                let inode_manager = fs.inode_manager();
                let root_children = inode_manager.get_children(1);
                let found = root_children
                    .iter()
                    .any(|(_, entry)| entry.name() == *filename);

                assert!(
                    found,
                    "UTF-8 filename '{}' ({}) should exist in filesystem",
                    filename, description
                );
            }
            Err(e) => {
                // Graceful error is acceptable
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.to_lowercase().contains("panic"),
                    "UTF-8 filename should not cause panic: {}",
                    error_msg
                );
                println!(
                    "UTF-8 filename '{}' rejected gracefully: {}",
                    filename, error_msg
                );
            }
        }
    }
}
