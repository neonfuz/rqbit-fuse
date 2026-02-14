pub mod api;
pub mod config;
pub mod fs;
pub mod types;

pub use config::{CliArgs, Config};

use anyhow::Result;

pub async fn run(config: Config) -> Result<()> {
    tracing::info!("torrent-fuse starting");
    tracing::debug!("Configuration: {:?}", config);
    Ok(())
}
