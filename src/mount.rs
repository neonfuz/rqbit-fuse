use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn setup_logging(verbose: u8, quiet: bool) -> Result<()> {
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

pub fn run_command<S: AsRef<std::ffi::OsStr>>(
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

pub fn try_unmount(path: &std::path::Path, force: bool) -> Result<()> {
    let path_str = path.to_string_lossy();
    let args: Vec<&str> = if force {
        vec!["-zu", &path_str]
    } else {
        vec!["-u", &path_str]
    };

    match run_command("fusermount3", &args, "fusermount3") {
        Ok(_) => return Ok(()),
        Err(e) => {
            let err_str = e.to_string();
            if !err_str.contains("command not found") && !err_str.contains("No such file") {
                return Err(e);
            }
        }
    }

    run_command("fusermount", &args, "fusermount").map(|_| ())
}

pub fn is_mount_point(path: &PathBuf) -> Result<bool> {
    use std::process::Command;

    let output = Command::new("mount")
        .output()
        .with_context(|| "Failed to run mount command")?;

    if !output.status.success() {
        anyhow::bail!("mount command failed");
    }

    let mount_output = String::from_utf8_lossy(&output.stdout);
    let path_str = path.to_string_lossy();

    for line in mount_output.lines() {
        if line.contains(&*path_str) {
            return Ok(true);
        }
    }

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

pub fn unmount_filesystem(path: &std::path::Path, force: bool) -> Result<()> {
    try_unmount(path, force)
}

#[derive(Debug)]
pub struct MountInfo {
    pub filesystem: String,
    pub size: String,
    pub used: String,
    pub available: String,
}

pub fn get_mount_info(path: &std::path::Path) -> Result<MountInfo> {
    use std::process::Command;

    let output = Command::new("df")
        .args(["-h", &path.to_string_lossy()])
        .output()
        .with_context(|| "Failed to run df command")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
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

    Ok(MountInfo {
        filesystem: "fuse.torrent-fuse".to_string(),
        size: "unknown".to_string(),
        used: "unknown".to_string(),
        available: "unknown".to_string(),
    })
}
