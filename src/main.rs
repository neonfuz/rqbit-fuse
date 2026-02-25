use anyhow::Result;
use clap::{Parser, Subcommand};
use rqbit_fuse::config::{CliArgs, Config};
use rqbit_fuse::mount::{is_mount_point, setup_logging, unmount_filesystem};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rqbit-fuse")]
#[command(about = "A FUSE filesystem for accessing torrents via rqbit")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mount the torrent filesystem
    Mount {
        /// Path to mount point (overrides config)
        #[arg(short, long, env = "TORRENT_FUSE_MOUNT_POINT")]
        mount_point: Option<PathBuf>,

        /// rqbit API URL (overrides config)
        #[arg(short, long, env = "TORRENT_FUSE_API_URL")]
        api_url: Option<String>,

        /// Path to config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// rqbit API username for HTTP Basic Auth (overrides config)
        #[arg(long, env = "TORRENT_FUSE_AUTH_USERNAME")]
        username: Option<String>,

        /// rqbit API password for HTTP Basic Auth (overrides config)
        #[arg(long, env = "TORRENT_FUSE_AUTH_PASSWORD")]
        password: Option<String>,

        /// Increase verbosity (can be used multiple times)
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,

        /// Suppress all output except errors
        #[arg(short, long)]
        quiet: bool,
    },

    /// Unmount the torrent filesystem
    Umount {
        /// Path to mount point (overrides config)
        #[arg(short, long, env = "TORRENT_FUSE_MOUNT_POINT")]
        mount_point: Option<PathBuf>,

        /// Path to config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// Force unmount even if filesystem is busy
        #[arg(short, long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Mount {
            mount_point,
            api_url,
            config,
            username,
            password,
            verbose,
            quiet,
        } => {
            setup_logging(verbose, quiet)?;
            run_mount(mount_point, api_url, config, username, password).await
        }
        Commands::Umount {
            mount_point,
            config,
            force,
        } => run_umount(mount_point, config, force).await,
    }
}

fn load_config(
    config_file: Option<PathBuf>,
    mount_point: Option<PathBuf>,
    api_url: Option<String>,
    username: Option<String>,
    password: Option<String>,
) -> Result<Config> {
    let cli_args = CliArgs {
        api_url,
        mount_point,
        config_file: config_file.clone(),
        username,
        password,
    };

    if let Some(ref config_path) = config_file {
        Ok(Config::from_file(config_path)?
            .merge_from_env()?
            .merge_from_cli(&cli_args))
    } else {
        Ok(Config::load_with_cli(&cli_args)?)
    }
}

async fn run_mount(
    mount_point: Option<PathBuf>,
    api_url: Option<String>,
    config_file: Option<PathBuf>,
    username: Option<String>,
    password: Option<String>,
) -> Result<()> {
    let config = load_config(config_file, mount_point, api_url, username, password)?;

    if !config.mount.mount_point.exists() {
        tracing::info!(
            "Creating mount point: {}",
            config.mount.mount_point.display()
        );
        std::fs::create_dir_all(&config.mount.mount_point).with_context(|| {
            format!(
                "Failed to create mount point: {}",
                config.mount.mount_point.display()
            )
        })?;
    }

    tracing::info!("rqbit-fuse starting");
    tracing::info!("Using rqbit API at: {}", config.api.url);
    tracing::info!("Mount point: {}", config.mount.mount_point.display());
    tracing::debug!("Configuration: {:?}", config);

    rqbit_fuse::run(config).await
}

async fn run_umount(
    mount_point: Option<PathBuf>,
    config_file: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let config = load_config(config_file, mount_point.clone(), None, None, None)?;

    let mount_point = mount_point.unwrap_or_else(|| config.mount.mount_point.clone());

    tracing::info!("Unmounting: {}", mount_point.display());

    if !is_mount_point(&mount_point)? {
        anyhow::bail!("{} is not a mount point", mount_point.display());
    }

    unmount_filesystem(&mount_point, force)?;

    tracing::info!("Successfully unmounted {}", mount_point.display());
    Ok(())
}

use anyhow::Context;
