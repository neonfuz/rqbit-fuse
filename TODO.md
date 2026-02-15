# torrent-fuse Improvement Checklist

## How to Use This File

Each item is designed to be completed independently. Research references are stored in `research/` folder with corresponding names.

**Workflow:**
1. Pick an unchecked item
2. If it references a research file (e.g., `[research:cache-design]`), read that file first
3. Complete the task
4. Check the box
5. Commit your changes

---

## Phase 1: Critical Fixes (Must Fix Before Production)

### Cache System (src/cache.rs)

- [x] **CACHE-001**: Research and document cache design options
  - [research:cache-design](research/cache-design.md) comparing:
    - Current DashMap + custom LRU approach
    - Using `lru` crate
    - Using `cached` crate  
    - Using `moka` crate (RECOMMENDED)
  - Documented migration plan to Moka
  - Fixes identified: CACHE-002 through CACHE-006

- [x] **CACHE-002**: Implement O(1) cache eviction
  - Depends on: `[research:cache-design]`
  - Migrated to `moka` crate which provides O(1) eviction via TinyLFU algorithm
  - No full scans required - eviction is handled internally
  - Thread-safe with lock-free reads

- [x] **CACHE-003**: Fix capacity check race condition
  - Fixed by `moka` crate's atomic operations
  - Moka handles capacity management internally with proper synchronization
  - Concurrent insertions are handled safely without race conditions

- [x] **CACHE-004**: Fix `contains_key()` memory leak
  - Fixed by using `moka`'s built-in TTL handling
  - Expired entries are automatically removed, never returned
  - `contains_key()` now uses `get()` which returns None for expired entries

- [x] **CACHE-005**: Fix TOCTOU in expired entry removal
  - Fixed by `moka` crate's atomic operations
  - No manual expiration checking/removal needed
  - Moka handles expiration transparently and atomically

- [x] **CACHE-006**: Fix cache remove ambiguity
  - Fixed by `moka` crate's clear API semantics
  - `invalidate()` removes without returning value
  - Current implementation returns `Option<V>` which clearly indicates NotFound vs Removed

- [x] **CACHE-007**: Add cache statistics endpoint
  - Implemented `stats()` method returning `CacheStats`
  - Tracks hits, misses, and cache size
  - Eviction count not exposed by moka (handled internally)

- [ ] **CACHE-008**: Optimize cache statistics collection
  - Depends on: CACHE-007
  - Reduce contention on stats counter
  - Use sharded counters or atomic operations
  - Measure impact on concurrent read performance

### Filesystem Implementation (src/fs/filesystem.rs)

- [x] **FS-001**: Research async FUSE patterns
  - Create `research/async-fuse-patterns.md` and `[spec:async-fuse]` documenting:
    - Current `block_in_place` + `block_on` approach and deadlock risks
    - Alternative: Spawn tasks and use channels
    - Alternative: Use `fuser` async support if available
    - Alternative: Restructure to avoid async-in-sync
  - Document recommended approach with examples

- [ ] **FS-002**: Fix blocking async in sync callbacks
  - Depends on: `[research:async-fuse-patterns]`, `[spec:async-fuse]`
  - Replace `block_in_place` + `block_on` pattern
  - Eliminate deadlock risk in FUSE callbacks
  - Add stress test with concurrent operations

- [ ] **FS-003**: Implement unique file handle allocation
  - Currently using inode as file handle (violates FUSE semantics)
  - Create handle table with unique IDs per open
  - Map handles to (inode, open flags, state)
  - Update all file operations to use handles

- [ ] **FS-004**: Fix read_states memory leak
  - Clean up `read_states` entries in `release()` callback
  - Add TTL-based eviction for orphaned states
  - Add memory usage metrics for read_states

- [ ] **FS-005**: Replace std::sync::Mutex with tokio::sync::Mutex
  - Find all std::sync::Mutex in async context (lines 73, 77, 79, 83, 101, 102)
  - Replace with tokio::sync::Mutex
  - Verify no blocking operations remain

- [ ] **FS-006**: Fix path resolution for nested directories
  - Line 1117-1123: Nested directories resolve incorrectly
  - Add test cases for multi-level directory structures
  - Fix and verify correct resolution

- [ ] **FS-007**: Add proper FUSE operation tests
  - Depends on: `[spec:testing]`
  - Create `tests/fuse_operations.rs`
  - Test lookup, getattr, readdir, read with real FUSE
  - Use fuse_mt or similar for testing
  - Include error case testing

- [x] **FS-008**: Fix race condition in torrent discovery
  - Lines 1351-1407: readdir() spawned discovery without atomic check-and-act
  - Two concurrent calls could both pass cooldown before either updated timestamp
  - Fixed by using AtomicU64 with compare_exchange for lock-free atomic check-and-set
  - Only one task proceeds with discovery even with concurrent readdir() calls

