# Migration Guide: SIMPLIFY-001 - Config Macros

**Task ID:** SIMPLIFY-001  
**Status:** 📝 Ready for Implementation  
**Estimated Line Reduction:** ~168 lines  
**Priority:** Medium

---

## Scope

**Files to Modify:**
- `src/config/mod.rs` - Main configuration module
- Add macro definitions at the top of the file

---

## Current State

The `src/config/mod.rs` file contains significant boilerplate for:
1. **25 default value functions** (lines 85-163) - ~78 lines
2. **6 Default trait implementations** (lines 165-225) - ~61 lines  
3. **24 environment variable merge blocks** (lines 269-393) - ~125 lines

**Total boilerplate: ~264 lines**

### Repetitive Pattern 1: Default Functions (25 instances)

```rust
fn default_api_url() -> String {
    "http://127.0.0.1:3030".to_string()
}

fn default_metadata_ttl() -> u64 {
    60
}

fn default_max_entries() -> usize {
    1000
}
// ... 22 more similar functions
```

### Repetitive Pattern 2: Default Trait Implementations (6 instances)

```rust
impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            url: default_api_url(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: default_metadata_ttl(),
            torrent_list_ttl: default_torrent_list_ttl(),
            piece_ttl: default_piece_ttl(),
            max_entries: default_max_entries(),
        }
    }
}
// ... 4 more similar impl blocks
```

### Repetitive Pattern 3: Environment Variable Merging (24 instances)

```rust
if let Ok(url) = std::env::var("TORRENT_FUSE_API_URL") {
    self.api.url = url;
}

if let Ok(mount_point) = std::env::var("TORRENT_FUSE_MOUNT_POINT") {
    self.mount.mount_point = PathBuf::from(mount_point);
}

if let Ok(ttl) = std::env::var("TORRENT_FUSE_METADATA_TTL") {
    self.cache.metadata_ttl = ttl.parse().map_err(|_| {
        ConfigError::InvalidValue("TORRENT_FUSE_METADATA_TTL must be a number".into())
    })?;
}
// ... 21 more similar blocks
```

---

## Target State

Three macros will eliminate the boilerplate:

### Macro 1: `default_fn!`

```rust
macro_rules! default_fn {
    ($name:ident, $ty:ty, $val:expr) => {
        fn $name() -> $ty {
            $val
        }
    };
}
```

**Usage:**
```rust
default_fn!(default_api_url, String, "http://127.0.0.1:3030".to_string());
default_fn!(default_metadata_ttl, u64, 60);
default_fn!(default_max_entries, usize, 1000);
// ... etc
```

### Macro 2: `default_impl!`

```rust
macro_rules! default_impl {
    ($struct:ty, $($field:ident: $default_fn:ident),* $(,)?) => {
        impl Default for $struct {
            fn default() -> Self {
                Self {
                    $($field: $default_fn(),)*
                }
            }
        }
    };
}
```

**Usage:**
```rust
default_impl!(ApiConfig, url: default_api_url);
default_impl!(CacheConfig, 
    metadata_ttl: default_metadata_ttl,
    torrent_list_ttl: default_torrent_list_ttl,
    piece_ttl: default_piece_ttl,
    max_entries: default_max_entries,
);
// ... etc
```

### Macro 3: `env_var!`

```rust
macro_rules! env_var {
    // String type - no parsing needed
    ($self:ident, $env_name:expr, $field:expr) => {
        if let Ok(val) = std::env::var($env_name) {
            $field = val;
        }
    };
    
    // Type with parsing (numbers, bools, PathBuf)
    ($self:ident, $env_name:expr, $field:expr, $ty:ty, $parse:expr) => {
        if let Ok(val) = std::env::var($env_name) {
            $field = val.parse::<$ty>().map_err(|_| {
                ConfigError::InvalidValue(concat!($env_name, " has invalid format").into())
            })?;
        }
    };
}
```

**Usage:**
```rust
pub fn merge_from_env(mut self) -> Result<Self, ConfigError> {
    env_var!(self, "TORRENT_FUSE_API_URL", self.api.url);
    env_var!(self, "TORRENT_FUSE_MOUNT_POINT", self.mount.mount_point, PathBuf, PathBuf::from);
    env_var!(self, "TORRENT_FUSE_METADATA_TTL", self.cache.metadata_ttl, u64, |s| s.parse());
    // ... etc
    Ok(self)
}
```

