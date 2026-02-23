# SIMPLIFY-003: Extract Helper Functions from main.rs

## Task ID
**SIMPLIFY-003**

## Scope
- **Primary File**: `src/main.rs`
- **Lines Affected**: 145-151, 194-200, 225-231 (config loading), 363-399 (unmount fallback)
- **New Helper Location**: `src/main.rs` (internal helpers)

## Current State

Three code patterns are duplicated across command handlers:

### 1. Config Loading (duplicated 3 times)

**Location 1**: `run_mount()` lines 139-151
```rust
let cli_args = CliArgs {
    api_url,
    mount_point,
    config_file,
};

let mut config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};
```

**Location 2**: `run_umount()` lines 188-200
```rust
let cli_args = CliArgs {
    api_url: None,
    mount_point: mount_point.clone(),
    config_file,
};

let config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};
```

**Location 3**: `run_status()` lines 219-231
```rust
let cli_args = CliArgs {
    api_url: None,
    mount_point: None,
    config_file,
};

let config = if let Some(ref config_path) = cli_args.config_file {
    Config::from_file(config_path)?
        .merge_from_env()?
        .merge_from_cli(&cli_args)
} else {
    Config::load_with_cli(&cli_args)?
};
```

### 2. Fusermount Fallback Logic (duplicated command execution pattern)

**Location**: `unmount_filesystem()` lines 363-399
```rust
let mut cmd = Command::new("fusermount3");
if force {
    cmd.arg("-zu");
} else {
    cmd.arg("-u");
}
cmd.arg(path);

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

### 3. Shell Command Execution Pattern (3 locations)

**Location 1**: `is_mount_point()` lines 324-330
```rust
let output = Command::new("mount")
    .output()
    .with_context(|| "Failed to run mount command")?;

if !output.status.success() {
    anyhow::bail!("mount command failed");
}
```

**Location 2**: `get_mount_info()` lines 408-413
```rust
let output = Command::new("df")
    .args(["-h", &path.to_string_lossy()])
    .output()
    .with_context(|| "Failed to run df command")?;
```

## Target State

### Extracted Helper 1: `load_config()`

```rust
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
        config_file,
    };

    if let Some(ref config_path) = cli_args.config_file {
        Config::from_file(config_path)?
            .merge_from_env()?
            .merge_from_cli(&cli_args)
    } else {
        Config::load_with_cli(&cli_args)
    }
}
```

**Usage in `run_mount()`**:
```rust
let mut config = load_config(config_file, mount_point, api_url)?;
```

**Usage in `run_umount()`**:
```rust
let config = load_config(config_file, mount_point.clone(), None)?;
```

**Usage in `run_status()`**:
```rust
let config = load_config(config_file, None, None)?;
```

### Extracted Helper 2: `run_command()` / `try_run_command()`

```rust
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

/// Try to run a command, returning true on success, false on failure
/// 
/// Does not return an error - useful when checking optional commands
fn try_run_command<S: AsRef<std::ffi::OsStr>>(
    program: &str,
    args: &[S],
) -> bool {
    std::process::Command::new(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

**Usage in `is_mount_point()`**:
```rust
// Before:
let output = Command::new("mount")
    .output()
    .with_context(|| "Failed to run mount command")?;

if !output.status.success() {
    anyhow::bail!("mount command failed");
}

// After:
let output = run_command("mount", &[] as &[&str], "mount command")?;
```

### Extracted Helper 3: `try_unmount()`

```rust
/// Unmount a FUSE filesystem, trying fusermount3 then fusermount
/// 
/// # Arguments
/// * `path` - Mount point to unmount
/// * `force` - Whether to force unmount even if busy
/// 
/// # Returns
/// * `Ok(())` on successful unmount
/// * `Err` if both fusermount3 and fusermount fail
fn try_unmount(path: &PathBuf, force: bool) -> Result<()> {
    let args = if force {
        vec!["-zu", &path.to_string_lossy()]
    } else {
        vec!["-u", &path.to_string_lossy()]
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
    run_command("fusermount", &args, "fusermount")
        .map(|_| ())
}
```

**Usage in `unmount_filesystem()`**:
```rust
// Before: 37 lines of inline code
// After:
try_unmount(&mount_point, force)?;
```

## Implementation Steps

1. **Add `load_config()` helper**
   - Insert after `setup_logging()` function (around line 130)
   - Copy signature and implementation from Target State
   - Mark as `#[allow(dead_code)]` temporarily

2. **Replace config loading in `run_mount()`**
   - Lines 139-151: Replace with `load_config(config_file, mount_point, api_url)?`
   - Add `mut` if config needs modification: `let mut config = load_config(...)?`
   - Remove unused `cli_args` binding

3. **Replace config loading in `run_umount()`**
   - Lines 188-200: Replace with `load_config(config_file, mount_point.clone(), None)?`
   - Remove unused `cli_args` binding

4. **Replace config loading in `run_status()`**
   - Lines 219-231: Replace with `load_config(config_file, None, None)?`
   - Remove unused `cli_args` binding

5. **Remove `#[allow(dead_code)]` from `load_config()`**
   - Now that all call sites use it

6. **Add `run_command()` helper**
   - Insert after `load_config()` function
   - Implement as shown in Target State

7. **Add `try_unmount()` helper**
   - Insert after `run_command()` function
   - Implement as shown in Target State
   - Uses `run_command()` internally

8. **Replace unmount logic in `unmount_filesystem()`**
   - Lines 363-399: Replace entire block with `try_unmount(path, force)?`
   - Keep function as thin wrapper for any additional logic

9. **Verify all helpers are used**
   - Remove any `#[allow(dead_code)]` attributes
   - Run `cargo check` to ensure no warnings

10. **Format and lint**
    - Run `cargo fmt`
    - Run `cargo clippy`
    - Fix any issues

## Testing

### Build Verification
```bash
cargo build
```

### Test All Commands Still Work
```bash
# Test mount command (dry run - just check args parsing)
cargo run -- mount --help

# Test umount command
cargo run -- umount --help

# Test status command
cargo run -- status --help
```

### Test Config Loading
```bash
# Create a test config
cat > /tmp/test-config.toml << 'EOF'
[api]
url = "http://localhost:3030"

[mount]
mount_point = "/tmp/test-mount"
EOF

# Test that config loads correctly
cargo run -- status --config /tmp/test-config.toml
```

### Verify No Regression
```bash
cargo test
cargo clippy -- -D warnings
```

## Expected Reduction

**Lines removed**: ~76 lines

| Location | Before | After | Reduction |
|----------|--------|-------|-----------|
| Config loading (3x) | 39 lines | 3 lines | -36 lines |
| `unmount_filesystem()` | 42 lines | 4 lines | -38 lines |
| Helper functions added | 0 lines | +45 lines | +45 lines |
| **Net change** | 81 lines | 52 lines | **-29 lines** |

Note: While the raw line count reduction is ~29 lines, the actual code duplication eliminated is ~76 lines (3x config loading at ~13 lines each = 39 lines, plus unmount logic ~37 lines).

## Related Tasks

- None (this is a standalone refactoring)

## Dependencies

- None - this is a pure refactoring with no functional changes

## Notes

- The `run_command()` helper could be extended later to support:
  - Timeout handling
  - Environment variable injection
  - Working directory changes
  - Stdin input
  
- Consider moving helpers to a separate module if `main.rs` grows beyond 300 lines

- The `try_unmount()` pattern (try modern tool, fallback to legacy) could be generalized into a `try_commands!()` macro if more commands need similar fallback behavior

---

*Migration guide created: February 14, 2026*
*Task Type: Refactoring / Code Simplification*
