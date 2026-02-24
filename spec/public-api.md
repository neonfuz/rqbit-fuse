# Public API Specification

**Status:** Current as of codebase review (February 2026)

## Overview

This document specifies the actual public API for `rqbit-fuse` as currently implemented. It documents the existing module structure, public exports, and identifies areas where the implementation differs from the original design.

## Current Public API

### Crate Root Exports (lib.rs)

The following modules and types are publicly exported from the crate root:

```rust
// Public modules
pub mod api;
pub mod config;
pub mod error;
pub mod fs;
pub mod metrics;
pub mod mount;
pub mod types;

// Re-exports
pub use config::{CliArgs, Config};
pub use fs::async_bridge::AsyncFuseWorker;
pub use fs::filesystem::TorrentFS;
pub use metrics::Metrics;

// Main entry point
pub async fn run(config: Config) -> Result<()>;
```

### Module Structure

#### `api` Module

**File:** `src/api/mod.rs`

**Public Submodules:**
- `pub mod client` - HTTP client implementation
- `pub mod streaming` - Persistent streaming manager
- `pub mod types` - API data types

**Re-exports:**
```rust
pub use client::create_api_client;
pub use streaming::{PersistentStreamManager, StreamManagerStats};
pub use types::{ListTorrentsResult, TorrentInfo, TorrentSummary};
pub use crate::error::RqbitFuseError as ApiError;
```

**Key Types:**
- `RqbitClient` (in `client` module) - HTTP client for rqbit API
- `PersistentStreamManager` - Manages persistent HTTP streams for file reads
- `StreamManagerStats` - Statistics for stream manager
- `TorrentInfo` - Full torrent information
- `TorrentSummary` - Brief torrent summary for lists
- `ListTorrentsResult` - Result type for torrent listing with partial failure support

#### `config` Module

**File:** `src/config/mod.rs`

**Public Types:**
- `Config` - Main configuration struct
- `CliArgs` - Command-line arguments
- `ApiConfig` - API connection settings
- `CacheConfig` - Cache configuration (note: cache module not implemented)
- `MountConfig` - FUSE mount options
- `PerformanceConfig` - Performance tuning options
- `LoggingConfig` - Logging configuration

**Methods on Config:**
- `Config::new()` - Create default config
- `Config::from_file(path)` - Load from file (JSON or TOML)
- `Config::from_default_locations()` - Load from standard config paths
- `Config::merge_from_env()` - Override from environment variables
- `Config::merge_from_cli(cli)` - Override from CLI args
- `Config::load()` - Load with env merge
- `Config::load_with_cli(cli)` - Load with env and CLI merge
- `Config::validate()` - Validate configuration

#### `error` Module

**File:** `src/error.rs`

**Public Types:**
- `RqbitFuseError` - Main error enum with 11 variants:
  - `NotFound(String)` - ENOENT equivalent
  - `PermissionDenied(String)` - EACCES equivalent
  - `TimedOut(String)` - ETIMEDOUT equivalent
  - `NetworkError(String)` - Network failures
  - `ApiError { status, message }` - HTTP API errors
  - `IoError(String)` - I/O errors
  - `InvalidArgument(String)` - EINVAL equivalent
  - `ValidationError(Vec<ValidationIssue>)` - Config validation errors
  - `NotReady(String)` - EAGAIN equivalent
  - `ParseError(String)` - Parsing failures
  - `IsDirectory` - EISDIR equivalent
  - `NotDirectory` - ENOTDIR equivalent

- `ValidationIssue` - Single validation error with field and message
- `RqbitFuseResult<T>` - Type alias for `Result<T, RqbitFuseError>`
- `ToFuseError` trait - Convert errors to FUSE error codes

**Methods on RqbitFuseError:**
- `to_errno(&self) -> i32` - Convert to libc error code
- `is_transient(&self) -> bool` - Check if error is retryable
- `is_server_unavailable(&self) -> bool` - Check if server is unreachable

#### `fs` Module

**File:** `src/fs/mod.rs`

**Public Submodules:**
- `pub mod async_bridge` - Async bridge for FUSE callbacks
- `pub mod filesystem` - Main filesystem implementation
- `pub mod inode` - Inode management (backward compatibility)
- `pub mod inode_entry` - Inode entry definitions
- `pub mod inode_manager` - Inode table management

