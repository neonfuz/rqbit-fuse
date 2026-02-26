//! A read-only FUSE filesystem that mounts BitTorrent torrents as virtual directories.
//!
//! Integrates with rqbit to expose torrent files through standard filesystem operations.

// Re-exports
//
// The primary types and functions intended for public use.

pub mod api;
pub mod config;
pub mod error;
pub mod fs;
pub mod metrics;
pub mod mount;
pub mod types;

/// Configuration module re-exports.
///
/// See [`config`] module for more details.
pub use config::{CliArgs, Config};

/// Async worker for handling FUSE callbacks.
///
/// This worker bridges synchronous FUSE callbacks with asynchronous operations
/// by spawning tasks on a Tokio runtime. Required for mounting the filesystem.
pub use fs::async_bridge::AsyncFuseWorker;

/// The main filesystem implementation.
///
/// This is the core type that handles all FUSE operations. Use [`TorrentFS::new()`]
/// to create an instance, then call [`TorrentFS::mount()`] to mount it.
pub use fs::filesystem::TorrentFS;

/// Metrics collection for monitoring performance.
///
/// Tracks API call latency, cache hits/misses, FUSE operation counts, and other
/// useful metrics for debugging and optimization.
pub use metrics::Metrics;

use crate::api::create_api_client;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;

/// Run the rqbit-fuse filesystem.
///
/// This is the main entry point for using rqbit-fuse as a library.
/// It sets up the metrics collection, API client, async worker, and filesystem,
/// then mounts the FUSE filesystem at the configured mount point.
///
/// # Arguments
///
/// * `config` - Configuration for the filesystem, API client, cache, and mount options
///
/// # Returns
///
/// Returns `Ok(())` on successful unmount, or an error if:
/// - API client creation fails
/// - Filesystem creation fails
/// - Mounting fails
/// - An error occurs during operation
///
/// # Example
///
/// ```ignore
/// use rqbit_fuse::{run, Config};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::from_args()?;
///     run(config).await?;
///     Ok(())
/// }
/// ```
///
/// # Note
///
/// This function blocks until the filesystem is unmounted. It handles SIGINT and
/// SIGTERM gracefully, cleaning up resources on shutdown.
pub async fn run(config: Config) -> Result<()> {
    tracing::info!(operation = "startup", message = "rqbit-fuse starting");
    tracing::debug!(config = ?config, "Configuration loaded");

    // Create metrics
    let metrics = Arc::new(Metrics::new());

    // Create API client for the async worker
    let api_client = Arc::new(
        create_api_client(&config.api, Some(Arc::clone(&metrics)))
            .context("API client creation failed")?,
    );

    // Create async worker for FUSE callbacks
    // Channel capacity of 1000 allows for good concurrency without excessive memory use
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, Arc::clone(&metrics), 1000));

    // Create the filesystem with async worker
    let fs = TorrentFS::new(config, Arc::clone(&metrics), async_worker)
        .context("filesystem creation failed")?;

    // Wrap in Arc for sharing between signal handler and main flow
    let fs_arc = Arc::new(fs);
    let mount_point = fs_arc.mount_point().to_path_buf();
    let mount_point_cleanup = mount_point.clone();

    // Clone for signal handler
    let fs_for_signal = Arc::clone(&fs_arc);
    let fs_for_mount = Arc::clone(&fs_arc);

    // Channel to signal shutdown from signal handler to mount task
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn signal handler task
    let signal_handler = tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT, initiating graceful shutdown...");
            }
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, initiating graceful shutdown...");
            }
        }

        // Signal the mount task to shut down
        let _ = shutdown_tx.send(());

        // Initiate graceful shutdown with timeout
        let shutdown_timeout = Duration::from_secs(10);
        let mount_point_force = mount_point.clone();

        let shutdown_result = tokio::time::timeout(shutdown_timeout, async {
            fs_for_signal.shutdown();

            // Try to unmount the filesystem gracefully
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("fusermount")
                    .arg("-u")
                    .arg(&mount_point)
                    .output()
            })
            .await
        })
        .await;

        match shutdown_result {
            Ok(Ok(Ok(_))) => {
                tracing::info!("Graceful shutdown completed successfully");
            }
            Ok(Ok(Err(e))) => {
                tracing::warn!("Unmount failed, trying force unmount: {}", e);
                // Try force unmount
                if let Err(force_err) = tokio::task::spawn_blocking(move || {
                    std::process::Command::new("fusermount")
                        .arg("-uz")
                        .arg(&mount_point_force)
                        .output()
                })
                .await
                {
                    tracing::error!("Force unmount also failed: {}", force_err);
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Shutdown task failed: {}", e);
            }
            Err(_) => {
                tracing::warn!(
                    "Shutdown timed out after {:?}, forcing exit",
                    shutdown_timeout
                );
            }
        }
    });

    // Discover existing torrents before mounting
    crate::fs::filesystem::discover_existing_torrents(&fs_arc)
        .await
        .context("torrent discovery failed")?;

    // Mount the filesystem in a blocking task so signals can be processed
    // This will return when the filesystem is unmounted (either via signal or externally)
    let mount_result =
        tokio::task::spawn_blocking(move || <TorrentFS as Clone>::clone(&fs_for_mount).mount())
            .await;

    // Race between mount completing and receiving shutdown signal
    // If we received a signal, shutdown_rx will be Some(Err(Canceled))
    tokio::select! {
        _ = shutdown_rx => {
            tracing::info!("Shutdown signal received, mount task is completing...");
        }
        _ = async {} => {}
    }

    if let Err(e) = mount_result {
        // If mount fails, we still need to clean up
        fs_arc.shutdown();
        metrics.log_summary();
        return Err(anyhow::anyhow!("Mount task failed: {}", e));
    }

    // Check if mount returned due to shutdown signal
    if mount_result.as_ref().is_ok_and(|r| r.is_err()) {
        tracing::info!("Mount returned due to unmount signal");
    }

    // The filesystem has been unmounted, clean up
    // Use timeout to ensure we don't hang on shutdown
    let cleanup_timeout = Duration::from_secs(5);
    let cleanup = async {
        fs_arc.shutdown();

        // Try to unmount if still mounted
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("fusermount")
                .arg("-u")
                .arg(mount_point_cleanup)
                .output()
        })
        .await
        .ok();
    };

    let _ = tokio::time::timeout(cleanup_timeout, cleanup).await;

    // Wait for signal handler to complete (it will timeout if already done)
    let _ = tokio::time::timeout(Duration::from_secs(5), signal_handler).await;

    // Log final metrics on shutdown
    metrics.log_summary();

    Ok(())
}
