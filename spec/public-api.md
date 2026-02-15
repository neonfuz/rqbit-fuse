# Public API Design Specification

## Overview

This document specifies the public API design for `rqbit-fuse`, addressing module visibility concerns and establishing clear boundaries between public and internal APIs.

## Current Issues

### ARCH-001: Audit Module Visibility

**Problem Statement:**
The current codebase exposes too many internal implementation details through public module declarations, creating an inconsistent and overly broad API surface.

**Current Issues:**

1. **Excessive pub declarations (64 total):**
   - All top-level modules are public: `api`, `cache`, `config`, `fs`, `metrics`, `types`
   - All sub-modules are public: `api::client`, `api::streaming`, `api::types`, `fs::filesystem`, `fs::inode`, `types::attr`, `types::file`, `types::inode`, `types::torrent`
   - Internal implementation types exposed: `CircuitBreaker`, `CircuitState`, `InodeManager`

2. **Internal modules exposed:**
   - `fs::inode` - inode management is internal implementation detail
   - `api::client::CircuitBreaker` - circuit breaker is internal retry mechanism
   - `types::attr` - attribute helpers are internal to FUSE implementation
   - `types::torrent` - appears to be dead code (see TYPES-001)

3. **Inconsistent API surface:**
   - `TorrentFS` accessed via `fs::filesystem::TorrentFS` (deep nesting)
   - No convenience re-exports at module level
   - Mixed patterns: some types exported via `pub use`, others only via module path
   - `types::*` wildcard export includes everything (no filtering)

4. **Dead code exposed:**
   - `types::torrent::Torrent` - appears unused (see TYPES-001 research)
   - `TorrentSummary` and `FileStats` - marked for removal (TYPES-003)

### ARCH-002: Implement Module Re-exports

**Problem Statement:**
Users must navigate deep module hierarchies to access commonly used types, resulting in verbose and confusing import paths.

**Current Pain Points:**

```rust
// Current: Verbose and inconsistent imports
use rqbit_fuse::fs::filesystem::TorrentFS;
use rqbit_fuse::api::client::RqbitClient;
use rqbit_fuse::api::types::TorrentInfo;
use rqbit_fuse::config::Config;
```

**Target Experience:**

```rust
// Target: Clean, intuitive imports
use rqbit_fuse::{TorrentFS, Client, Config, TorrentInfo};
```

## Public API Design

### Design Principles

1. **Minimal Surface:** Only expose what external users need
2. **Ergonomic Paths:** Common types at crate root, organized by domain
3. **Clear Boundaries:** Separate public API from implementation details
4. **Stability:** Public API changes require semver bumps
5. **Documentation:** Every public item has comprehensive docs

### Public vs Private Decision Matrix

| Item | Current | Target | Rationale |
|------|---------|--------|-----------|
| `TorrentFS` | `fs::filesystem::TorrentFS` | `fs::TorrentFS` + crate root | Primary API - main filesystem type |
| `RqbitClient` | `api::client::RqbitClient` | `api::Client` | Primary API - HTTP client |
| `Config` | `config::Config` | crate root | Essential configuration |
| `CliArgs` | `config::CliArgs` | `config::CliArgs` | CLI-specific, keep in config |
| `Cache` | `cache::Cache` | `cache::Cache` | Advanced users may customize |
| `CacheStats` | `cache::CacheStats` | `cache::CacheStats` | Metrics access |
| `Metrics` | `metrics::Metrics` | `metrics::Metrics` | Observability |
| `TorrentInfo` | `api::types::TorrentInfo` | `api::TorrentInfo` | Core data type |
| `TorrentState` | `api::types::TorrentState` | `api::TorrentState` | Core data type |
| `InodeManager` | `fs::inode::InodeManager` | private | Internal implementation |
| `CircuitBreaker` | `api::client::CircuitBreaker` | private | Internal retry mechanism |
| `attr` module | `types::attr` | private | Internal FUSE helpers |
| `torrent` module | `types::torrent` | remove | Dead code (TYPES-003) |

