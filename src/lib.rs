pub mod api;
pub mod config;
pub mod fs;
pub mod types;

use anyhow::Result;

pub async fn run() -> Result<()> {
    tracing::info!("torrent-fuse starting");
    Ok(())
}
