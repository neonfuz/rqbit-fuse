# SIMPLIFY-003: Extract Main.rs Helper Functions

## Task ID

**SIMPLIFY-003**

## Scope

- **Primary File**: `src/main.rs` (438 lines → ~362 lines, -76 lines)
- **New Module**: `src/main_helpers.rs` (create new file)
- **Update**: `src/lib.rs` to export new helpers

## Current State

### 1. Duplicated Config Loading (3 locations)

Used in `run_mount()`, `run_umount()`, and `run_status()`:

```rust
// Lines 145-151 in run_mount()
let mut config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};

// Lines 194-200 in run_umount() - IDENTICAL
let config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};

// Lines 225-231 in run_status() - IDENTICAL
let config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};
```

**Lines duplicated**: 7 lines × 3 locations = 21 lines

### 2. Shell Command Execution Pattern (3 locations)

Pattern used in `is_mount_point()`, `unmount_filesystem()`, `get_mount_info()`:

```rust
// Lines 324-330 in is_mount_point()
let output = Command::new("mount")
    .output()
    .with_context(|| "Failed to run mount command")?;

if !output.status.success() {
    anyhow::bail!("mount command failed");
}

// Lines 371-373 in unmount_filesystem() - similar pattern
let output = cmd
    .output()
    .with_context(|| "Failed to run fusermount3 command")?;

if !output.status.success() {
    // ... error handling
}

// Lines 408-411 in get_mount_info() - similar pattern
let output = Command::new("df")
    .args(["-h", &path.to_string_lossy()])
    .output()
    .with_context(|| "Failed to run df command")?;
```

### 3. Fusermount Fallback Logic (1 location, complex)

```rust
// Lines 371-398 in unmount_filesystem()
let output = cmd
    .output()
    .with_context(|| "Failed to run fusermount3 command")?;

if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Try fusermount as fallback (older systems)
    if stderr.contains("command not found") || stderr.contains("No such file") {
        let mut cmd = Command::new("fusermount");
        if force {
            cmd.arg("-zu");
        } else {
            cmd.arg("-u");
        }
        cmd.arg(path);

        let output = cmd
            .output()
            .with_context(|| "Failed to run fusermount command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to unmount: {}", stderr);
        }
    } else {
        anyhow::bail!("Failed to unmount: {}", stderr);
    }
}
```

**Lines**: 28 lines of complex fallback logic

---

## Target State

### New File: `src/main_helpers.rs`

```rust
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use torrent_fuse::config::{CliArgs, Config};

/// Load configuration from file or CLI arguments
/// 
/// Checks if a config file is specified in CLI args, and if so,
/// loads from file with env/cli merging. Otherwise loads defaults
/// with CLI overrides.
pub fn load_config(cli_args: &CliArgs) -> Result<Config> {
    if let Some(ref config_path) = cli_args.config_file {
        Config::from_file(config_path)?
            .merge_from_env()?
            .merge_from_cli(cli_args)
    } else {
        Config::load_with_cli(cli_args)
    }
}

/// Run a shell command and return output if successful
/// 
/// # Arguments
/// * `program` - The command to execute
/// * `args` - Arguments to pass to the command
/// * `context` - Error context message
/// 
/// # Returns
/// * `Ok(Output)` if command succeeds
/// * `Err` if command fails to execute or returns non-zero exit
pub fn run_command<I, S>(program: &str, args: I, context: &str) -> Result<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("Failed to execute {}", program))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}: {}", context, stderr);
    }

    Ok(output)
}

/// Try to run a command, returning true on success without error on failure
/// 
/// Useful when you want to try a command but handle failure gracefully
pub fn try_run_command<I, S>(program: &str, args: I) -> Option<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(program).args(args).output().ok()
}

/// Attempt to unmount a FUSE filesystem with fallback support
/// 
/// Tries fusermount3 first, then falls back to fusermount for older systems.
/// Supports force unmount with the -z flag when force=true.
/// 
/// # Arguments
/// * `path` - Mount point path to unmount
/// * `force` - If true, use lazy unmount (-z flag)
/// 
/// # Returns
/// * `Ok(())` on successful unmount
/// * `Err` with detailed error message on failure
pub fn try_unmount(path: &Path, force: bool) -> Result<()> {
    // Build fusermount3 command
    let mut args = vec!["-u"];
    if force {
        args.push("-z");
    }
    
    let output = Command::new("fusermount3")
        .args(&args)
        .arg(path)
        .output()
        .with_context(|| "Failed to execute fusermount3")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check if fusermount3 is not available
    if stderr.contains("command not found") || stderr.contains("No such file") {
        // Try fusermount as fallback
        let output = Command::new("fusermount")
            .args(&args)
            .arg(path)
            .output()
            .with_context(|| "Failed to execute fusermount (fallback)")?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to unmount (tried fusermount3 and fusermount): {}", stderr);
    }

    anyhow::bail!("Failed to unmount: {}", stderr)
}
```

### Updated `src/lib.rs`

Add module export:

```rust
// At module level, add:
pub mod main_helpers;
```

### Updated `src/main.rs`

Replace 3 config loading blocks with:

```rust
use torrent_fuse::main_helpers::{load_config, run_command, try_unmount};

// In run_mount(), run_umount(), run_status():
let mut config = load_config(&cli_args)?;
```

Replace unmount logic in `unmount_filesystem()`:

```rust
fn unmount_filesystem(path: &PathBuf, force: bool) -> Result<()> {
    try_unmount(path, force)
}
```