### Module Re-export Strategy

#### lib.rs - Crate Root Exports

```rust
// Primary API - most common types
pub use config::Config;
pub use fs::TorrentFS;
pub use api::Client;

// Secondary API - advanced usage  
pub use cache::{Cache, CacheStats};
pub use metrics::Metrics;

// Data types
pub use api::types::{TorrentInfo, TorrentState, FileInfo, TorrentStats, LiveStats};
pub use types::{InodeEntry, TorrentFile};

// Error types
pub use api::types::ApiError;
pub use config::ConfigError;

// Internal modules - NOT PUBLIC
mod internal {
    // These modules contain implementation details
    // and should not be accessed by external users
}
```

#### api/mod.rs - Clean Client API

```rust
// Make internal modules private
mod client;
mod streaming;
mod types;

// Re-export primary types
pub use client::RqbitClient as Client;
pub use streaming::{PersistentStreamManager, StreamManagerStats};

// Selective type exports (not wildcard)
pub use types::{
    ApiError,
    TorrentInfo,
    TorrentState,
    TorrentStats,
    TorrentStatus,
    LiveStats,
    FileInfo,
    DownloadSpeed,
    UploadSpeed,
    PieceBitfield,
    AddTorrentResponse,
    AddMagnetRequest,
    AddTorrentUrlRequest,
};

// Remove: CircuitBreaker, CircuitState (internal)
// Remove: TorrentSummary, FileStats (dead code - TYPES-003)
```

#### fs/mod.rs - Filesystem Module

```rust
// Make internal modules private
mod filesystem;
mod inode;

// Re-export public types
pub use filesystem::TorrentFS;

// InodeEntry comes from types::inode, not fs::inode
// fs::inode::InodeManager stays private
```

#### types/mod.rs - Type Definitions

```rust
// Keep public - these are data types users work with
pub mod file;
pub mod inode;

// Make private - internal helpers
mod attr;  // FUSE attribute helpers

// Remove: torrent module (dead code)

// Re-export common types
pub use file::TorrentFile;
pub use inode::InodeEntry;
```

#### cache/mod.rs - Cache Module

No changes needed - already well-structured:

```rust
pub struct Cache;  // Already public
pub struct CacheStats;  // Already public

// Internal implementation details private by default
```

#### config/mod.rs - Configuration Module

Minor adjustment - keep CLI args separate:

```rust
pub struct Config;  // Keep public
pub struct ConfigError;  // Keep public

// CLI-specific, not part of library API
pub struct CliArgs;  // Keep but document as CLI-only

// Internal config structs - consider making fields private
// or providing builder pattern
```

### API Stability Considerations

#### Stability Levels

1. **Stable (1.x.x):** Core types that won't change
   - `TorrentFS` - main filesystem interface
   - `Config` - configuration structure
   - `Client` - HTTP client interface
   - `TorrentInfo` - core data type

2. **Unstable (0.x.x):** May change in minor versions
   - `Cache` - implementation may evolve
   - `Metrics` - observability interface evolving
   - `PersistentStreamManager` - streaming internals

3. **Internal:** Not part of public API
   - `InodeManager` - implementation detail
   - `CircuitBreaker` - internal retry logic
   - `attr` helpers - FUSE internals

#### SemVer Policy

- **Major:** Breaking changes to stable API
- **Minor:** New features, unstable API changes
- **Patch:** Bug fixes, docs, internal refactoring

## Module Organization

### Target Structure

