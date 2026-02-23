# Public API Audit - ARCH-001

## Current Module Structure

### Root Level (src/lib.rs)
All modules are public:
- `pub mod api`
- `pub mod cache`
- `pub mod config`
- `pub mod fs`
- `pub mod metrics`
- `pub mod sharded_counter`
- `pub mod types`

### api/ Module
- `pub mod client` - HTTP client for rqbit
- `pub mod streaming` - Stream management
- `pub mod types` - API types

**Re-exports:**
- `create_api_client`
- `PersistentStreamManager`
- `StreamManagerStats`
- `types::*`

### fs/ Module
- `pub mod async_bridge` - Async FUSE worker
- `pub mod error` - Error types
- `pub mod filesystem` - Main filesystem implementation
- `pub mod inode` - Inode management
- `pub mod macros` - FUSE macros

**Re-exports:**
- `fuse_error`, `fuse_log`, `fuse_ok` macros

### types/ Module
- `pub mod attr` - File attributes
- `pub mod handle` - File handles
- `pub mod inode` - Inode types

### cache/ Module
Already implemented with proper encapsulation (privateinternals).

### metrics/ Module
Single file - public interface only.

## Analysis

### What Should Be Public (Library API)
Based on current usage in lib.rs re-exports:

1. **Core types needed by consumers:**
   - `cache::{Cache, CacheStats}` - Already properly encapsulated
   - `config::{CliArgs, Config}` - Public configuration
   - `fs::filesystem::TorrentFS` - Main filesystem type
   - `fs::async_bridge::AsyncFuseWorker` - Required for mounting
   - `metrics::Metrics` - Public metrics interface
   - `sharded_counter::ShardedCounter` - Public counter utility

2. **Supporting types needed:**
   - `api::client::create_api_client` - Factory function
   - `api::types::*` - TorrentInfo, TorrentSummary
   - `types::attr::FileAttr` - File attributes
   - `types::handle::FileHandle` - File handle type
   - `types::inode::InodeEntry` - Inode entry type
   - `fs::error::FuseError` - Error types

### What Can Be Private (Internal)

1. **api/streaming** - Internal stream management, not needed externally
2. **fs/macros** - Internal FUSE macros
3. **fs/inode** - Could be encapsulated behind TorrentFS
4. **fs/async_bridge** - Already re-exported as needed
5. **fs/error** - Only FuseError needed externally

## Recommendations

### Option A: Minimal Public API
Keep only what lib.rs already re-exports as public. All other module internals private.

**Changes needed:**
1. Change internal module visibility from `pub mod X` to `mod X` where not needed
2. Add specific re-exports for needed types
3. Keep modules public only if they need to be accessed externally

### Option B: Current Structure (No Change)
Keep all modules public for flexibility. This is simpler but exposes more than necessary.

## Decision

**Recommended: Option A** - Minimal Public API

This provides:
- Clear separation between public and internal APIs
- Better encapsulation
- Easier future refactoring
- Less surface area for breaking changes

However, this is a **breaking change** for anyone currently importing from internal modules. Should be documented as such.

## Implementation Plan

1. Keep root modules public (api, cache, config, fs, metrics, sharded_counter, types)
2. Make sub-modules private where possible
3. Add specific type re-exports to maintain API compatibility
4. Update lib.rs re-exports with explicit types
5. Document breaking changes

## Impact Assessment

### Types currently used from internal modules (would need re-exports):
- `api::types::TorrentInfo` - Used in tests
- `api::types::TorrentSummary` - Used in client.rs
- `api::streaming::StreamManagerStats` - Public re-export already
- `fs::inode::InodeEntry` - Used in tests
- `fs::inode::InodeManager` - Used in filesystem.rs
- `types::inode::Inode` - Used throughout
- `types::attr::FileAttr` - Used throughout

All these are already being used, so we'd need to add explicit re-exports.
