# File Splitting Strategy for SIMPLIFY-002

**Date:** February 22, 2026  
**Related:** SIMPLIFY-002 in TODO.md

## Current State

### File Sizes
- `src/fs/filesystem.rs`: **1,434 lines** (5 `impl` blocks: TorrentFS, Filesystem trait, helpers, tests)
- `src/fs/inode.rs`: **1,205 lines** (5 `impl` blocks: InodeEntry, InodeManager, serialization, tests)

### Current Structure
```
src/fs/
├── filesystem.rs     # 1,434 lines - Too large
├── inode.rs          # 1,205 lines - Too large
└── mod.rs
```

## Proposed Structure

### For `src/fs/filesystem.rs`:

**Split into 3 files:**

1. **`fs/torrent_fs.rs`** (~600 lines)
   - Core `TorrentFS` struct definition
   - Constructor (`new()`)
   - Configuration accessors (`config()`, `mount_point()`)
   - Helper methods (not FUSE callbacks)
   - Internal state management

2. **`fs/fuse_callbacks.rs`** (~500 lines)
   - `impl Filesystem for TorrentFS`
   - All FUSE callback implementations:
     - `lookup()`, `readdir()`, `read()`, `getattr()`
     - `open()`, `release()`, `statfs()`, `access()`
   - Error mapping to FUSE errno codes

3. **`fs/discovery.rs`** (~300 lines)
   - Background torrent discovery logic
   - `discover_existing_torrents()` function
   - Status monitoring task (`start_status_monitoring()`)
   - Cleanup task for file handles
   - Note: Monitoring merged here (too small for separate file)

### For `src/fs/inode.rs`:

**Split into 2 files:**

1. **`fs/inode_entry.rs`** (~400 lines)
   - `InodeEntry` enum definition (Directory, File, Symlink)
   - `Serialize`/`Deserialize` implementations
   - Helper methods on `InodeEntry`
   - Path utilities and validation

2. **`fs/inode_manager.rs`** (~800 lines)
   - `InodeManager` struct
   - Inode allocation methods
   - Path resolution and lookup
   - Directory tree management
   - Cleanup and removal logic
   - Torrent-to-inode mapping

3. **`tests/inode_manager_tests.rs`** (~200 lines)
   - Move inline tests from old `inode.rs`
   - Property-based tests
   - Concurrent stress tests

## Dependencies

```
torrent_fs.rs
    ↑
fuse_callbacks.rs (depends on torrent_fs.rs)
    ↑
discovery.rs (depends on both)

inode_entry.rs
    ↑
inode_manager.rs (depends on inode_entry.rs)
```

All modules depend on:
- `api/` (client, types)
- `types/` (handle, attr)
- `cache.rs`
- `metrics.rs`

## Public API Preservation

### Must remain public:
- `TorrentFS` struct in `torrent_fs.rs`
- `impl Filesystem for TorrentFS` (re-export in `fs/mod.rs`)
- `InodeEntry` enum in `inode_entry.rs`
- `InodeManager` struct in `inode_manager.rs`

### Module exports in `fs/mod.rs`:
```rust
pub mod torrent_fs;
pub mod fuse_callbacks;
pub mod discovery;
pub mod inode_entry;
pub mod inode_manager;

// Re-exports for backward compatibility
pub use torrent_fs::TorrentFS;
pub use inode_entry::InodeEntry;
pub use inode_manager::InodeManager;
```

## Test Organization

### Current tests to migrate:

**From `filesystem.rs`:**
- Keep FUSE operation tests in `tests/fuse_operations.rs` (already separate)
- Move any inline unit tests to appropriate new files

**From `inode.rs`:**
- Move all inline tests to `tests/inode_manager_tests.rs`
- Property-based tests (proptest)
- Concurrent stress tests
- Unit tests for allocation, lookup, removal

