pub mod api;
pub mod cache;
pub mod config;
pub mod fs;
pub mod metrics;
pub mod types;

pub use cache::{Cache, CacheStats};
pub use config::{CliArgs, Config};
pub use fs::filesystem::TorrentFS;
pub use metrics::Metrics;

use anyhow::{Context, Result};
use std::sync::Arc;

pub async fn run(config: Config) -> Result<()> {
    tracing::info!(operation = "startup", message = "torrent-fuse starting");
    tracing::debug!(config = ?config, "Configuration loaded");

    // Create metrics
    let metrics = Arc::new(Metrics::new());

    // Create the filesystem
    let fs = TorrentFS::new(config, Arc::clone(&metrics))
        .context("Failed to create torrent filesystem")?;

    // Mount the filesystem (this blocks until unmounted)
    fs.mount().context("Failed to mount filesystem")?;

    // Log final metrics on shutdown
    metrics.log_full_summary();

    Ok(())
}