**Simplified struct definitions** (remove `#[serde(default = "...")]`):
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheConfig {
    pub metadata_ttl: u64,
    pub torrent_list_ttl: u64,
    pub piece_ttl: u64,
    pub max_entries: usize,
}
```

---

## Implementation Steps

### Phase 1: Add Macro Definitions

- [ ] Add `default_fn!` macro at the top of `src/config/mod.rs`
- [ ] Add `default_impl!` macro after `default_fn!`
- [ ] Add `env_var!` macro after `default_impl!`

### Phase 2: Replace Default Functions

- [ ] Replace `default_api_url()` function with macro call
- [ ] Replace `default_metadata_ttl()` function with macro call
- [ ] Replace `default_torrent_list_ttl()` function with macro call
- [ ] Replace `default_piece_ttl()` function with macro call
- [ ] Replace `default_max_entries()` function with macro call
- [ ] Replace `default_mount_point()` function with macro call
- [ ] Replace `default_allow_other()` function with macro call
- [ ] Replace `default_auto_unmount()` function with macro call
- [ ] Replace `default_read_timeout()` function with macro call
- [ ] Replace `default_max_concurrent_reads()` function with macro call
- [ ] Replace `default_readahead_size()` function with macro call
- [ ] Replace `default_piece_check_enabled()` function with macro call
- [ ] Replace `default_return_eagain_for_unavailable()` function with macro call
- [ ] Replace `default_status_poll_interval()` function with macro call
- [ ] Replace `default_stalled_timeout()` function with macro call
- [ ] Replace `default_log_level()` function with macro call
- [ ] Replace `default_log_fuse_operations()` function with macro call
- [ ] Replace `default_log_api_calls()` function with macro call
- [ ] Replace `default_metrics_enabled()` function with macro call
- [ ] Replace `default_metrics_interval_secs()` function with macro call

### Phase 3: Replace Default Implementations

- [ ] Replace `impl Default for ApiConfig` with macro call
- [ ] Replace `impl Default for CacheConfig` with macro call
- [ ] Replace `impl Default for MountConfig` with macro call
- [ ] Replace `impl Default for PerformanceConfig` with macro call
- [ ] Replace `impl Default for MonitoringConfig` with macro call
- [ ] Replace `impl Default for LoggingConfig` with macro call

### Phase 4: Replace Environment Variable Merging

- [ ] Replace env var block for `TORRENT_FUSE_API_URL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_MOUNT_POINT` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_METADATA_TTL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_TORRENT_LIST_TTL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_PIECE_TTL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_MAX_ENTRIES` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_READ_TIMEOUT` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_MAX_CONCURRENT_READS` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_READAHEAD_SIZE` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_ALLOW_OTHER` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_AUTO_UNMOUNT` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_STATUS_POLL_INTERVAL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_STALLED_TIMEOUT` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_PIECE_CHECK_ENABLED` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_RETURN_EAGAIN` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_LOG_LEVEL` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_LOG_FUSE_OPS` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_LOG_API_CALLS` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_METRICS_ENABLED` with macro call
- [ ] Replace env var block for `TORRENT_FUSE_METRICS_INTERVAL` with macro call

### Phase 5: Cleanup

- [ ] Remove `#[serde(default = "...")]` attributes from struct fields
- [ ] Verify all default functions are replaced
- [ ] Verify all Default impls are replaced
- [ ] Verify merge_from_env uses macros exclusively

---

## Testing

### Step 1: Unit Tests

```bash
cargo test --lib config::
```

**Expected:** All existing tests pass:
- `test_default_config` - Verifies default values
- `test_toml_config_parsing` - Verifies TOML deserialization
- `test_json_config_parsing` - Verifies JSON deserialization
- `test_merge_from_cli` - Verifies CLI merging

### Step 2: Compile Check

```bash
cargo check
```

**Expected:** No compilation errors, no macro-related warnings

### Step 3: Clippy

```bash
cargo clippy -- -D warnings
```

**Expected:** No warnings related to macros or config module

### Step 4: Manual Verification

Create test script `test_config.sh`:

```bash
#!/bin/bash
set -e

# Test 1: Default values
echo "Testing default config..."
cargo test test_default_config -- --nocapture

# Test 2: Environment variable override
echo "Testing env var override..."
TORRENT_FUSE_API_URL="http://test:8080" cargo test test_env_override -- --nocapture 2>/dev/null || echo "Add test_env_override test!"

# Test 3: Config file parsing
echo "Testing config file parsing..."
cargo test test_toml_config_parsing -- --nocapture
cargo test test_json_config_parsing -- --nocapture

echo "All tests passed!"
```

---

## Expected Line Reduction

| Section | Current Lines | After Macros | Reduction |
|---------|--------------|--------------|-----------|
| Default functions | 78 | 25 (macro calls) | ~53 lines |
| Default impls | 61 | 6 (macro calls) | ~55 lines |
| merge_from_env | 125 | 24 (macro calls) | ~60 lines |
| **Total** | **264** | **~55** | **~168 lines** |

**Final file size estimate: ~350 lines (down from ~515 lines)**

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Macro compilation errors | High | Test incrementally, start with one macro |
| Serde compatibility issues | Medium | Keep `#[serde(default)]` on structs, test deserialization |
| Error message clarity | Low | Ensure env_var! preserves original error messages |
| Future maintainability | Low | Document macros clearly, add examples |

---

## Success Criteria

- [ ] All existing tests pass without modification
- [ ] File size reduced by ~168 lines
- [ ] No functionality changes (pure refactoring)
- [ ] Compilation warnings eliminated
- [ ] Code is more maintainable and DRY

---

## Post-Migration Notes

After completing this migration:
1. Update any documentation referencing the old function names
2. Consider adding the macros to a shared `src/macros.rs` module if they'll be used elsewhere
3. Add new config fields using the macro patterns going forward

---

## References

- Original file: `src/config/mod.rs` (515 lines)
- Rust macro documentation: https://doc.rust-lang.org/book/ch19-06-macros.html
- Serde default attributes: https://serde.rs/attr-default.html
