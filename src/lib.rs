pub mod api;
pub mod cache;
pub mod config;
pub mod fs;
pub mod types;

pub use cache::{Cache, CacheStats};
pub use config::{CliArgs, Config};
pub use fs::filesystem::TorrentFS;

use anyhow::{Context, Result};

pub async fn run(config: Config) -> Result<()> {
    tracing::info!("torrent-fuse starting");
    tracing::debug!("Configuration: {:?}", config);

    // Create the filesystem
    let fs = TorrentFS::new(config).context("Failed to create torrent filesystem")?;

    // Mount the filesystem (this blocks until unmounted)
    fs.mount().context("Failed to mount filesystem")?;

    Ok(())
}