### Inode Management (src/fs/inode.rs)

- [x] **INODE-001**: Research inode table design
  - Create `research/inode-design.md` and `[spec:inode-design]` comparing:
    - Current multi-map approach
    - Single DashMap with composite keys
    - RwLock + HashMap approach
    - Trade-offs for each

- [ ] **INODE-002**: Make inode table operations atomic
  - Depends on: `[research:inode-design]`, `[spec:inode-design]`
  - Currently `path_to_inode` and `entries` updated separately
  - Use composite key or transaction to make atomic
  - Add test for concurrent inode creation/removal

- [ ] **INODE-003**: Fix torrent directory mapping
  - Depends on: `[spec:inode-design]`
  - Currently maps torrent_id to file's parent
  - Should map to torrent directory inode
  - Fix path resolution for torrent files
  - Update directory listing to show torrent contents

- [ ] **INODE-004**: Make entries field private
  - Depends on: `[spec:inode-design]`
  - Change `pub entries` to private
  - Add controlled accessor methods
  - Prevent external code from breaking invariants
  - Update all existing callers

- [ ] **INODE-005**: Fix stale path references
  - Depends on: `[spec:inode-design]`
  - `remove_inode()` rebuilds path which may be outdated
  - Store canonical path or use inode-based removal
  - Add test for path updates after directory rename

### Streaming Implementation (src/api/streaming.rs)

- [ ] **STREAM-001**: Fix unwrap panic in stream access
  - Line 384: `.unwrap()` on stream get after lock
  - Handle case where stream was dropped between check and access
  - Return proper error instead of panic

- [ ] **STREAM-002**: Fix check-then-act race condition
  - Lines 372-407: Lock is released between check and action
  - Use entry API or keep lock across entire operation
  - Add test for concurrent stream access

- [ ] **STREAM-003**: Add yielding in large skip operations
  - Lines 187-236: Large skips block runtime
  - Add `.await` yield points every N bytes
  - Use `tokio::task::yield_now()` or similar

- [ ] **STREAM-004**: Implement backward seeking
  - Currently only supports forward seeks
  - Implement seek backward in stream
  - Add seek tests for all directions

---

## Phase 2: High Priority Fixes

### Error Handling

- [x] **ERROR-001**: Research typed error design
  - Create `research/error-design.md` and `[spec:error-handling]` with:
    - Current string-based error detection issues
    - Proposed error enum hierarchy
    - FUSE error code mapping strategy
    - Library vs application error separation

- [ ] **ERROR-002**: Replace string matching with typed errors
  - Depends on: `[research:error-design]`, `[spec:error-handling]`
  - Remove `.contains("not found")` pattern (filesystem.rs:1012-1015)
  - Create specific error types for each failure mode
  - Update error mapping to FUSE codes

- [ ] **ERROR-003**: Fix silent failures in list_torrents()
  - Depends on: `[spec:error-handling]`
  - Lines 320-338: Logs but doesn't propagate errors
  - Return Result with partial success info
  - Let caller decide how to handle partial failures

- [ ] **ERROR-004**: Preserve error context
  - Depends on: `[spec:error-handling]`
  - Lines 289-292: `.unwrap_or_else()` loses original error
  - Use proper error chaining with `anyhow::Context`
  - Ensure root cause is preserved in error messages

### API Client (src/api/client.rs)

- [ ] **API-001**: Remove panics from API client
  - Lines 142-143, 170-171: Replace `.expect()` with Result
  - Line 541: Handle request clone failure gracefully
  - Return proper errors for all failure cases

- [ ] **API-002**: Add authentication support
  - Research rqbit auth methods
  - Add auth token/API key support to client
  - Update configuration for credentials
  - Add auth failure error handling

- [ ] **API-003**: Fix N+1 query in list_torrents()
  - Lines 308-346: Makes N+1 API calls
  - Use bulk endpoint if available
  - Or add caching to reduce redundant calls
  - Add performance benchmark

- [ ] **API-004**: Use reqwest::Url instead of String
  - Change URL fields from String to reqwest::Url
  - Validate URLs at construction time
  - Fail fast on invalid URL configuration

### Type System

- [ ] **TYPES-001**: Research torrent type consolidation
  - Create `research/torrent-types.md` analyzing:
    - `types/torrent.rs` (appears dead code)
    - `api/types.rs::TorrentInfo`
    - Any other torrent representations
  - Document consolidation strategy

- [ ] **TYPES-002**: Consolidate torrent representations
  - Depends on: `[research:torrent-types]`
  - Remove or complete `types/torrent.rs`
  - Use single canonical type throughout codebase
  - Update all imports and conversions