```
rqbit-fuse/
├── lib.rs              # Crate root with selective re-exports
├── api/
│   ├── mod.rs          # Re-exports: Client, types
│   ├── client.rs       # RqbitClient (private)
│   ├── streaming.rs    # PersistentStreamManager (public)
│   └── types.rs        # API types (selective re-export)
├── cache/
│   └── mod.rs          # Cache, CacheStats (public)
├── config/
│   └── mod.rs          # Config, CliArgs, ConfigError (public)
├── fs/
│   ├── mod.rs          # Re-exports: TorrentFS
│   ├── filesystem.rs   # TorrentFS impl (private)
│   └── inode.rs        # InodeManager (private)
├── metrics/
│   └── mod.rs          # Metrics (public)
├── types/
│   ├── mod.rs          # Re-exports: TorrentFile, InodeEntry
│   ├── file.rs         # TorrentFile (public)
│   ├── inode.rs        # InodeEntry (public)
│   └── attr.rs         # FUSE helpers (private)
└── main.rs             # CLI entry point
```

### Import Path Examples

**Before:**
```rust
use rqbit_fuse::fs::filesystem::TorrentFS;
use rqbit_fuse::api::client::RqbitClient;
use rqbit_fuse::api::types::{TorrentInfo, TorrentState};
use rqbit_fuse::config::Config;
use rqbit_fuse::cache::{Cache, CacheStats};
```

**After:**
```rust
// Option 1: Crate root imports (recommended)
use rqbit_fuse::{TorrentFS, Client, Config, TorrentInfo, TorrentState};

// Option 2: Module-specific imports
use rqbit_fuse::fs::TorrentFS;
use rqbit_fuse::api::{Client, TorrentInfo};
use rqbit_fuse::config::Config;
use rqbit_fuse::cache::Cache;

// Option 3: Full paths for clarity
use rqbit_fuse::api::types::{TorrentInfo, TorrentState};
```

## Refactoring Plan

### Phase 1: Audit and Inventory

1. **Generate pub inventory:**
   ```bash
   grep -r "^pub " src/ --include="*.rs" | wc -l
   grep -r "^pub mod" src/ --include="*.rs"
   grep -r "^pub use" src/ --include="*.rs"
   ```

2. **Categorize each pub item:**
   - Public API (external users need this)
   - Internal but exposed (implementation detail)
   - Dead code (nothing uses it)

3. **Document dependencies:**
   - Which external imports use which paths?
   - What would break if we made X private?

### Phase 2: Make Internal Modules Private

**Priority order (least impact first):**

1. **types/attr.rs** - Only used internally
   - Change `pub mod attr` to `mod attr` in types/mod.rs
   - Update any cross-module usage

2. **types/torrent.rs** - Dead code
   - Remove module entirely (coordinate with TYPES-003)

3. **fs/inode.rs** - Internal implementation
   - Change `pub mod inode` to `mod inode` in fs/mod.rs
   - Ensure InodeManager stays private
   - InodeEntry remains public via types::inode

4. **api/client.rs** - Circuit breaker internal
   - Keep `RqbitClient` public
   - Make `CircuitBreaker` and `CircuitState` private
   - Add `pub use client::RqbitClient as Client` in api/mod.rs

### Phase 3: Add Convenience Re-exports

1. **lib.rs re-exports:**
   ```rust
   pub use fs::TorrentFS;
   pub use api::Client;
   pub use config::Config;
   ```

2. **api/mod.rs re-exports:**
   ```rust
   pub use client::RqbitClient as Client;
   pub use types::{TorrentInfo, TorrentState, /* ... */};
   ```

3. **fs/mod.rs re-exports:**
   ```rust
   pub use filesystem::TorrentFS;
   ```

4. **types/mod.rs re-exports:**
   ```rust
   pub use file::TorrentFile;
   pub use inode::InodeEntry;
   ```

### Phase 4: Update All Imports

**Files to update:**

