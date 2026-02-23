//! WireMock server utilities for API testing
//!
//! Provides helper functions to set up mock rqbit servers with various
//! configurations for testing different scenarios.

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Standard mock server setup with basic health check endpoint
///
/// # Returns
/// A MockServer instance with a default /torrents endpoint returning empty list
///
/// # Example
/// ```rust
/// let mock_server = setup_mock_server().await;
/// // Use mock_server.uri() as the API URL
/// ```
pub async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Default health check response
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": []
        })))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Mock server with predefined torrent data
///
/// Sets up a mock server with:
/// - /torrents endpoint returning a list with one torrent
/// - /torrents/1 endpoint with full torrent details
///
/// # Returns
/// A MockServer instance with torrent data
///
/// # Example
/// ```rust
/// let mock_server = setup_mock_server_with_torrents().await;
/// ```
pub async fn setup_mock_server_with_torrents() -> MockServer {
    let mock_server = MockServer::start().await;

    // Torrent list endpoint
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": [
                {"id": 1, "info_hash": "abc123", "name": "Test Torrent"}
            ]
        })))
        .mount(&mock_server)
        .await;

    // Torrent details endpoint
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

    mock_server
}

/// Mock server with streaming data endpoint
///
/// Extends the torrent mock server with a streaming endpoint that
/// returns test data for file reads.
///
/// # Returns
/// A MockServer instance with streaming support
///
/// # Example
/// ```rust
/// let mock_server = setup_mock_server_with_data().await;
/// // File reads will return "Hello from FUSE!"
/// ```
pub async fn setup_mock_server_with_data() -> MockServer {
    let mock_server = setup_mock_server_with_torrents().await;

    // File streaming endpoint - returns test data
    Mock::given(method("GET"))
        .and(path("/torrents/1/stream/0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hello from FUSE!"))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Mock server with error responses
///
/// Creates a mock server that returns errors for testing error handling.
///
/// # Arguments
/// * `status_code` - The HTTP status code to return
/// * `message` - Optional error message
///
/// # Returns
/// A MockServer instance configured to return errors
///
/// # Example
/// ```rust
/// let mock_server = setup_mock_server_with_error(503, Some("Service Unavailable")).await;
/// ```
pub async fn setup_mock_server_with_error(status_code: u16, message: Option<&str>) -> MockServer {
    let mock_server = MockServer::start().await;

    let body = if let Some(msg) = message {
        serde_json::json!({"error": msg})
    } else {
        serde_json::json!({})
    };

    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(status_code).set_body_json(body))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Creates a test configuration pointing to the mock server
///
/// # Arguments
/// * `mock_uri` - The URI of the mock server
/// * `mount_point` - The filesystem mount point
///
/// # Returns
/// A Config instance configured for testing
///
/// # Example
/// ```rust
/// let temp_dir = TempDir::new().unwrap();
/// let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
/// ```
pub fn create_test_config(mock_uri: String, mount_point: std::path::PathBuf) -> rqbit_fuse::Config {
    let mut config = rqbit_fuse::Config::default();
    config.api.url = mock_uri;
    config.mount.mount_point = mount_point;
    config
}
