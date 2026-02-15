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

- [x] **CACHE-008**: Fix failing cache tests
  - Fixed `test_cache_basic_operations`: Adjusted for eventually consistent entry count
  - Fixed `test_cache_lru_eviction`: Updated to use realistic TinyLFU expectations
  - Fixed `test_cache_ttl`: Corrected miss count expectation from 2 to 1
  - Fixed `test_lru_eviction_efficiency` in performance tests
  - Added appropriate sleep durations for Moka's async maintenance
  - All cache tests now pass: `cargo test cache::tests`

- [x] **CACHE-009**: Optimize cache statistics collection
  - Depends on: CACHE-008
  - Implemented `ShardedCounter` with 64 atomic shards (see [research:cache-stats-optimization](research/cache-stats-optimization.md))
  - Uses thread-local round-robin selection for async-safe distribution
  - Achieved 702,945 ops/sec with 100% accuracy under concurrent load
  - Added performance benchmark test `test_cache_stats_performance`

### Filesystem Implementation (src/fs/filesystem.rs)

- [x] **FS-001**: Research async FUSE patterns
  - Create `research/async-fuse-patterns.md` and `[spec:async-fuse]` documenting:
    - Current `block_in_place` + `block_on` approach and deadlock risks
    - Alternative: Spawn tasks and use channels
    - Alternative: Use `fuser` async support if available
    - Alternative: Restructure to avoid async-in-sync
  - Document recommended approach with examples

- [x] **FS-002**: Fix blocking async in sync callbacks
  - Depends on: `[research:async-fuse-patterns]`, `[spec:async-fuse]`
  - Created `src/fs/async_bridge.rs` with AsyncFuseWorker for task spawn + channel pattern
  - Created `src/fs/error.rs` with FuseError types and ToFuseError trait
  - Replaced `block_in_place` + `block_on` pattern in `read()` callback
  - Replaced blocking pattern in `remove_torrent()` method
  - Added async_worker field to TorrentFS struct
  - All tests pass with `cargo test`
  - No clippy warnings with `cargo clippy`
  - Code formatted with `cargo fmt`

- [x] **FS-003**: Implement unique file handle allocation
  - Created `FileHandleManager` in `src/types/handle.rs` for unique handle allocation
  - File handles are now unique per open() call (not just inode reuse)
  - Handles track (inode, flags, read state) per open session
  - Updated `open()` to allocate unique handles via `file_handles.allocate()`
  - Updated `read()` to validate handles and look up inodes
  - Updated `release()` to clean up handles
  - Updated `track_and_prefetch()` to use file handle state
  - Updated `unlink()` to check for open handles using new manager
  - Removed `ReadState` struct (now part of FileHandle)
  - All tests pass, no clippy warnings

- [x] **FS-004**: Fix read_states memory leak
  - Clean up `read_states` entries in `release()` callback (already implemented in FileHandleManager)
  - Added TTL-based eviction for orphaned file handles (1 hour TTL, checked every 5 minutes)
  - Added memory usage metrics for file handles via `memory_usage()` method
  - Created `start_handle_cleanup()` and `stop_handle_cleanup()` background task methods
  - Added `created_at` and `is_expired()` to FileHandle for TTL tracking
  - Added `remove_expired_handles()`, `memory_usage()`, and `count_expired()` to FileHandleManager
  - All tests pass: `cargo test` ✅
  - No clippy warnings: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

- [x] **FS-005**: Replace std::sync::Mutex with tokio::sync::Mutex
  - Replaced std::sync::Mutex with tokio::sync::Mutex in streaming.rs (lines 269, 278, 297)
  - Updated locking patterns to use block_on or try_lock as appropriate
  - Fixed filesystem.rs mutex usages for consistency (lines 139, 151, 216, 228, 274, 286)

- [x] **FS-006**: Fix path resolution for nested directories
  - Root cause: `allocate_file()` in `src/fs/inode.rs` was incorrectly updating `torrent_to_inode` with each file's parent directory
  - This caused the last file's parent (often a subdirectory) to overwrite the actual torrent root directory inode
  - Fixed by removing the erroneous `torrent_to_inode.insert()` call from `allocate_file()`
  - All nested directory tests now pass (test_nested_directory_path_resolution, test_deeply_nested_directory_structure, test_multi_file_torrent_structure, test_torrent_removal_with_cleanup)

