# SIMPLIFY-2.md - Codebase Simplification Tasks

This checklist contains individually actionable items to simplify the rqbit-fuse codebase based on the code review.

## High Priority

### 1. Remove ShardedCounter Module
- [x] Delete `src/sharded_counter.rs` (already removed)
- [x] Remove `pub mod sharded_counter;` from `src/lib.rs` (already removed)
- [x] Replace `ShardedCounter` usage in `src/cache.rs` with simple `AtomicU64` (already using AtomicU64)
- [x] Update `src/cache.rs` imports to remove `ShardedCounter` (no imports needed)
- [x] Update test comment referencing sharded counters
- [x] Run tests to verify cache still works correctly

### 2. Archive Migration Directory
- [x] Verify all migration plans in `migration/` are completed (check git history)
- [x] Move `migration/` to `archive/migration/` or delete if fully completed
- [x] Update any references in documentation
- [x] Commit the cleanup

### 3. Archive Research Directory
- [x] Review `research/` directory contents for any active relevance
- [x] Move valuable notes to archive/research/
- [x] Move `research/` directory to archive
- [x] Commit the cleanup

### 4. Reduce Test Coverage in inode.rs
- [ ] Analyze `src/fs/inode.rs` test coverage (currently ~150% lines ratio)
- [ ] Identify redundant tests (e.g., multiple tests for same functionality)
- [ ] Remove duplicate test cases while maintaining core coverage
- [ ] Target: Reduce from 720 test lines to ~360 lines (50% ratio)
- [ ] Run `cargo test` to ensure all tests still pass

## Medium Priority

### 5. Simplify Metrics System
- [ ] Review `src/metrics.rs` (657 lines)
- [ ] Identify over-engineered parts (custom LatencyMetrics trait, atomic snapshots)
- [ ] Replace with simpler counter-based approach or use `metrics` crate
- [ ] Remove unused metric types
- [ ] Update all call sites in `src/api/client.rs` and `src/fs/filesystem.rs`
- [ ] Run tests to verify metrics still collect correctly

### 6. Evaluate Circuit Breaker Necessity
- [ ] Review `src/api/circuit_breaker.rs`
- [ ] Analyze if circuit breaking adds value for localhost rqbit API
- [ ] If overkill: Remove circuit breaker and simplify to basic retry logic
- [ ] Update `src/api/client.rs` to remove circuit breaker usage
- [ ] Run tests to verify API client still works

### 7. Simplify File Handle State Tracking
- [ ] Review `src/types/handle.rs`
- [ ] Identify complex read pattern detection (sequential reads, prefetching)
- [ ] Check if prefetching logic is actually used
- [ ] Simplify to basic handle tracking if advanced features unused
- [ ] Remove unused state tracking fields
- [ ] Run tests to verify file handles still work

### 8. Replace Config Macros
- [ ] Review `src/config/mod.rs` macros: `default_fn!`, `default_impl!`, `env_var!`
- [ ] Replace with standard Rust patterns:
  - `default_fn!` → `impl Default` or `const fn`
  - `default_impl!` → derive `Default` or manual impl
  - `env_var!` → standard env var parsing with `std::env::var`
- [ ] Remove macro definitions (~35 lines)
- [ ] Run tests to verify config still loads correctly

## Lower Priority

### 9. Review Unused Dependencies
- [ ] Check `Cargo.toml` for potentially unused dependencies:
  - [ ] `strum` - verify only used for Display derive
  - [ ] `base64` - verify only used for HTTP Basic Auth
  - [ ] `proptest` - heavy dev dependency, verify usage
- [ ] For each unused dependency:
  - [ ] Remove from `Cargo.toml`
  - [ ] Remove related code
  - [ ] Run `cargo build` to verify

### 10. Merge Duplicate Error Types
- [ ] Review `src/fs/error.rs` (FuseError) and `src/api/types.rs` (ApiError)
- [ ] Identify overlapping error variants
- [ ] Merge into unified error type or simplify hierarchy
- [ ] Update all error conversions (`to_fuse_error()`)
- [ ] Run tests to verify error handling still works

### 11. Consolidate Inode Types
- [ ] Review `src/types/inode.rs` and `src/fs/inode.rs`
- [ ] Identify split responsibilities
- [ ] Merge into single module or clarify separation
- [ ] Update all imports across codebase
- [ ] Run tests to verify inodes still work

### 12. Remove Unused Config Fields
- [ ] Review `src/config/mod.rs` for unimplemented features:
  - [ ] `prefetch_enabled` - verify if prefetching is implemented
  - [ ] `piece_check_enabled` - verify if piece verification is implemented
- [ ] For each unused field:
  - [ ] Remove from struct
  - [ ] Remove from default functions
  - [ ] Remove from env var parsing
  - [ ] Remove from validation
  - [ ] Run tests to verify

## Verification Steps

After completing each task:
1. Run `cargo build` to check for compilation errors
2. Run `cargo test` to verify tests pass
3. Run `cargo clippy` to check for warnings
4. Run `cargo fmt` to format code
5. Update documentation if needed

## Estimated Impact

- **Lines of code**: Remove ~1,500-2,000 lines
- **Dependencies**: Remove 3-5 unused crates
- **Compile time**: 10-20% faster
- **Maintainability**: Significantly improved
