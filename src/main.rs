use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use torrent_fuse::config::{CliArgs, Config};

#[derive(Parser)]
#[command(name = "torrent-fuse")]
#[command(about = "A FUSE filesystem for accessing torrents via rqbit")]
#[command(version)]
struct Cli {
    #[arg(short, long, help = "Increase verbosity")]
    verbose: bool,

    #[arg(short, long, help = "Path to config file", value_name = "FILE")]
    config: Option<PathBuf>,

    #[arg(short, long, help = "rqbit API URL", env = "TORRENT_FUSE_API_URL")]
    api_url: Option<String>,

    #[arg(short, long, help = "Mount point path", env = "TORRENT_FUSE_MOUNT_POINT")]
    mount_point: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let cli_args = CliArgs {
        api_url: cli.api_url,
        mount_point: cli.mount_point,
        config_file: cli.config,
    };

    let config = if let Some(ref config_path) = cli_args.config_file {
        Config::from_file(config_path)?
            .merge_from_env()?
            .merge_from_cli(&cli_args)
    } else {
        Config::load_with_cli(&cli_args)?
    };

    tracing::info!("Using rqbit API at: {}", config.api.url);
    tracing::info!("Mount point: {}", config.mount.mount_point.display());

    torrent_fuse::run(config).await
}