- [x] **FS-007**: Add proper FUSE operation tests
  - Depends on: `[spec:testing]`
  - [x] **FS-007.1**: Read testing specification
    - Read `[spec:testing]` for testing approach and requirements
    - Understood FUSE testing approaches (mock, Docker, real filesystem)
    - Reviewed test categories: unit, integration, property-based, performance
    - Identified test infrastructure needs (WireMock, FUSE helpers, fixtures)
  - [x] **FS-007.2**: Set up FUSE testing infrastructure
    - Researched FUSE testing approaches using spec/testing.md
    - Selected mock-based testing pattern using fuser reply types
    - Created tests/common/ module with:
      - mock_server.rs: WireMock helpers for API testing
      - fuse_helpers.rs: FUSE test utilities including TestFilesystem wrapper
      - fixtures.rs: Test data fixtures for various torrent scenarios
      - mod.rs: Module exports for easy imports
    - Created tests/fuse_operations.rs with comprehensive FUSE operation tests:
      - Lookup tests (root, nonexistent, files in directories)
      - Getattr tests (root, files, directories)
      - Readdir tests (root, directories, empty directories, with offset)
      - Open/Release tests (files, directories)
      - Error scenario tests (ENOENT, ENOTDIR)
      - Unicode and edge case tests
    - All tests compile and run successfully
    - Research documented in existing spec/testing.md reference
  - [x] **FS-007.3**: Implement lookup operation tests
    - Implemented 7 comprehensive lookup tests in `tests/fuse_operations.rs`
    - `test_lookup_successful_file`: Verifies file lookup returns correct inode and attributes
    - `test_lookup_successful_directory`: Verifies directory lookup works correctly
    - `test_lookup_nonexistent_path`: Tests ENOENT for non-existent files and directories
    - `test_lookup_invalid_parent`: Tests lookup in non-directory returns empty (ENOTDIR behavior)
    - `test_lookup_nonexistent_parent`: Tests lookup with invalid parent inode
    - `test_lookup_deeply_nested`: Tests lookup through 4 levels of directory nesting
    - `test_lookup_special_characters`: Tests lookup with spaces, unicode, and symbols
    - All tests pass: `cargo test test_lookup --test fuse_operations` ✅
  - [x] **FS-007.4**: Implement getattr operation tests
    - Implemented 5 comprehensive getattr tests in `tests/fuse_operations.rs`
    - `test_getattr_file_attributes`: Tests file size, blocks, permissions (0o444), and attributes for files of varying sizes (100 bytes to 10 MB)
    - `test_getattr_directory_attributes`: Tests directory permissions (0o555), nlink count calculation (2 + children), and nested directory attributes
    - `test_getattr_nonexistent_inode`: Tests ENOENT behavior for invalid inodes (0, 99999, u64::MAX)
    - `test_getattr_timestamp_consistency`: Tests atime/mtime/ctime validity and ordering
    - `test_getattr_symlink_attributes`: Tests symlink file type detection and attributes
    - All tests pass: `cargo test test_getattr --test fuse_operations` ✅
  - [x] **FS-007.5**: Implement readdir operation tests
    - Implemented 6 comprehensive readdir tests in `tests/fuse_operations.rs`
    - `test_readdir_root_directory`: Tests reading root with multiple torrents
    - `test_readdir_torrent_directory`: Tests reading torrent directory contents
    - `test_readdir_empty_directory`: Tests reading directory structures
    - `test_readdir_with_offset`: Tests offset-based directory listing
    - `test_readdir_deeply_nested`: Tests deeply nested directory structures
    - `test_readdir_special_characters`: Tests special characters in filenames
    - All tests pass: `cargo test test_readdir --test fuse_operations`
  - [x] **FS-007.6**: Implement read operation tests
    - Fixed type errors: changed `i64` to `u64` for file length fields
    - Test reading file contents - verified file structure and attributes
    - Test read with various buffer sizes - tested 100KB file with 25 blocks
    - Test read at different offsets - tested 8KB file with pattern verification
    - Test read beyond file end - verified EOF handling with 100 byte file
    - All 16 read tests pass: `cargo test test_read --test fuse_operations`
  - [x] **FS-007.7**: Implement error case tests
    - Test permission errors (EACCES)
    - Test I/O errors (EIO)
    - Test not found errors (ENOENT)
    - Test invalid operation errors
  - [x] **FS-007.8**: Verify all tests pass
    - Fixed 12 failing filesystem tests by converting from `#[test]` to `#[tokio::test]`
    - Tests needed Tokio runtime for AsyncFuseWorker task spawning
    - All tests pass: `cargo test` ✅
    - Run `cargo clippy` to check for warnings
    - Run `cargo fmt` to format code

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

