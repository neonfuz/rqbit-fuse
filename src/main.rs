use anyhow::Result;
use clap::{Parser, Subcommand};
use rqbit_fuse::config::{CliArgs, Config};
use rqbit_fuse::mount::{get_mount_info, is_mount_point, setup_logging, unmount_filesystem};
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

        /// Allow other users to access the mount
        #[arg(long, env = "TORRENT_FUSE_ALLOW_OTHER")]
        allow_other: bool,

        /// Auto-unmount on process exit
        #[arg(long, env = "TORRENT_FUSE_AUTO_UNMOUNT")]
        auto_unmount: bool,
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

    /// Show status of mounted filesystems
    Status {
        /// Path to config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
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
            allow_other,
            auto_unmount,
        } => {
            setup_logging(verbose, quiet)?;
            run_mount(
                mount_point,
                api_url,
                config,
                username,
                password,
                allow_other,
                auto_unmount,
            )
            .await
        }
        Commands::Umount {
            mount_point,
            config,
            force,
        } => run_umount(mount_point, config, force).await,
        Commands::Status { config, format } => run_status(config, format).await,
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
    allow_other: bool,
    auto_unmount: bool,
) -> Result<()> {
    let mut config = load_config(config_file, mount_point, api_url, username, password)?;

    if allow_other {
        config.mount.allow_other = true;
    }
    if auto_unmount {
        config.mount.auto_unmount = true;
    }

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

async fn run_status(config_file: Option<PathBuf>, format: OutputFormat) -> Result<()> {
    let config = load_config(config_file.clone(), None, None, None, None)?;

    let mount_point = &config.mount.mount_point;
    let is_mounted = is_mount_point(mount_point).unwrap_or(false);

    match format {
        OutputFormat::Text => {
            println!("rqbit-fuse Status");
            println!("===================");
            println!();
            println!("Configuration:");
            println!(
                "  Config file:    {}",
                config_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(default)".to_string())
            );
            println!("  API URL:        {}", config.api.url);
            println!("  Mount point:    {}", mount_point.display());
            println!();
            println!("Mount Status:");
            if is_mounted {
                println!("  Status:         MOUNTED");
                if let Ok(info) = get_mount_info(mount_point) {
                    println!("  Filesystem:     {}", info.filesystem);
                    println!("  Size:           {}", info.size);
                    println!("  Used:           {}", info.used);
                    println!("  Available:      {}", info.available);
                }
            } else {
                println!("  Status:         NOT MOUNTED");
            }
        }
        OutputFormat::Json => {
            #[derive(serde::Serialize)]
            struct StatusOutput {
                mounted: bool,
                config: ConfigOutput,
                mount_info: Option<MountInfoOutput>,
            }

            #[derive(serde::Serialize)]
            struct ConfigOutput {
                api_url: String,
                mount_point: String,
            }

            #[derive(serde::Serialize)]
            struct MountInfoOutput {
                filesystem: String,
                size: String,
                used: String,
                available: String,
            }

            let output = StatusOutput {
                mounted: is_mounted,
                config: ConfigOutput {
                    api_url: config.api.url.clone(),
                    mount_point: mount_point.display().to_string(),
                },
                mount_info: if is_mounted {
                    get_mount_info(mount_point)
                        .ok()
                        .map(|info| MountInfoOutput {
                            filesystem: info.filesystem,
                            size: info.size,
                            used: info.used,
                            available: info.available,
                        })
                } else {
                    None
                },
            };

            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

use anyhow::Context;