1. **src/main.rs** - Update CLI imports
2. **src/lib.rs** - Update re-exports
3. **src/fs/filesystem.rs** - Update internal imports
4. **src/api/client.rs** - Update type imports
5. **src/api/streaming.rs** - Update type imports
6. **tests/** - Update all test imports

**Migration strategy:**

```rust
// Old (keep for backward compat during transition)
#[deprecated(since = "0.2.0", note = "Use fs::TorrentFS instead")]
pub use fs::filesystem::TorrentFS;

// New
pub use fs::TorrentFS;
```

### Phase 5: Verification

1. **Check compilation:**
   ```bash
   cargo check
   cargo build
   ```

2. **Run tests:**
   ```bash
   cargo test
   ```

3. **Verify public API:**
   ```bash
   cargo doc --no-deps --open
   # Review what appears in documentation
   ```

4. **Check for unused pub items:**
   ```bash
   cargo +nightly rustdoc -- -D rustdoc::missing_docs
   ```

## Documentation Requirements

### Public API Documentation Standards

Every public item must have:

1. **Doc comment** explaining purpose and usage
2. **Example code** showing typical usage
3. **Error conditions** documented
4. **Panics** section if applicable

**Example:**

```rust
/// A FUSE filesystem backed by torrent data from an rqbit server.
///
/// This is the primary interface for mounting torrents as a filesystem.
/// It handles all FUSE callbacks, manages inodes, and coordinates
/// with the rqbit API for data retrieval.
///
/// # Example
///
/// ```no_run
/// use rqbit_fuse::{TorrentFS, Config};
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let metrics = Arc::new(rqbit_fuse::Metrics::new());
/// let fs = TorrentFS::new(config, metrics)?;
/// fs.mount().await?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The mount point doesn't exist or isn't accessible
/// - The rqbit server is unreachable
/// - FUSE initialization fails
///
/// # Panics
///
/// Panics if called from a thread that is not a Tokio runtime thread.
pub struct TorrentFS { /* ... */ }
```

### Crate-Level Documentation (lib.rs)

```rust
//! # rqbit-fuse
//!
//! Mount torrents from an [rqbit](https://github.com/ikatson/rqbit) server as a FUSE filesystem.
//!
//! ## Quick Start
//!
//! ```no_run
//! use rqbit_fuse::{TorrentFS, Config};
//!
//! # async fn quickstart() -> anyhow::Result<()> {
//! let config = Config::default();
//! let fs = TorrentFS::new(config)?;
//! fs.mount().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - **Streaming reads**: Read torrent data on-demand without full download
//! - **Intelligent caching**: LRU cache with TTL for metadata and pieces
//! - **Resilient**: Circuit breaker pattern for API resilience
//! - **Observable**: Built-in metrics and logging
//!
//! ## Modules
//!
//! - `api`: HTTP client for rqbit server
//! - `cache`: Caching layer for torrent data
//! - `config`: Configuration management
//! - `fs`: FUSE filesystem implementation
//! - `metrics`: Performance and health metrics
//! - `types`: Core data types
//!
//! ## Stability
//!
//! This crate follows Semantic Versioning. The public API is marked as
//! stable once we reach 1.0.0. Until then, minor versions may include
//! breaking changes to unstable APIs.
```

### Examples Directory

Create `examples/` with:

1. **basic_mount.rs** - Simple filesystem mount
2. **custom_cache.rs** - Custom cache configuration
3. **api_client.rs** - Using the API client directly
4. **metrics.rs** - Accessing metrics programmatically

### Stability Guarantees

**Document in README.md and lib.rs:**

```markdown
## API Stability

### Stable API (1.0.0+)
These types will not change in breaking ways:
- `TorrentFS` - Main filesystem interface
- `Config` - Configuration structure  
- `Client` - HTTP client interface
- `TorrentInfo`, `TorrentState` - Core data types

### Evolving API (0.x.x)
These may change in minor versions:
- `Cache` - Cache configuration and access
- `Metrics` - Metrics interface
- `PersistentStreamManager` - Streaming internals