**Re-exports:**
```rust
pub use crate::error::{RqbitFuseError, RqbitFuseResult};
pub use async_bridge::AsyncFuseWorker;
pub use filesystem::TorrentFS;
pub use inode_entry::InodeEntry;
pub use inode_manager::{InodeEntryRef, InodeManager};
```

**Key Types:**
- `TorrentFS` - Main FUSE filesystem implementation
- `AsyncFuseWorker` - Worker for handling async FUSE operations
- `InodeManager` - Manages inode table (currently public but should be internal)
- `InodeEntry` - Represents a filesystem entry
- `InodeEntryRef` - Reference to an inode entry

#### `metrics` Module

**File:** `src/metrics.rs` (single file, not a directory)

**Public Types:**
- `Metrics` - Metrics collection with atomic counters

**Methods on Metrics:**
- `Metrics::new()` - Create new metrics instance
- `record_read(bytes)` - Record bytes read
- `record_error()` - Record an error
- `record_cache_hit()` - Record cache hit
- `record_cache_miss()` - Record cache miss
- `log_summary()` - Log metrics summary

**Fields (all public AtomicU64):**
- `bytes_read`
- `error_count`
- `cache_hits`
- `cache_misses`

#### `mount` Module

**File:** `src/mount.rs` (single file, not a directory)

**Public Functions:**
- `setup_logging(verbose, quiet) -> Result<()>` - Initialize tracing subscriber
- `try_unmount(path, force) -> Result<()>` - Unmount FUSE filesystem
- `is_mount_point(path) -> Result<bool>` - Check if path is a mount point
- `unmount_filesystem(path, force) -> Result<()>` - Unmount wrapper
- `run_command(program, args, context) -> Result<Output>` - Run external command

#### `types` Module

**File:** `src/types/mod.rs`

**Public Submodules:**
- `pub mod attr` - FUSE attribute helpers
- `pub mod handle` - File handle management

**Re-exports:**
```rust
pub use crate::fs::inode::InodeEntry;
pub use fuser::FileAttr;
pub use handle::FileHandle;
```

**Key Types:**
- `InodeEntry` - Filesystem entry (re-exported from fs::inode)
- `FileAttr` - FUSE file attributes (from fuser crate)
- `FileHandle` - File handle for open files
- `FileHandleManager` - Manages file handles (in handle submodule)

**Note:** The `attr` module contains FUSE attribute helpers that are implementation details.

## Discrepancies from Original Design

### 1. Missing `cache` Module

**Expected:** `src/cache/mod.rs` with `Cache` and `CacheStats` types

**Actual:** Cache module does not exist. The `metrics` module exists and tracks cache hits/misses, but no actual caching implementation is present.

**Impact:** The `CacheConfig` in `config` module configures a non-existent cache.

### 2. Excessive Public Modules

Several modules that should be internal implementation details are currently public:

| Module | Current Visibility | Should Be | Reason |
|--------|-------------------|-----------|---------|
| `api::client` | `pub mod` | `mod` | Contains internal client implementation |
| `fs::inode` | `pub mod` | `mod` | Internal inode management |
| `fs::inode_entry` | `pub mod` | `mod` | Internal inode implementation |
| `fs::inode_manager` | `pub mod` | `mod` | Internal inode table management |
| `types::attr` | `pub mod` | `mod` | FUSE attribute helpers |
| `types::handle` | `pub mod` | `mod` | File handle management |

### 3. Missing Re-exports

The original design planned for convenience re-exports that don't exist:

- `api::Client` should be a re-export of `api::client::RqbitClient`
- `fs::TorrentFS` should be a re-export (exists, but path is direct)
- `api::TorrentInfo` should be a re-export from `api::types`

### 4. Additional Public Items Not in Original Design

The following items are public but were not in the original design:

- `error` module - Unified error types (good addition)
- `mount` module - Mount utilities (good addition)
- `CliArgs` re-exported at crate root (convenient for CLI users)
- `AsyncFuseWorker` re-exported at crate root
- `InodeManager` and `InodeEntryRef` are public (should be internal)

### 5. Module Structure Differences

**Expected (from design):**
```
metrics/
  └── mod.rs
```

**Actual:**
```
metrics.rs (single file)
```

Same for `mount.rs` and `error.rs`.

## Current API Usage Examples

### Basic Library Usage