### New test files structure:
```
tests/
├── fuse_operations.rs      # Existing - keep as-is
├── integration_tests.rs    # Existing - keep as-is
├── inode_manager_tests.rs  # NEW - move from inline
└── common/
    └── mod.rs
```

## Migration Steps

### Phase 1: Preparation
1. Run full test suite: `cargo test`
2. Run clippy: `cargo clippy -- -D warnings`
3. Create backup branch: `git checkout -b simplify-002-file-split`

### Phase 2: Split filesystem.rs
1. Create `fs/torrent_fs.rs` with `pub(crate)` visibility
2. Move `TorrentFS` struct and constructor
3. Move helper methods (not FUSE callbacks)
4. Run `cargo test` - fix any import issues
5. Create `fs/fuse_callbacks.rs` with `pub(crate)` visibility
6. Move `impl Filesystem for TorrentFS`
7. Update imports in `fuse_callbacks.rs`
8. Run `cargo test` - fix any issues
9. Create `fs/discovery.rs`
10. Move discovery and monitoring functions
11. Run `cargo test` - verify all pass
12. Delete old `filesystem.rs`
13. Update `fs/mod.rs` with new module declarations

### Phase 3: Split inode.rs
1. Create `fs/inode_entry.rs` with `pub(crate)` visibility
2. Move `InodeEntry` enum and implementations
3. Run `cargo test` - fix imports
4. Create `fs/inode_manager.rs` with `pub(crate)` visibility
5. Move `InodeManager` struct and methods
6. Run `cargo test` - fix imports
7. Create `tests/inode_manager_tests.rs`
8. Move inline tests from old `inode.rs`
9. Run `cargo test` - verify all pass
10. Delete old `inode.rs`
11. Update `fs/mod.rs` with new module declarations

### Phase 4: Finalize
1. Change `pub(crate)` to `pub` for public APIs
2. Add re-exports to `fs/mod.rs`
3. Run full test suite: `cargo test`
4. Run clippy: `cargo clippy -- -D warnings`
5. Check documentation: `cargo doc`
6. Commit changes

## Why This Structure?

### vs Original TODO.md suggestion:

**Original:**
- `fs/operations.rs` - vague name
- `fs/monitor.rs` - too small (~100 lines)
- `inode/allocator.rs`, `lookup.rs`, `tree.rs` - too fragmented

**Revised:**
- `fs/fuse_callbacks.rs` - clear, descriptive name
- `fs/discovery.rs` - merged monitoring (appropriate size)
- `fs/inode_entry.rs` + `inode_manager.rs` - logical separation (data vs management)

### Benefits:

1. **Single Responsibility**: Each file has one clear purpose
2. **Reasonable Size**: All files < 1000 lines
3. **Logical Grouping**: Related functionality stays together
4. **Testability**: Can test components independently
5. **Maintainability**: Easier to find and modify code
6. **No Breaking Changes**: Public API preserved via re-exports

## Risk Mitigation

### Potential Issues:

1. **Circular Dependencies**
   - Mitigation: Check dependency graph before starting
   - Use `pub(crate)` visibility during migration

2. **Import Errors**
   - Mitigation: Fix imports incrementally after each file move
   - Use `cargo check` frequently

3. **Test Failures**
   - Mitigation: Run tests after each file move
   - Keep old files until all tests pass

4. **Documentation Gaps**
   - Mitigation: Ensure all public items have doc comments
   - Run `cargo doc` to verify

## Success Criteria

- [ ] All 209+ tests pass
- [ ] Zero clippy warnings
- [ ] Documentation builds without errors
- [ ] No breaking changes to public API
- [ ] All new files < 1000 lines
- [ ] Clean compilation: `cargo build --release`

## Future Considerations

If files grow again:
- `fuse_callbacks.rs` could split by callback type (read_ops.rs, dir_ops.rs)
- `inode_manager.rs` could extract path resolution to separate file
- Consider feature flags for optional functionality