### Internal API
These are implementation details and not covered by semver:
- `InodeManager` - Inode table management
- `CircuitBreaker` - Retry logic
- Attribute helpers in `types::attr`
```

## Export Structure

### Final lib.rs Structure

```rust
// Primary API - Most common types at crate root
pub use config::Config;
pub use fs::TorrentFS;
pub use api::Client;

// Secondary API - Advanced usage
pub mod api;
pub mod cache;
pub mod config;
pub mod fs;
pub mod metrics;
pub mod types;

// Re-exports for convenience
pub use api::types::{
    TorrentInfo,
    TorrentState,
    TorrentStats,
    FileInfo,
};
pub use types::{InodeEntry, TorrentFile};

// Internal modules - private implementation
// mod internal { ... }

// Main entry point for the library
pub async fn run(config: Config) -> anyhow::Result<()> { ... }
```

### Module Re-exports Summary

| Module | Public Types | Re-export Location |
|--------|-------------|-------------------|
| `api::client` | `RqbitClient` | `api::Client` |
| `api::streaming` | `PersistentStreamManager`, `StreamManagerStats` | `api::` |
| `api::types` | `TorrentInfo`, `TorrentState`, etc. | `api::`, `crate::` |
| `fs::filesystem` | `TorrentFS` | `fs::`, `crate::` |
| `types::file` | `TorrentFile` | `types::`, `crate::` |
| `types::inode` | `InodeEntry` | `types::`, `crate::` |

### Feature Flags (Future Considerations)

If needed in the future:

```rust
[features]
default = ["fuse"]
fuse = ["fuser"]                    # FUSE filesystem support
api-client = ["reqwest"]            # Standalone API client only
metrics = []                        # Metrics collection
tracing = ["tracing-subscriber"]    # Enhanced logging
```

Current: No feature flags needed - all features are core functionality.

## Implementation Checklist

- [ ] Audit all `pub` declarations (64 items)
- [ ] Categorize: public API / internal / dead code
- [ ] Create `fs::TorrentFS` re-export
- [ ] Create `api::Client` re-export
- [ ] Make `fs::inode` module private
- [ ] Make `api::client` module private (keep Client public)
- [ ] Make `types::attr` module private
- [ ] Remove `types::torrent` module (coordinate TYPES-003)
- [ ] Remove wildcard export `api::types::*`
- [ ] Add selective type re-exports in `api/mod.rs`
- [ ] Update all imports in `src/` files
- [ ] Update all imports in `tests/` files
- [ ] Add comprehensive doc comments
- [ ] Create crate-level documentation
- [ ] Write examples in `examples/` directory
- [ ] Verify `cargo doc` output
- [ ] Run `cargo test` to verify
- [ ] Update CHANGELOG with breaking changes
- [ ] Update README with new import paths

## Breaking Changes

This refactoring includes breaking changes requiring a minor version bump:

### Removed (Internal → Private)
- `rqbit_fuse::fs::inode` - Use fs module APIs instead
- `rqbit_fuse::api::client::CircuitBreaker` - Internal implementation
- `rqbit_fuse::api::client::CircuitState` - Internal implementation
- `rqbit_fuse::types::attr` - Internal helpers
- `rqbit_fuse::types::torrent` - Dead code

### Changed (Path Changes)
- `rqbit_fuse::fs::filesystem::TorrentFS` → `rqbit_fuse::fs::TorrentFS` or `rqbit_fuse::TorrentFS`
- `rqbit_fuse::api::client::RqbitClient` → `rqbit_fuse::api::Client` or `rqbit_fuse::Client`

### Migration Guide

```rust
// Before
use rqbit_fuse::fs::filesystem::TorrentFS;
use rqbit_fuse::api::client::RqbitClient;

// After
use rqbit_fuse::{TorrentFS, Client};
// or
use rqbit_fuse::fs::TorrentFS;
use rqbit_fuse::api::Client;
```

---

*This specification addresses ARCH-001 and ARCH-002 from TODO.md*
