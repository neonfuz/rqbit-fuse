pub mod api;
pub mod cache;
pub mod config;
pub mod fs;
pub mod metrics;
pub mod sharded_counter;
pub mod types;

pub use cache::{Cache, CacheStats};
pub use config::{CliArgs, Config};
pub use fs::async_bridge::AsyncFuseWorker;
pub use fs::filesystem::TorrentFS;
pub use metrics::Metrics;
pub use sharded_counter::ShardedCounter;

use anyhow::{Context, Result};
use std::sync::Arc;

pub async fn run(config: Config) -> Result<()> {
    tracing::info!(operation = "startup", message = "torrent-fuse starting");
    tracing::debug!(config = ?config, "Configuration loaded");

    // Create metrics
    let metrics = Arc::new(Metrics::new());

    // Create API client for the async worker
    let api_client = Arc::new(api::client::RqbitClient::new(
        config.api.url.clone(),
        Arc::clone(&metrics.api),
    ));

    // Create async worker for FUSE callbacks
    // Channel capacity of 1000 allows for good concurrency without excessive memory use
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, Arc::clone(&metrics), 1000));

    // Create the filesystem with async worker
    let fs = TorrentFS::new(config, Arc::clone(&metrics), async_worker)
        .context("Failed to create torrent filesystem")?;

    // Discover existing torrents before mounting
    crate::fs::filesystem::discover_existing_torrents(&fs)
        .await
        .context("Failed to discover existing torrents")?;

    // Mount the filesystem (this blocks until unmounted)
    fs.mount().context("Failed to mount filesystem")?;

    // Log final metrics on shutdown
    metrics.log_full_summary();

    Ok(())
}