- [ ] **TYPES-003**: Remove unused types
  - Remove `TorrentSummary` (api/types.rs:151-161)
  - Remove `FileStats` (api/types.rs:259-264)
  - Verify no code references these types
  - Check tests for usage

- [ ] **TYPES-004**: Fix platform-dependent types
  - Change `file_index: usize` to `u64` (types/inode.rs:16)
  - Audit for other usize vs u64 issues
  - Ensure 32-bit and 64-bit compatibility

- [ ] **TYPES-005**: Improve InodeEntry children lookup
  - `children: Vec<u64>` has O(n) lookup
  - Consider HashSet or DashSet for O(1)
  - Measure impact on directory operations

### Configuration (src/config/mod.rs)

- [ ] **CONFIG-001**: Add comprehensive config validation
  - Validate URLs (non-empty, valid format)
  - Validate timeouts (positive, reasonable range)
  - Validate paths (exist, permissions)
  - Return detailed validation errors

- [ ] **CONFIG-002**: Remove hardcoded UID/GID
  - Lines 17-18, 36-37: Remove hardcoded 1000
  - Use current user's UID/GID by default
  - Make configurable via config file

- [ ] **CONFIG-003**: Add documentation to config module
  - Add doc comments to all structs
  - Document all configuration fields
  - Add example configurations
  - Document environment variable names

- [ ] **CONFIG-004**: Fix inconsistent env var naming
  - Audit all environment variables
  - Use consistent prefix (e.g., `TORRENT_FUSE_*`)
  - Document naming convention

- [ ] **CONFIG-005**: Fix case-sensitive file extension detection
  - Make config file detection case-insensitive
  - Support .toml, .TOML, .json, .JSON
  - Add test for various extensions

---

## Phase 3: Documentation & Testing

### Documentation

- [ ] **DOCS-001**: Research documentation standards
  - Create `research/doc-standards.md` with:
    - Rust doc comment conventions
    - Required sections (Examples, Panics, Errors)
    - Crate-level documentation requirements
    - Module-level documentation requirements

- [ ] **DOCS-002**: Add crate-level documentation
  - Depends on: `[research:doc-standards]`
  - Document overall purpose and architecture
  - Add quickstart example
  - Document feature flags if any

- [ ] **DOCS-003**: Document blocking behavior
  - Add prominent documentation about async/blocking
  - Document which operations block
  - Add warnings about deadlock risks
  - Include in crate-level docs

- [ ] **DOCS-004**: Document public API
  - Depends on: `[research:doc-standards]`
  - Add doc comments to all public items
  - Include examples where appropriate
  - Document error conditions

- [ ] **DOCS-005**: Add troubleshooting guide
  - Common issues and solutions
  - Performance tuning tips
  - Debugging techniques
  - FAQ section

- [ ] **DOCS-006**: Document security considerations
  - Path traversal prevention
  - Resource exhaustion limits
  - Error information leakage
  - TOCTOU vulnerabilities

### Testing

- [x] **TEST-001**: Research FUSE testing approaches
  - Create `research/fuse-testing.md` and `[spec:testing]` documenting:
    - Testing with libfuse mock
    - Docker-based integration tests
    - Testing on CI (GitHub Actions)
    - Real filesystem operation tests

- [ ] **TEST-002**: Add FUSE operation integration tests
  - Depends on: `[research:fuse-testing]`, `[spec:testing]`
  - Test mount/unmount cycles
  - Test file operations (open, read, close)
  - Test directory operations (lookup, readdir)
  - Test error scenarios

- [ ] **TEST-003**: Fix misleading concurrent test
  - Depends on: `[spec:testing]`
  - `test_concurrent_torrent_additions` doesn't test concurrency
  - Rewrite with actual concurrent operations
  - Use barriers or synchronization
  - Verify proper concurrent behavior

- [ ] **TEST-004**: Add cache integration tests
  - Depends on: `[spec:testing]`
  - Test TTL expiration
  - Test LRU eviction
  - Test concurrent cache access
  - Test cache statistics accuracy

- [ ] **TEST-005**: Add mock verification to tests
  - Depends on: `[spec:testing]`
  - Verify WireMock expectations are met
  - Check request counts and patterns
  - Add assertions for API call efficiency

- [x] **TEST-006**: Research property-based testing
  - Create `research/property-testing.md` and `[spec:testing]`
  - Document proptest or quickcheck integration
  - Identify properties to test (invariants)

- [ ] **TEST-007**: Add property-based tests
  - Depends on: `[research:property-testing]`, `[spec:testing]`
  - Test inode table invariants
  - Test cache consistency properties
  - Test path resolution properties

