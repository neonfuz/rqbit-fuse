use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use torrent_fuse::config::{CliArgs, Config};

#[derive(Parser)]
#[command(name = "torrent-fuse")]
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
            verbose,
            quiet,
            allow_other,
            auto_unmount,
        } => {
            setup_logging(verbose, quiet)?;
            run_mount(mount_point, api_url, config, allow_other, auto_unmount).await
        }
        Commands::Umount {
            mount_point,
            config,
            force,
        } => run_umount(mount_point, config, force).await,
        Commands::Status { config, format } => run_status(config, format).await,
    }
}

fn setup_logging(verbose: u8, quiet: bool) -> Result<()> {
    use tracing_subscriber::fmt;

    if quiet {
        let subscriber = fmt()
            .with_max_level(tracing::Level::ERROR)
            .without_time()
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let level = match verbose {
            0 => tracing::Level::INFO,
            1 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        };

        let subscriber = fmt().with_max_level(level).with_target(true).finish();

        tracing::subscriber::set_global_default(subscriber)?;
    }

    Ok(())
}

/// Load configuration from CLI arguments
///
/// Handles the config file -> env -> CLI merge order
fn load_config(
    config_file: Option<PathBuf>,
    mount_point: Option<PathBuf>,
    api_url: Option<String>,
) -> Result<Config> {
    let cli_args = CliArgs {
        api_url,
        mount_point,
        config_file: config_file.clone(),
    };

    if let Some(ref config_path) = config_file {
        Ok(Config::from_file(config_path)?
            .merge_from_env()?
            .merge_from_cli(&cli_args))
    } else {
        Ok(Config::load_with_cli(&cli_args)?)
    }
}

/// Run a shell command and return output on success
///
/// # Arguments
/// * `program` - The program to execute
/// * `args` - Arguments to pass to the program
/// * `context` - Context message for error reporting
///
/// # Returns
/// * `Ok(Output)` if command succeeds
/// * `Err` if command fails to run or returns non-zero exit code
fn run_command<S: AsRef<std::ffi::OsStr>>(
    program: &str,
    args: &[S],
    context: &str,
) -> Result<std::process::Output> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run {}", context))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} failed: {}", context, stderr);
    }

    Ok(output)
}

/// Unmount a FUSE filesystem, trying fusermount3 then fusermount
///
/// # Arguments
/// * `path` - Mount point to unmount
/// * `force` - Whether to force unmount even if busy
///
/// # Returns
/// * `Ok(())` on successful unmount
/// * `Err` if both fusermount3 and fusermount fail
fn try_unmount(path: &std::path::Path, force: bool) -> Result<()> {
    let path_str = path.to_string_lossy();
    let args: Vec<&str> = if force {
        vec!["-zu", &path_str]
    } else {
        vec!["-u", &path_str]
    };

    // Try fusermount3 first (modern systems)
    match run_command("fusermount3", &args, "fusermount3") {
        Ok(_) => return Ok(()),
        Err(e) => {
            let err_str = e.to_string();
            // Only try fallback if command not found
            if !err_str.contains("command not found") && !err_str.contains("No such file") {
                return Err(e);
            }
        }
    }

    // Fallback to fusermount (older systems)
    run_command("fusermount", &args, "fusermount").map(|_| ())
}

async fn run_mount(
    mount_point: Option<PathBuf>,
    api_url: Option<String>,
    config_file: Option<PathBuf>,
    allow_other: bool,
    auto_unmount: bool,
) -> Result<()> {
    let mut config = load_config(config_file, mount_point, api_url)?;

    // Apply command-line overrides for mount options
    if allow_other {
        config.mount.allow_other = true;
    }
    if auto_unmount {
        config.mount.auto_unmount = true;
    }

    // Validate mount point exists
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

    tracing::info!("torrent-fuse starting");
    tracing::info!("Using rqbit API at: {}", config.api.url);
    tracing::info!("Mount point: {}", config.mount.mount_point.display());
    tracing::debug!("Configuration: {:?}", config);

    torrent_fuse::run(config).await
}

async fn run_umount(
    mount_point: Option<PathBuf>,
    config_file: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let config = load_config(config_file, mount_point.clone(), None)?;

    let mount_point = mount_point.unwrap_or_else(|| config.mount.mount_point.clone());

    tracing::info!("Unmounting: {}", mount_point.display());

    // Check if the path is actually a mount point
    if !is_mount_point(&mount_point)? {
        anyhow::bail!("{} is not a mount point", mount_point.display());
    }

    // Perform unmount
    unmount_filesystem(&mount_point, force)?;

    tracing::info!("Successfully unmounted {}", mount_point.display());
    Ok(())
}

async fn run_status(config_file: Option<PathBuf>, format: OutputFormat) -> Result<()> {
    let config = load_config(config_file.clone(), None, None)?;

    let mount_point = &config.mount.mount_point;
    let is_mounted = is_mount_point(mount_point).unwrap_or(false);

    match format {
        OutputFormat::Text => {
            println!("torrent-fuse Status");
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
                mount_info: Option<MountInfo>,
            }

            #[derive(serde::Serialize)]
            struct ConfigOutput {
                api_url: String,
                mount_point: String,
            }

            #[derive(serde::Serialize)]
            struct MountInfo {
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
                    get_mount_info(mount_point).ok().map(|info| MountInfo {
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

#[derive(Debug)]
struct MountInfo {
    filesystem: String,
    size: String,
    used: String,
    available: String,
}

fn is_mount_point(path: &PathBuf) -> Result<bool> {
    use std::process::Command;

    let output = Command::new("mount")
        .output()
        .with_context(|| "Failed to run mount command")?;

    if !output.status.success() {
        anyhow::bail!("mount command failed");
    }

    let mount_output = String::from_utf8_lossy(&output.stdout);
    let path_str = path.to_string_lossy();

    // Check if the path appears in mount output
    // This is a simple check that works on most Unix systems
    for line in mount_output.lines() {
        if line.contains(&*path_str) {
            return Ok(true);
        }
    }

    // Also check using stat to compare device IDs
    // If the path and its parent have different device IDs, it's a mount point
    if cfg!(target_os = "linux") {
        use std::os::unix::fs::MetadataExt;
        let path_meta = std::fs::metadata(path)
            .with_context(|| format!("Failed to stat {}", path.display()))?;
        let root = PathBuf::from("/");
        let parent = path.parent().unwrap_or(&root);
        let parent_meta = std::fs::metadata(parent)
            .with_context(|| format!("Failed to stat parent of {}", path.display()))?;

        return Ok(path_meta.dev() != parent_meta.dev());
    }

    Ok(false)
}

fn unmount_filesystem(path: &std::path::Path, force: bool) -> Result<()> {
    try_unmount(path, force)
}

fn get_mount_info(path: &std::path::Path) -> Result<MountInfo> {
    use std::process::Command;

    // Try df command first
    let output = Command::new("df")
        .args(["-h", &path.to_string_lossy()])
        .output()
        .with_context(|| "Failed to run df command")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse df output
        // Format: Filesystem Size Used Avail Use% Mounted on
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                return Ok(MountInfo {
                    filesystem: parts[0].to_string(),
                    size: parts[1].to_string(),
                    used: parts[2].to_string(),
                    available: parts[3].to_string(),
                });
            }
        }
    }

    // Fallback: return basic info
    Ok(MountInfo {
        filesystem: "fuse.torrent-fuse".to_string(),
        size: "unknown".to_string(),
        used: "unknown".to_string(),
        available: "unknown".to_string(),
    })
}