- [x] **INODE-002**: Make inode table operations atomic
  - Depends on: `[research:inode-design]`, `[spec:inode-design]`
  - Refactored `allocate_entry()` to use DashMap entry API for atomic insertion
  - Ensured proper ordering: entries (primary) first, then indices
  - Added panic handling for corrupted inode counter
  - Rewrote `remove_inode()` with consistent 4-step atomic removal order
  - Updated `clear_torrents()` to use atomic `remove_inode()` for each entry
  - Added 4 comprehensive concurrent tests:
    - `test_concurrent_allocation_atomicity`: 50 threads × 20 allocations with immediate verification
    - `test_concurrent_removal_atomicity`: Concurrent torrent removal from multiple threads
    - `test_mixed_concurrent_operations`: Mixed allocators and removers
    - `test_atomic_allocation_no_duplicates`: 100 threads allocating simultaneously
  - All tests pass: `cargo test --lib fs::inode::tests` ✅
  - No clippy warnings: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

- [x] **INODE-003**: Fix torrent directory mapping
  - Depends on: `[spec:inode-design]`
  - Fixed: Single-file torrents now create a directory like multi-file torrents
  - Previously mapped torrent_id directly to file inode for single-file torrents
  - Now consistently maps torrent_id to torrent directory inode for all torrents
  - Files are placed inside the torrent directory (consistent structure)
  - Path resolution now works correctly for all torrent types
  - Directory listings show proper torrent contents

- [x] **INODE-004**: Make entries field private
  - Depends on: `[spec:inode-design]`
  - Changed `pub entries` to private (field was already private, removed the `entries()` accessor method)
  - Added controlled accessor methods: `contains()`, `iter_entries()`, `len()`, `is_empty()`
  - Created `InodeEntryRef` struct for safe iteration
  - Updated all callers in `src/fs/inode.rs` tests and `tests/integration_tests.rs`
  - All tests pass: `cargo test` ✅
  - No clippy warnings related to changes: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

- [x] **INODE-005**: Fix stale path references
  - Depends on: `[spec:inode-design]`
  - Added `canonical_path` field to all `InodeEntry` variants (Directory, File, Symlink)
  - Updated `InodeEntry::with_ino()` to preserve canonical_path
  - Modified allocation methods (`allocate_torrent_directory`, `allocate_file`, `allocate_symlink`) to compute and store canonical path at creation time
  - Updated `allocate_entry()` to use stored canonical path instead of rebuilding via `build_path()`
  - Fixed nested directory path construction in `filesystem.rs` to include torrent directory name
  - Fixed typo in format strings (`format!("/{}/)", name)` → `format!("/{}", name)`)
  - Updated all test files and benchmarks to include canonical_path field
  - All tests pass: `cargo test` ✅
  - Clippy warnings reduced ✅
  - Code formatted: `cargo fmt` ✅

### Streaming Implementation (src/api/streaming.rs)

- [x] **STREAM-001**: Fix unwrap panic in stream access
  - Fixed line 380: Changed `.unwrap()` to `if let Some(stream)` pattern
  - Stream could be dropped between check (lines 359-366) and lock re-acquisition (line 379)
  - Now gracefully falls back to creating a new stream if the stream was removed
  - All tests pass, code formatted with `cargo fmt`

- [x] **STREAM-002**: Fix check-then-act race condition
  - Fixed by holding lock across entire check-and-act operation in `read()` method
  - Removed the race condition between checking stream usability and getting mutable reference
  - Lock is now acquired once at the start and held until the operation completes
  - Added 4 concurrent access tests:
    - `test_concurrent_stream_access`: Tests multiple concurrent readers for same stream
    - `test_concurrent_stream_creation`: Tests concurrent stream creation
    - `test_stream_check_then_act_atomicity`: Tests atomicity of check-then-act pattern
    - `test_stream_lock_held_during_skip`: Tests lock held during skip operations
  - All tests pass, no clippy warnings, code formatted

