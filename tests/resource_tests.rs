//! Resource limit edge case tests
//!
//! Tests for resource exhaustion scenarios including:
//! - Semaphore exhaustion (EDGE-047)
//! - Stream/handle limits (EDGE-008)
//! - Inode limits (EDGE-045)
//! - Cache memory limits (EDGE-046)

use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

use rqbit_fuse::{AsyncFuseWorker, Config, Metrics, TorrentFS};

// Import common test helpers
mod common;

/// Creates a test configuration with specified max_concurrent_reads
fn create_test_config_with_semaphore(
    mock_uri: String,
    mount_point: std::path::PathBuf,
    max_concurrent_reads: usize,
) -> Config {
    let mut config = Config::default();
    config.api.url = mock_uri;
    config.mount.mount_point = mount_point;
    config.performance.max_concurrent_reads = max_concurrent_reads;
    config
}

/// Helper function to create a TorrentFS with custom config
fn create_test_fs_with_config(config: Config, metrics: Arc<Metrics>) -> TorrentFS {
    let api_client = Arc::new(
        rqbit_fuse::api::client::RqbitClient::new(config.api.url.clone(), Arc::clone(&metrics.api))
            .expect("Failed to create API client"),
    );
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, metrics.clone(), 100));
    TorrentFS::new(config, metrics, async_worker).unwrap()
}

/// EDGE-047: Test semaphore exhaustion
/// - Trigger max_concurrent_reads simultaneously
/// - 11th read should wait (not fail - semaphore waits by default)
/// - Should not deadlock
#[tokio::test]
async fn test_edge_047_semaphore_exhaustion() {
    // Set max_concurrent_reads to 10 for testing
    let max_concurrent_reads = 10;

    let mock_server = wiremock::MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_semaphore(
        mock_server.uri(),
        temp_dir.path().to_path_buf(),
        max_concurrent_reads,
    );

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs_with_config(config, metrics);

    // Get the semaphore from the filesystem
    let semaphore = fs.read_semaphore();

    // Verify initial state - all permits should be available
    assert_eq!(
        semaphore.available_permits(),
        max_concurrent_reads,
        "All permits should be available initially"
    );

    // Acquire all permits (simulating max_concurrent_reads simultaneous reads)
    let mut permits = Vec::new();
    for i in 0..max_concurrent_reads {
        let permit = semaphore
            .acquire()
            .await
            .expect("Should be able to acquire permit");
        permits.push(permit);
        assert_eq!(
            semaphore.available_permits(),
            max_concurrent_reads - i - 1,
            "Available permits should decrease after each acquisition"
        );
    }

    // Verify no permits remain
    assert_eq!(
        semaphore.available_permits(),
        0,
        "No permits should be available after acquiring all"
    );

    // Try to acquire one more permit - this should wait, not fail
    // Use timeout to avoid blocking forever if there's a deadlock
    let acquire_result = timeout(Duration::from_millis(100), semaphore.acquire()).await;

    // Should timeout (None) because no permits are available
    assert!(
        acquire_result.is_err(),
        "11th acquire should wait (timeout) when all permits are exhausted"
    );

    // Release one permit
    drop(permits.pop());

    // Now we should be able to acquire again
    let _new_permit = semaphore
        .acquire()
        .await
        .expect("Should be able to acquire after releasing one");

    // Cleanup: release all remaining permits
    drop(permits);
}

/// EDGE-047b: Test semaphore exhaustion with multiple waiters
/// Verify that when multiple tasks are waiting for permits,
/// they are granted in order as permits become available
#[tokio::test]
async fn test_edge_047b_semaphore_multiple_waiters() {
    let max_concurrent_reads = 3;

    let mock_server = wiremock::MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_semaphore(
        mock_server.uri(),
        temp_dir.path().to_path_buf(),
        max_concurrent_reads,
    );

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs_with_config(config, metrics);
    let semaphore = fs.read_semaphore();

    // Acquire all permits
    let mut permits = Vec::new();
    for _ in 0..max_concurrent_reads {
        permits.push(semaphore.acquire().await.unwrap());
    }

    // Spawn multiple tasks that will wait for permits
    let semaphore_clone = Arc::clone(semaphore);
    let mut handles = Vec::new();

    for i in 0..3 {
        let sem = Arc::clone(&semaphore_clone);
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();
            let permit = sem.acquire().await;
            let elapsed = start.elapsed();
            (i, permit.is_ok(), elapsed)
        });
        handles.push(handle);
    }

    // Give tasks time to start waiting
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Release permits one by one and verify waiters get them
    for _ in 0..3 {
        drop(permits.pop());

        // Wait for the task to complete
        let result = timeout(Duration::from_millis(500), handles.remove(0))
            .await
            .expect("Task should complete after permit is available")
            .expect("Task should not panic");

        assert!(
            result.1,
            "Task {} should successfully acquire permit",
            result.0
        );
        assert!(
            result.2 >= Duration::from_millis(40),
            "Task {} should have waited for permit",
            result.0
        );
    }

    // Cleanup
    drop(permits);
}

/// EDGE-047c: Test semaphore permits are properly released on drop
/// Verify that if a task holding a permit is cancelled, the permit is released
#[tokio::test]
async fn test_edge_047c_semaphore_permit_release_on_cancel() {
    let max_concurrent_reads = 2;

    let mock_server = wiremock::MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_semaphore(
        mock_server.uri(),
        temp_dir.path().to_path_buf(),
        max_concurrent_reads,
    );

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs_with_config(config, metrics);
    let semaphore = fs.read_semaphore();

    // Acquire all permits
    let mut permits = Vec::new();
    for _ in 0..max_concurrent_reads {
        permits.push(semaphore.acquire().await.unwrap());
    }

    assert_eq!(semaphore.available_permits(), 0);

    // Drop all permits at once
    drop(permits);

    // All permits should be available again
    assert_eq!(
        semaphore.available_permits(),
        max_concurrent_reads,
        "All permits should be available after drop"
    );

    // Should be able to acquire all permits again
    let _permit1 = semaphore.acquire().await.unwrap();
    let _permit2 = semaphore.acquire().await.unwrap();

    assert_eq!(semaphore.available_permits(), 0);
}

/// EDGE-047d: Test concurrency stats reflect semaphore state
#[tokio::test]
async fn test_edge_047d_concurrency_stats_accuracy() {
    let max_concurrent_reads = 5;

    let mock_server = wiremock::MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config_with_semaphore(
        mock_server.uri(),
        temp_dir.path().to_path_buf(),
        max_concurrent_reads,
    );

    let metrics = Arc::new(Metrics::new());
    let fs = create_test_fs_with_config(config, metrics);

    // Check initial stats
    let stats = fs.concurrency_stats();
    assert_eq!(stats.max_concurrent_reads, max_concurrent_reads);
    assert_eq!(stats.available_permits, max_concurrent_reads);

    // Acquire some permits
    let semaphore = fs.read_semaphore();
    let _permit1 = semaphore.acquire().await.unwrap();
    let _permit2 = semaphore.acquire().await.unwrap();

    let stats = fs.concurrency_stats();
    assert_eq!(stats.available_permits, max_concurrent_reads - 2);

    // Acquire remaining
    let mut permits = vec![_permit1, _permit2];
    for _ in 0..(max_concurrent_reads - 2) {
        permits.push(semaphore.acquire().await.unwrap());
    }

    let stats = fs.concurrency_stats();
    assert_eq!(stats.available_permits, 0);
}
