# Phase 1: Code Removal - Status Command Migration Guide

This guide provides step-by-step instructions for removing the `status` command from the rqbit-fuse codebase.

## Overview

The `status` command is being removed from the CLI. This migration removes:
1. The `Status` enum variant from the `Commands` enum
2. The match arm handling `Commands::Status` in the `main()` function  
3. The `run_status()` function implementation
4. Any associated documentation and imports

**Affected File:** `src/main.rs`

## Prerequisites

- Review the spec changes in commit `4ff33398b08c7ed4fe01ca4475d4cd2278fa5650`
- Ensure you have the latest code from the main branch
- Run tests before starting: `nix-shell --run 'cargo test'`

---

## Step 1: Remove the Status Enum Variant

**Location:** `src/main.rs`, Lines 64-69

### Current Code (to remove):

```rust
    /// Show status of mounted filesystems
    Status {
        /// Path to config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
```

### Action:

Delete lines 64-69 from the `Commands` enum. The enum should transition from:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Mount the torrent filesystem
    Mount {
        // ... mount fields
    },

    /// Unmount the torrent filesystem
    Umount {
        // ... umount fields
    },

    /// Show status of mounted filesystems
    Status {
        /// Path to config file
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
}
```

To:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Mount the torrent filesystem
    Mount {
        // ... mount fields
    },

    /// Unmount the torrent filesystem
    Umount {
        // ... umount fields
    },
}
```

### Verification:

```bash
nix-shell --run 'cargo check'
```

Expected: Compilation errors in `main()` due to missing match arm (to be fixed in Step 2).

---

## Step 2: Remove the Status Match Arm

**Location:** `src/main.rs`, Line 94

### Current Code (to remove):

```rust
        Commands::Status { config } => run_status(config).await,
```

### Action:

Delete line 94 from the `match cli.command` block in `main()`. The match block should transition from:

```rust
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
        Commands::Status { config } => run_status(config).await,
    }
```

To:

```rust
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
```

### Verification:

```bash
nix-shell --run 'cargo check'
```

Expected: Compilation errors due to undefined `run_status` function (to be fixed in Step 3).

---

## Step 3: Remove the run_status() Function

**Location:** `src/main.rs`, Lines 173-201

### Current Code (to remove):

```rust
async fn run_status(config_file: Option<PathBuf>) -> Result<()> {
    let config = load_config(config_file.clone(), None, None, None, None)?;

    let mount_point = &config.mount.mount_point;
    let is_mounted = is_mount_point(mount_point).unwrap_or(false);

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
    } else {
        println!("  Status:         NOT MOUNTED");
    }

    Ok(())
}
```

### Action:

Delete the entire `run_status()` function (lines 173-201). Note that this function uses:
- `is_mount_point()` - This function is also used in `run_umount()`, so it should NOT be removed from imports
- `load_config()` - Still needed by other functions

### Verification:

```bash
nix-shell --run 'cargo build'
```

Expected: Clean compilation with no errors.

---

## Step 4: Import Cleanup (Optional)

After removing the `run_status()` function, verify if any imports have become unused.

### Check Imports:

Review the imports at the top of `src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use rqbit_fuse::config::{CliArgs, Config};
use rqbit_fuse::mount::{is_mount_point, setup_logging, unmount_filesystem};
use std::path::PathBuf;
```

### Analysis:

- `is_mount_point` - **KEEP**: Still used in `run_umount()` at line 163
- `setup_logging` - **KEEP**: Used in `main()` for the Mount command
- `unmount_filesystem` - **KEEP**: Used in `run_umount()`
- All other imports - **KEEP**: Still in use

**No import cleanup is required** for this migration.

### Verification:

```bash
nix-shell --run 'cargo clippy'
```

Expected: No warnings about unused imports.

---

## Final Verification

### Build and Test:

```bash
# Build the project
nix-shell --run 'cargo build'

# Run all tests
nix-shell --run 'cargo test'

# Run linting
nix-shell --run 'cargo clippy'

# Format code
nix-shell --run 'cargo fmt'
```

### Manual Verification:

Test that the CLI no longer accepts the `status` subcommand:

```bash
# This should fail with "error: unrecognized subcommand 'status'"
cargo run -- status

# These should still work
cargo run -- --help
cargo run -- mount --help
cargo run -- umount --help
```

---

## Summary of Changes

| Location | Lines | Change |
|----------|-------|--------|
| `Commands` enum | 64-69 | Remove `Status` variant |
| `main()` match | 94 | Remove `Commands::Status` arm |
| `run_status()` function | 173-201 | Remove entire function |
| Imports | N/A | No changes needed |

**Total lines removed:** ~29 lines

---

## Rollback Instructions

If you need to rollback this migration:

1. Restore from git: `git checkout src/main.rs`
2. Or manually re-add the three sections removed above

---

## Related Documentation

- Spec changes: See commit `4ff33398b08c7ed4fe01ca4475d4cd2278fa5650`
- Architecture: `spec/architecture.md`
- API Documentation: `spec/api.md`