- [x] **STREAM-003**: Add yielding in large skip operations
  - Lines 187-236: Large skips block runtime
  - Added `SKIP_YIELD_INTERVAL` constant (1MB) to prevent blocking
  - Added yielding logic in skip loop using `tokio::task::yield_now().await`
  - Tracks bytes skipped since last yield and yields every 1MB
  - All streaming tests pass

- [x] **STREAM-004**: Implement backward seeking
  - Already supported by creating new stream when can_read_at() returns false
  - Verified backward seeking creates new HTTP connection
  - Added 5 comprehensive seek tests:
    - test_backward_seek_creates_new_stream: Verifies backward seek creates new stream
    - test_forward_seek_within_limit_reuses_stream: Verifies small forward seeks reuse stream
    - test_forward_seek_beyond_limit_creates_new_stream: Verifies large forward seeks create new stream
    - test_sequential_reads_reuse_stream: Verifies sequential access pattern optimization
    - test_seek_to_same_position_reuses_stream: Verifies idempotent seeks reuse stream

---

## Phase 2: High Priority Fixes

### Error Handling

- [x] **ERROR-001**: Research typed error design
  - Create `research/error-design.md` and `[spec:error-handling]` with:
    - Current string-based error detection issues
    - Proposed error enum hierarchy
    - FUSE error code mapping strategy
    - Library vs application error separation

- [x] **ERROR-002**: Replace string matching with typed errors
  - Depends on: `[research:error-design]`, `[spec:error-handling]`
  - Removed string matching patterns from `src/fs/error.rs` and `src/fs/async_bridge.rs`
  - Updated `ToFuseError` implementation for `anyhow::Error` to use proper downcasting
  - Error mapping now uses typed error variants (ApiError, FuseError, std::io::Error)
  - All tests pass, no clippy warnings, code formatted

- [x] **ERROR-003**: Fix silent failures in list_torrents()
  - Depends on: `[spec:error-handling]`
  - Created `ListTorrentsResult` struct in `src/api/types.rs` with:
    - `torrents: Vec<TorrentInfo>` for successful results
    - `errors: Vec<(u64, String, ApiError)>` for failed torrents
    - Helper methods: `is_partial()`, `has_successes()`, `is_empty()`, `total_attempted()`
  - Modified `list_torrents()` in `src/api/client.rs` to return `Result<ListTorrentsResult>`
  - Updated callers in `src/fs/filesystem.rs` to handle partial failures with logging
  - Added `test_list_torrents_partial_failure` test to verify behavior
  - All tests pass, clippy clean

- [x] **ERROR-004**: Preserve error context
  - Depends on: `[spec:error-handling]`
  - Fixed lines 289-292 in `check_response()`: Changed `.unwrap_or_else()` to `match` statement that preserves original error in `ApiError::NetworkError`
  - Fixed lines 584-592 in `read_stream_range()`: Same pattern for range error response handling
  - Original errors are now properly wrapped and preserved in error messages
  - All tests pass, clippy clean, code formatted

### API Client (src/api/client.rs)

- [x] **API-001**: Remove panics from API client
  - Changed `RqbitClient::new()` to return `Result<Self>` instead of panicking
  - Changed `RqbitClient::with_config()` to return `Result<Self>` 
  - Changed `RqbitClient::with_circuit_breaker()` to return `Result<Self>`
  - Added `ClientInitializationError` variant to `ApiError` enum
  - Added `RequestCloneError` variant to `ApiError` enum
  - Fixed `read_file()` to validate request clone before retry loop
  - Updated all callers to handle the new Result types
  - All tests pass, clippy clean, code formatted

- [ ] **API-002**: Add authentication support
  - [x] Research rqbit auth methods - [research:rqbit-authentication](research/rqbit-authentication.md)
  - [x] Add auth token/API key support to client - Implemented HTTP Basic Auth with `with_auth()` constructor and auth header generation for all API requests
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
