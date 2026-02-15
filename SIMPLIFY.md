# Code Simplification Checklist

## How to Use This File

Each item is designed to be completed independently. Migration guides are stored in `migration/` folder with corresponding names.

**Workflow:**
1. Pick an unchecked item
2. Read the migration guide (e.g., `[migration:SIMPLIFY-001]`)
3. Complete the task following the guide
4. Check the box
5. Commit your changes

**Dependencies:** Some tasks have dependencies noted - complete those first.

---

## Phase 1: High-Impact Simplifications (168-258 lines)

### Configuration (`src/config/mod.rs`)

- [x] **SIMPLIFY-001**: Add macros to reduce config boilerplate
  - [migration:SIMPLIFY-001-config-macros](migration/SIMPLIFY-001-config-macros.md)
  - Create `env_var!`, `default_fn!`, `default_impl!` macros
  - Replace 20 default functions, 6 Default impls, 20 env merge blocks
  - **Lines reduced**: ~168 lines (515 → 347)
  - **Risk**: Low - macros preserve exact behavior
  - **Test**: `cargo test config::tests` ✅ All tests pass

### API Client (`src/api/client.rs`)

- [x] **SIMPLIFY-002**: Unify torrent control methods
  - [migration:SIMPLIFY-002-torrent-control](migration/SIMPLIFY-002-torrent-control.md)
  - Create `torrent_action()` helper for pause/start/forget/delete
  - Replace 4 nearly identical methods (~72 lines → ~12 lines)
  - **Lines reduced**: ~60 lines
  - **Risk**: Low - same logic, just extracted
  - **Test**: `cargo test api::client::tests`

- [x] **SIMPLIFY-013**: Add tracing instrumentation
  - [migration:SIMPLIFY-013-tracing-instrument](migration/SIMPLIFY-013-tracing-instrument.md)
  - Add `#[instrument]` to 12 public methods
  - Remove manual `trace!`/`debug!` calls
  - **Lines reduced**: ~30 lines
  - **Risk**: Low - no behavior change
  - **Depends on**: None (can do in parallel with SIMPLIFY-002)
  - **Test**: `cargo test api::client::tests`, verify logs

- [x] **SIMPLIFY-014**: Create unified request helpers
  - [migration:SIMPLIFY-014-request-helpers](migration/SIMPLIFY-014-request-helpers.md)
  - Create `get_json<T>()` and `post_json<B, T>()` generics
  - Refactor `get_torrent`, `get_torrent_stats`, `add_torrent_*`
  - **Lines reduced**: ~25 lines
  - **Risk**: Medium - generic constraints need careful testing
  - **Depends on**: SIMPLIFY-002 (after torrent_action done)
  - **Test**: `cargo test api::client::tests` ✅ All tests pass

---

## Phase 2: CLI and Filesystem (196-296 lines)

### CLI (`src/main.rs`)

- [x] **SIMPLIFY-003**: Extract main.rs helpers
  - [migration:SIMPLIFY-003-main-helpers](migration/SIMPLIFY-003-main-helpers.md)
  - Extract `load_config()` (used 3x), command execution helpers, `try_unmount()`
  - **Lines reduced**: ~76 lines (438 → 362)
  - **Risk**: Low - pure extraction
  - **Test**: `cargo test` ✅ All tests pass, `cargo clippy` ✅ Clean

### FUSE Filesystem (`src/fs/filesystem.rs`)

- [x] **SIMPLIFY-004**: Add FUSE logging macros
  - [migration:SIMPLIFY-004-fuse-logging](migration/SIMPLIFY-004-fuse-logging.md)
  - Create `fuse_log!`, `fuse_error!`, `fuse_ok!` macros
  - Replace ~42 repetitive logging blocks across 7 operations
  - **Lines reduced**: ~120 lines
  - **Risk**: Low - macros are declarative
  - **Test**: `cargo test`, `cargo clippy`, `cargo fmt` ✅ All pass

- [x] **SIMPLIFY-005**: Add error handler methods
  - [migration:SIMPLIFY-005-error-handlers](migration/SIMPLIFY-005-error-handlers.md)
  - Created `reply_ino_not_found!`, `reply_not_directory!`, `reply_not_file!`, `reply_no_permission!` macros
  - Replaced 15+ error handling blocks across 6 FUSE operations
  - **Lines reduced**: ~100 lines
  - **Risk**: Low - pure extraction
  - **Depends on**: SIMPLIFY-004 (after logging macros done)
  - **Test**: `cargo check` ✅ Clean, `cargo clippy` ✅ Clean, `cargo fmt` ✅ Applied

- [x] **SIMPLIFY-006**: Unify torrent discovery
  - [migration:SIMPLIFY-006-torrent-discovery](migration/SIMPLIFY-006-torrent-discovery.md)
  - Create single `discover_torrents()` async method
  - Replace 3 duplicated discovery implementations
  - **Lines reduced**: ~80 lines
  - **Risk**: Medium - async code consolidation
  - **Depends on**: None (can do in parallel)
  - **Test**: `cargo test` ✅ All tests pass, `cargo clippy` ✅ Clean

---

## Phase 3: Core Utilities (130-169 lines)

### Inode Management (`src/fs/inode.rs`, `src/types/inode.rs`)

- [x] **SIMPLIFY-007**: Simplify inode allocation
  - [migration:SIMPLIFY-007-inode-allocation](migration/SIMPLIFY-007-inode-allocation.md)
  - Create generic `allocate_entry()` helper
  - Add `with_ino()` to `InodeEntry`, simplify `build_path()`
  - **Lines reduced**: ~64 lines (730 → 666)
  - **Risk**: Medium - touches core data structures
  - **Test**: `cargo test fs::inode::tests` ✅ All tests pass