Replace mount command execution in `is_mount_point()`:

```rust
fn is_mount_point(path: &PathBuf) -> Result<bool> {
    let output = run_command("mount", Vec::<&str>::new(), "mount command failed")?;
    // ... rest of implementation
}
```

Replace df command in `get_mount_info()`:

```rust
fn get_mount_info(path: &std::path::Path) -> Result<MountInfo> {
    let output = run_command(
        "df",
        &["-h", &path.to_string_lossy()],
        "df command failed"
    )?;
    // ... rest of implementation
}
```

---

## Implementation Steps

### Step 1: Create `src/main_helpers.rs`
1. Create new file at `src/main_helpers.rs`
2. Copy the target state code above
3. Add proper module documentation
4. Verify imports compile with `cargo check`

### Step 2: Update `src/lib.rs`
1. Add `pub mod main_helpers;` to the module declarations
2. Ensure it's placed in a logical location with other module exports

### Step 3: Refactor `src/main.rs` - Config Loading
1. In `run_mount()` (line 145): Replace 7-line config block with `load_config(&cli_args)?`
2. In `run_umount()` (line 194): Replace 7-line config block with `load_config(&cli_args)?`
3. In `run_status()` (line 225): Replace 7-line config block with `load_config(&cli_args)?`
4. Add import: `use torrent_fuse::main_helpers::load_config;`

### Step 4: Refactor `src/main.rs` - Shell Commands
1. Update `is_mount_point()` (line 321): Use `run_command()` helper
2. Update `get_mount_info()` (line 404): Use `run_command()` helper
3. Update `unmount_filesystem()` (line 360): Use `try_unmount()` helper
4. Add import: `use torrent_fuse::main_helpers::{run_command, try_unmount};`

### Step 5: Update Imports in `src/main.rs`
1. Remove unused imports (verify with `cargo check`)
2. Consolidate imports from new helper module
3. Ensure no unused import warnings

### Step 6: Run Verification
1. `cargo check` - ensure no compilation errors
2. `cargo clippy` - check for warnings
3. `cargo test` - run all tests
4. `cargo fmt` - format code

---

## Testing

### Unit Tests (Add to `src/main_helpers.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_try_run_command_success() {
        // Test with 'echo' which should exist on all Unix systems
        let output = try_run_command("echo", &["hello"]);
        assert!(output.is_some());
        assert!(output.unwrap().status.success());
    }

    #[test]
    fn test_try_run_command_failure() {
        // Test with non-existent command
        let output = try_run_command("nonexistent_command_xyz", &[] as &[&str]);
        assert!(output.is_none());
    }

    #[test]
    fn test_run_command_success() {
        let result = run_command("echo", &["test"], "echo failed");
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_command_failure() {
        let result = run_command("false", &[] as &[&str], "command failed");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_with_file() {
        // Create temp config file
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_config.toml");
        std::fs::write(&config_path, "[api]\nurl = 'http://localhost:3030'").unwrap();

        let cli_args = CliArgs {
            api_url: None,
            mount_point: None,
            config_file: Some(config_path),
        };

        let result = load_config(&cli_args);
        assert!(result.is_ok());
        
        // Cleanup
        std::fs::remove_file(&config_path).ok();
    }
}
```

### Integration Testing

1. **Manual CLI Test**:
   ```bash
   cargo build --release
   ./target/release/torrent-fuse mount --help
   ./target/release/torrent-fuse status --help
   ```

2. **Test mount/unmount cycle**:
   ```bash
   mkdir -p /tmp/torrent-test-mount
   ./target/release/torrent-fuse mount -m /tmp/torrent-test-mount
   ./target/release/torrent-fuse status
   ./target/release/torrent-fuse umount -m /tmp/torrent-test-mount
   ```

3. **Verify helper function usage**:
   ```bash
   # Check that functions are being called
   cargo test --lib main_helpers::tests -- --nocapture
   ```

---

## Expected Reduction

| Change | Lines Before | Lines After | Delta |
|--------|-------------|-------------|-------|
| Config loading (3×) | 21 | 3 | -18 |
| Shell command exec (3×) | 12 | 3 | -9 |
| Fusermount fallback | 28 | 1 | -27 |
| Helper module creation | 0 | +61 | +61 |
| Import statements | 6 | 4 | -2 |
| **Total** | **67** | **72** | **-76** |

**Net reduction**: ~76 lines in `src/main.rs`

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Config loading behavior change | High | Write unit tests for all config loading paths |
| Shell command error messages change | Medium | Verify error messages match expected format |
| Fusermount fallback edge cases | Medium | Test on systems with only fusermount (no fusermount3) |
| Import conflicts | Low | Use `cargo check` immediately after each change |

---

## Success Criteria

- [ ] `src/main_helpers.rs` created with all 4 functions
- [ ] All 3 config loading sites use `load_config()`
- [ ] All shell commands use `run_command()` or `try_run_command()`
- [ ] `unmount_filesystem()` delegates to `try_unmount()`
- [ ] `cargo test` passes
- [ ] `cargo clippy` has no warnings
- [ ] `cargo fmt` makes no changes
- [ ] Line count in main.rs reduced by ~76 lines
- [ ] Integration tests pass for mount/umount/status commands

---

## Related Tasks

- **ARCH-003**: Extract mount operations (related to this work)
- **ERROR-002**: Replace string matching with typed errors (may affect error handling in helpers)

*Created: February 14, 2026*