---

## Phase 4: Architectural Improvements

### Module Organization

- [ ] **ARCH-001**: Audit module visibility
  - Review all `pub` declarations
  - Make internal modules private
  - Identify what should be public API
  - Create `research/public-api.md` with decisions

- [ ] **ARCH-002**: Implement module re-exports
  - Depends on: `[research:public-api]`
  - Add convenience re-exports at module roots
  - Export `fs::TorrentFS` instead of `fs::filesystem::TorrentFS`
  - Update all imports to use new paths

- [ ] **ARCH-003**: Extract mount operations
  - Move mount logic from main.rs to new module
  - Create `src/mount.rs` or similar
  - Keep main.rs focused on CLI only

- [ ] **ARCH-004**: Split RqbitClient into focused modules
  - Currently too large (HTTP, retry, circuit breaking, streaming)
  - Extract retry logic
  - Extract circuit breaker
  - Extract streaming to separate module

### Resource Management

- [ ] **RES-001**: Research signal handling options
  - Create `research/signal-handling.md` documenting:
    - tokio::signal usage
    - Graceful shutdown patterns
    - Child process cleanup on SIGTERM
    - FUSE unmount on signal

- [ ] **RES-002**: Implement graceful shutdown
  - Depends on: `[research:signal-handling]`
  - Handle SIGINT and SIGTERM
  - Flush caches on shutdown
  - Unmount FUSE cleanly
  - Clean up background tasks

- [ ] **RES-003**: Add child process cleanup
  - Ensure subprocess cleanup on exit
  - Add timeout for graceful shutdown
  - Force kill if needed
  - Test cleanup behavior

- [ ] **RES-004**: Add resource limits
  - Maximum cache size (bytes, not just entries)
  - Maximum open streams
  - Maximum inode count
  - Maximum concurrent operations

### Performance

- [ ] **PERF-001**: Research read-ahead strategies
  - Create `research/read-ahead.md` documenting:
    - Current prefetch behavior (fetched but dropped)
    - Sequential read detection
    - Configurable read-ahead size
    - Implementation approaches

- [ ] **PERF-002**: Implement read-ahead/prefetching
  - Depends on: `[research:read-ahead]`
  - Detect sequential access patterns
  - Prefetch next chunks
  - Don't immediately drop prefetched data
  - Make configurable

- [ ] **PERF-003**: Implement statfs operation
  - Add FUSE statfs callback
  - Return filesystem statistics
  - Required for some applications

- [ ] **PERF-004**: Implement access operation
  - Add FUSE access callback
  - Check file permissions
  - Required for proper permission handling

- [ ] **PERF-005**: Optimize buffer allocation
  - streaming.rs:394,423: Avoid zeroing large buffers
  - Use `Vec::with_capacity` instead of `vec![0u8; size]`
  - Profile memory allocation

- [ ] **PERF-006**: Add performance benchmarks
  - Depends on: CACHE-007 (statistics)
  - Benchmark cache operations
  - Benchmark FUSE operations
  - Create performance regression workflow

### Metrics

- [ ] **METRICS-001**: Fix race conditions in averages
  - Research atomic average calculation
  - Fix race in metrics calculations
  - Use proper atomic operations

- [ ] **METRICS-002**: Add critical cache metrics
  - Hit rate, miss rate
  - Eviction counts
  - Cache size over time
  - Required for performance monitoring

- [ ] **METRICS-003**: Reduce trace overhead
  - Remove traces from hot paths
  - Make trace level configurable
  - Measure overhead impact

- [ ] **METRICS-004**: Add periodic logging mechanism
  - Log metrics at regular intervals
  - Configurable log frequency
  - Human-readable format

---

## Quick Reference

### Research Files Created

When you see `[research:X]`, it means read the file at:
- `research/cache-design.md`
- `research/async-fuse-patterns.md`
- `research/inode-design.md`
- `research/error-design.md`
- `research/torrent-types.md`
- `research/doc-standards.md`
- `research/fuse-testing.md`
- `research/property-testing.md`
- `research/public-api.md`
- `research/signal-handling.md`
- `research/read-ahead.md`

### Priority Order

1. **Phase 1**: Critical fixes (safety/correctness)
2. **Phase 2**: High priority (reliability/maintainability)
3. **Phase 3**: Documentation & testing (understanding/confidence)
4. **Phase 4**: Architecture (performance/design)

### Completion Criteria

Each item should:
- Have code changes committed
- Have tests added/updated
- Pass `cargo test`
- Pass `cargo clippy`
- Pass `cargo fmt`
- Have checkbox marked as complete

---

*Generated from code review - February 14, 2026*