### Cache (`src/cache.rs`, `src/lib/`)

- [ ] **SIMPLIFY-008**: Extract ShardedCounter to lib
  - [migration:SIMPLIFY-008-sharded-counter](migration/SIMPLIFY-008-sharded-counter.md)
  - Move `ShardedCounter` to `src/lib/sharded_counter.rs`
  - Make it reusable utility
  - **Lines reduced**: ~43 lines (400 → 357)
  - **Risk**: Low - pure extraction
  - **Test**: `cargo test cache::tests`, check imports work

### Types (`src/types/*.rs`, `src/api/types.rs`)

- [ ] **SIMPLIFY-009**: Simplify API types
  - [migration:SIMPLIFY-009-api-types](migration/SIMPLIFY-009-api-types.md)
  - Merge `DownloadSpeed`/`UploadSpeed`, add `strum` derive
  - Derive `Serialize` for `TorrentStatus`, simplify error mappings
  - **Lines reduced**: ~70 lines (427 → 357)
  - **Risk**: Low - type changes only
  - **Test**: `cargo test api::types::tests`, verify JSON output

- [ ] **SIMPLIFY-012**: Consolidate type files
  - [migration:SIMPLIFY-012-type-consolidation](migration/SIMPLIFY-012-type-consolidation.md)
  - Merge `file.rs` into `torrent.rs`, add macro to `inode.rs`
  - Add `base_attr()` helper in `attr.rs`
  - **Lines reduced**: ~44 lines (130 → 86 total across files)
  - **Risk**: Low - structural changes only
  - **Depends on**: SIMPLIFY-009 (after API types done)
  - **Test**: `cargo test`, verify all type imports work

---

## Phase 4: Metrics and Streaming (75 lines)

### Metrics (`src/metrics.rs`)

- [ ] **SIMPLIFY-010**: Add metrics macros and trait
  - [migration:SIMPLIFY-010-metrics-macros](migration/SIMPLIFY-010-metrics-macros.md)
  - Create `record_op!` macro for 7 FuseMetrics methods
  - Create `LatencyMetrics` trait for avg calculations
  - **Lines reduced**: ~35 lines (294 → 259)
  - **Risk**: Low - macros preserve behavior
  - **Test**: `cargo test metrics::tests`

### Streaming (`src/api/streaming.rs`)

- [ ] **SIMPLIFY-011**: Extract streaming helpers
  - [migration:SIMPLIFY-011-streaming-helpers](migration/SIMPLIFY-011-streaming-helpers.md)
  - Create `consume_pending()`, `buffer_leftover()`, `read_from_stream()`
  - Reduce duplication between `read()` and `skip()`
  - **Lines reduced**: ~40 lines (505 → 465)
  - **Risk**: Medium - buffer handling is sensitive
  - **Test**: `cargo test api::streaming::tests`, test file reading

---

## Summary

| Phase | Tasks | Lines Reduced | Priority |
|-------|-------|---------------|----------|
| Phase 1 | SIMPLIFY-001, 002, 013, 014 | 283 | High |
| Phase 2 | SIMPLIFY-003, 004, 005, 006 | 376 | High |
| Phase 3 | SIMPLIFY-007, 008, 009, 012 | 221 | Medium |
| Phase 4 | SIMPLIFY-010, 011 | 75 | Medium |
| **Total** | **12 tasks** | **~955 lines** | |

**Current codebase**: ~5,700 lines  
**After simplification**: ~4,745 lines  
**Reduction**: ~17%

---

## Quick Reference

### Migration Guides

- [migration:SIMPLIFY-001](migration/SIMPLIFY-001-config-macros.md) - Config macros
- [migration:SIMPLIFY-002](migration/SIMPLIFY-002-torrent-control.md) - Torrent control
- [migration:SIMPLIFY-003](migration/SIMPLIFY-003-main-helpers.md) - Main.rs helpers
- [migration:SIMPLIFY-004](migration/SIMPLIFY-004-fuse-logging.md) - FUSE logging
- [migration:SIMPLIFY-005](migration/SIMPLIFY-005-error-handlers.md) - Error handlers
- [migration:SIMPLIFY-006](migration/SIMPLIFY-006-torrent-discovery.md) - Discovery
- [migration:SIMPLIFY-007](migration/SIMPLIFY-007-inode-allocation.md) - Inode allocation
- [migration:SIMPLIFY-008](migration/SIMPLIFY-008-sharded-counter.md) - ShardedCounter
- [migration:SIMPLIFY-009](migration/SIMPLIFY-009-api-types.md) - API types
- [migration:SIMPLIFY-010](migration/SIMPLIFY-010-metrics-macros.md) - Metrics macros
- [migration:SIMPLIFY-011](migration/SIMPLIFY-011-streaming-helpers.md) - Streaming helpers
- [migration:SIMPLIFY-012](migration/SIMPLIFY-012-type-consolidation.md) - Type consolidation
- [migration:SIMPLIFY-013](migration/SIMPLIFY-013-tracing-instrument.md) - Tracing
- [migration:SIMPLIFY-014](migration/SIMPLIFY-014-request-helpers.md) - Request helpers

### Testing Commands

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test config::tests
cargo test api::client::tests
cargo test api::types::tests
cargo test fs::inode::tests
cargo test cache::tests
cargo test metrics::tests

# Check and lint
cargo check
cargo clippy
cargo fmt
```

### Completion Criteria

Each task should:
- Have code changes committed
- Pass `cargo test`
- Pass `cargo clippy`
- Pass `cargo fmt`
- Have checkbox marked as complete
- Update this file with any discoveries

---

*Generated from code analysis - February 14, 2026*