```rust
use rqbit_fuse::{Config, TorrentFS, Metrics, run};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    run(config).await
}
```

### Using the API Client Directly

```rust
use rqbit_fuse::api::client::RqbitClient;
use rqbit_fuse::config::ApiConfig;

async fn use_client() -> anyhow::Result<()> {
    let client = RqbitClient::new("http://localhost:3030".to_string())?;
    let result = client.list_torrents().await?;
    Ok(())
}
```

### Accessing Metrics

```rust
use rqbit_fuse::metrics::Metrics;
use std::sync::Arc;

fn setup_metrics() -> Arc<Metrics> {
    Arc::new(Metrics::new())
}
```

### Configuration

```rust
use rqbit_fuse::config::Config;

fn configure() -> anyhow::Result<()> {
    let config = Config::from_default_locations()?
        .merge_from_env()?;
    config.validate()?;
    Ok(())
}
```

## Recommended Refactoring

### Priority 1: Make Internal Modules Private

The following should be changed from `pub mod` to `mod`:

1. **In `src/api/mod.rs`:**
   ```rust
   mod client;  // Change from pub mod
   pub use client::RqbitClient;  // Export just the client
   ```

2. **In `src/fs/mod.rs`:**
   ```rust
   mod inode;
   mod inode_entry;
   mod inode_manager;
   // Keep InodeEntry public, make managers private
   pub use inode_entry::InodeEntry;
   ```

3. **In `src/types/mod.rs`:**
   ```rust
   mod attr;  // Internal FUSE helpers
   mod handle;  // File handle management
   ```

### Priority 2: Add Convenience Re-exports

1. **In `src/api/mod.rs`:**
   ```rust
   pub use client::RqbitClient as Client;
   pub use types::{TorrentInfo, TorrentState, TorrentStats, FileInfo};
   ```

2. **In `src/lib.rs` (optional):**
   ```rust
   pub use api::{Client, TorrentInfo};
   ```

### Priority 3: Implement or Remove Cache

Either:
- Implement the `cache` module with `Cache` and `CacheStats` types
- Or remove `CacheConfig` from config module if caching is handled elsewhere

### Priority 4: Document Stability Levels

Add documentation to each public item indicating its stability:

```rust
/// Main FUSE filesystem implementation.
/// 
/// # Stability
/// This type is considered stable and will not change in breaking ways
/// without a major version bump.
pub struct TorrentFS { ... }
```

## Public API Inventory

### Total Public Items

```bash
$ grep -r "^pub " src/ --include="*.rs" | wc -l
# Approximately 80+ public items

$ grep -r "^pub mod" src/ --include="*.rs"
# 11 public modules

$ grep -r "^pub use" src/ --include="*.rs"
# 12 public re-exports
```

### Public Modules (11)

1. `api`
2. `api::client`
3. `api::streaming`
4. `api::types`
5. `config`
6. `error`
7. `fs`
8. `fs::async_bridge`
9. `fs::filesystem`
10. `fs::inode`
11. `fs::inode_entry`
12. `fs::inode_manager`
13. `metrics`
14. `mount`
15. `types`
16. `types::attr`
17. `types::handle`

### Core Public Types

**Must Be Public:**
- `TorrentFS` - Main filesystem
- `Config` - Configuration
- `RqbitClient` - API client
- `Metrics` - Metrics collection
- `RqbitFuseError` - Error handling
- `TorrentInfo` - Data type
- `FileInfo` - Data type
- `InodeEntry` - Filesystem entries

**Should Be Internal:**
- `InodeManager` - Implementation detail
- `InodeEntryRef` - Implementation detail
- `FileHandleManager` - Implementation detail
- `AsyncFuseWorker` - Could be internal
- `PersistentStreamManager` - Implementation detail

## Verification Commands

```bash
# Check public API
cargo doc --no-deps

# Check for unused public items
cargo +nightly rustdoc -- -D rustdoc::missing_docs

# Count public items
grep -r "^pub " src/ --include="*.rs" | wc -l
grep -r "^pub mod" src/ --include="*.rs"
grep -r "^pub use" src/ --include="*.rs"

# List all public modules
grep -r "^pub mod" src/ --include="*.rs" | sed 's/.*pub mod \([^;]*\).*/\1/'
```

---

*Last Updated: February 2026*
*This specification documents the actual public API as implemented in the codebase.*
