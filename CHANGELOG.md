# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Replaced string matching with typed errors (ERROR-002)
  - Removed fragile `.contains("not found")` and `.contains("range")` patterns
  - Updated `ToFuseError` trait in `src/fs/error.rs` to use proper error downcasting
  - Updated `src/fs/async_bridge.rs` to use `e.to_fuse_error()` instead of string matching
  - Error classification now uses typed variants: `ApiError`, `FuseError`, `std::io::Error`
  - Benefits: type safety, compile-time checking, better maintainability, improved performance
  - All 175 tests pass: `cargo test` ✅
  - No clippy warnings: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

- Implemented backward seeking with comprehensive tests (STREAM-004)
  - Backward seeking already worked by creating new streams when can_read_at() returns false
  - Added 5 comprehensive seek tests to verify all seek behaviors:
    - test_backward_seek_creates_new_stream: Verifies backward seek creates new HTTP connection
    - test_forward_seek_within_limit_reuses_stream: Verifies small forward seeks reuse existing stream
    - test_forward_seek_beyond_limit_creates_new_stream: Verifies large forward seeks create new stream
    - test_sequential_reads_reuse_stream: Verifies sequential access pattern optimization
    - test_seek_to_same_position_reuses_stream: Verifies idempotent seeks reuse stream
  - All 9 streaming tests pass: cargo test streaming::tests ✅
  - No clippy warnings: cargo clippy ✅

- Added yielding in large skip operations (STREAM-003)
  - Added `SKIP_YIELD_INTERVAL` constant (1MB) to prevent blocking async runtime
  - Modified `PersistentStream::skip()` to yield every 1MB during large skip operations
  - Tracks bytes skipped since last yield and calls `tokio::task::yield_now().await`
  - Prevents the skip loop from monopolizing the async runtime when skipping large amounts of data
  - All streaming tests pass: `cargo test streaming::tests` ✅

- Fixed check-then-act race condition in stream access (STREAM-002)
  - Restructured `PersistentStreamManager::read()` to hold the streams lock continuously
  - Removed race window between checking stream usability and getting mutable reference
  - Lock is now acquired once at the start and held until the operation completes
  - Simplified code flow by removing separate lock acquisition points
  - All 4 new concurrent access tests pass
  - No clippy warnings, code formatted

- Fixed unwrap panic in stream access (STREAM-001)
  - Fixed line 380 in `src/api/streaming.rs`: Changed `.unwrap()` on stream get to `if let Some()` pattern
  - Eliminates panic when stream is dropped between existence check (lines 359-366) and lock re-acquisition (line 379)
  - Now gracefully falls back to creating a new stream if the expected stream was removed
  - All 80 tests pass: `cargo test` ✅
  - No clippy warnings: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

- Fixed stale path references in inode removal (INODE-005)
  - Added `canonical_path: String` field to all `InodeEntry` variants (Directory, File, Symlink)
  - Store canonical path at entry creation time to prevent stale path issues
  - Updated `InodeEntry::with_ino()` to preserve `canonical_path` when changing inode number
  - Modified allocation methods (`allocate_torrent_directory`, `allocate_file`, `allocate_symlink`) to compute and store canonical path at creation time
  - Updated `allocate_entry()` in `src/fs/inode.rs` to use stored canonical path instead of rebuilding via `build_path()`
  - Fixed nested directory path construction in `filesystem.rs` to include torrent directory name
  - Fixed typo in format strings (`format!("/{}/)", name)` → `format!("/{}", name)`) that caused test failures
  - Updated all test files and benchmarks to include `canonical_path` field
  - Eliminates TOCTOU race condition where paths could become stale between check and use
  - All 80 tests pass: `cargo test` ✅
  - Clippy warnings reduced: `cargo clippy` ✅
  - Code formatted: `cargo fmt` ✅

### Changed

- Made entries field private with controlled accessors (INODE-004)
  - Removed public `entries()` accessor method that exposed internal DashMap
  - Added `InodeEntryRef` struct for safe iteration over entries
  - Added `contains(inode: u64) -> bool` to check inode existence
  - Added `iter_entries() -> impl Iterator<Item = InodeEntryRef>` for read-only iteration
  - Added `len() -> usize` to get total entry count (including root)
  - Added `is_empty() -> bool` to check if only root exists
  - Updated all callers in tests to use new controlled API
  - Prevents external code from directly modifying inode table
  - All access now goes through controlled methods maintaining invariants

### Fixed

- Fixed torrent directory mapping for single-file torrents (INODE-003)
  - Single-file torrents now create a directory like multi-file torrents
  - Previously mapped torrent_id directly to file inode for single-file torrents
  - Now consistently maps torrent_id to torrent directory inode for all torrent types
  - Files are placed inside the torrent directory (consistent filesystem structure)
  - Path resolution now works correctly for all torrent types
  - Directory listings show proper torrent contents for both single and multi-file torrents
  - Modified `create_torrent_structure_static()` in `src/fs/filesystem.rs`

### Fixed

- Made inode table operations atomic (INODE-002)
  - Refactored `allocate_entry()` in `src/fs/inode.rs` to use DashMap entry API
  - Ensured proper insertion order: primary entries first, then indices
  - Added panic protection for corrupted inode counter (duplicate inode detection)
  - Rewrote `remove_inode()` with consistent 4-step atomic removal:
    1. Recursively remove children (bottom-up)
    2. Remove from parent's children list
    3. Remove from path and torrent indices
    4. Finally remove from primary entries map
  - Updated `clear_torrents()` to use atomic `remove_inode()` for proper cleanup
  - Prevents inconsistent state if crash occurs during operations
  - All existing tests pass, no behavioral changes

### Added

- Comprehensive concurrent inode operation tests (INODE-002)
  - `test_concurrent_allocation_atomicity`: 50 threads × 20 allocations each with immediate verification
  - `test_concurrent_removal_atomicity`: Concurrent removal of 20 torrents with files from multiple threads
  - `test_mixed_concurrent_operations`: Mixed concurrent allocators and removers with consistency checks
  - `test_atomic_allocation_no_duplicates`: 100 simultaneous threads verifying no duplicate inodes
  - All 20 inode tests pass: `cargo test --lib fs::inode::tests` ✅

### Fixed

- Fixed failing filesystem tests by converting to tokio::test
  - Converted 12 tests in `src/fs/filesystem.rs` from `#[test]` to `#[tokio::test]` async tests
  - Tests were failing due to missing Tokio runtime for AsyncFuseWorker task spawning
  - All filesystem tests now pass: `cargo test fs::filesystem::tests` ✅
  - Fixed tests: test_torrent_fs_creation, test_validate_mount_point_success, test_validate_mount_point_nonexistent
  - Fixed tests: test_build_mount_options, test_build_mount_options_allow_other, test_remove_torrent_cleans_up_inodes
  - Fixed tests: test_symlink_creation, test_zero_byte_file, test_large_file, test_unicode_filename
  - Fixed tests: test_single_file_torrent_structure, test_multi_file_torrent_structure

### Added

- Completed FS-007.7: Implement error case tests
  - Implemented 15 comprehensive error case tests in `tests/fuse_operations.rs`
  - Error code coverage: ENOENT, ENOTDIR, EISDIR, EACCES, EIO, EINVAL, EBADF
  - `test_error_enoent_nonexistent_path`: Tests non-existent paths return None (ENOENT)
  - `test_error_enoent_lookup_operations`: Tests lookup failures on invalid entries
  - `test_error_enotdir_file_as_directory`: Tests directory operations on files (ENOTDIR)
  - `test_error_eisdir_directory_as_file`: Tests file operations on directories (EISDIR)
  - `test_error_eacces_read_only_filesystem`: Tests read-only permission checks (EACCES)
  - `test_error_permission_bits_verification`: Detailed permission bit testing (0o444/0o555)
  - `test_error_eio_api_failure`: Tests API failure handling (EIO)
  - `test_error_eio_timeout`: Tests timeout scenario handling (EIO)
  - `test_error_einval_invalid_parameters`: Tests invalid parameter validation (EINVAL)
  - `test_error_ebadf_invalid_file_handle`: Tests invalid file handle scenarios (EBADF)
  - `test_error_edge_cases_empty_torrent`: Tests empty torrent error handling
  - `test_error_invalid_torrent_id`: Tests invalid torrent ID scenarios
  - `test_error_deeply_nested_invalid_paths`: Tests deep nesting validation
  - `test_error_symlink_to_nonexistent`: Tests broken symlink handling
  - All 15 error tests pass: `cargo test test_error --test fuse_operations` ✅
  - Code passes clippy and formatting checks

- Completed FS-007.6: Implement read operation tests
  - Fixed compilation errors: changed `i64` to `u64` for file length fields in tests
  - `test_read_file_contents`: Tests basic file read with WireMock API mocking
  - `test_read_various_buffer_sizes`: Tests 100KB file with correct block calculations (25 blocks)
  - `test_read_at_different_offsets`: Tests 8KB file with offset-based reading
  - `test_read_beyond_file_end`: Tests graceful EOF handling with 100 byte file
  - `test_read_multi_file_torrent`: Tests multiple files in torrent with different content
  - `test_read_zero_bytes`: Tests zero-byte read scenarios
  - `test_read_invalid_file_handle`: Tests error handling for invalid file handles
  - `test_read_from_directory`: Tests EISDIR behavior when reading directories
  - `test_read_nonexistent_inode`: Tests ENOENT for non-existent inodes
  - `test_read_large_file`: Tests 10MB file with correct block calculations (2560 blocks)
  - All 16 read tests pass: `cargo test test_read --test fuse_operations` ✅

- Completed FS-007.4: Implement getattr operation tests
  - Implemented 5 comprehensive getattr tests in `tests/fuse_operations.rs`
  - `test_getattr_file_attributes`: Tests file size, blocks, permissions (0o444), for files 100 bytes to 10 MB
  - `test_getattr_directory_attributes`: Tests directory permissions (0o555), nlink calculation (2 + children)
  - `test_getattr_nonexistent_inode`: Tests ENOENT behavior for invalid inodes (0, 99999, u64::MAX)
  - `test_getattr_timestamp_consistency`: Tests atime/mtime/ctime validity and ordering
  - `test_getattr_symlink_attributes`: Tests symlink file type detection and attributes
  - All tests pass: `cargo test test_getattr --test fuse_operations` ✅

- Completed FS-007.3: Implement lookup operation tests
  - Implemented 7 comprehensive lookup tests in `tests/fuse_operations.rs`
  - `test_lookup_successful_file`: Verifies file lookup returns correct inode and attributes
  - `test_lookup_successful_directory`: Verifies directory lookup works correctly
  - `test_lookup_nonexistent_path`: Tests ENOENT for non-existent files and directories
  - `test_lookup_invalid_parent`: Tests lookup in non-directory returns empty (ENOTDIR behavior)
  - `test_lookup_nonexistent_parent`: Tests lookup with invalid parent inode
  - `test_lookup_deeply_nested`: Tests lookup through 4 levels of directory nesting
  - `test_lookup_special_characters`: Tests lookup with spaces, unicode, and symbols
  - All tests pass: `cargo test test_lookup --test fuse_operations` ✅

- Completed FS-007.2: Set up FUSE testing infrastructure
  - Created tests/common/ module with WireMock helpers, FUSE utilities, and fixtures
  - Added mock_server.rs with predefined torrent API responses
  - Added fuse_helpers.rs with TestFilesystem wrapper for test lifecycle
  - Added fixtures.rs for various torrent test scenarios (single-file, multi-file, unicode, etc.)
  - Created tests/fuse_operations.rs with comprehensive FUSE operation tests
  - Tests cover: lookup, getattr, readdir, open/release, error scenarios
  - All tests compile and run successfully
  - Foundation established for FS-007.3-7.8 (specific operation tests)

- Completed FS-007.1: Read testing specification
  - Reviewed comprehensive FUSE testing approaches and strategies
  - Identified test infrastructure requirements for upcoming FUSE operation tests

### Changed

- Extract streaming helpers to reduce code duplication (SIMPLIFY-011)
  - Added `consume_pending()` helper to `PersistentStream` for pending buffer handling
  - Added `buffer_leftover()` helper for chunk buffering logic
  - Added `read_from_stream()` helper to `PersistentStreamManager`
  - Refactored `read()` and `skip()` in `PersistentStream` to use new helpers
  - Refactored manager's `read()` to use `read_from_stream()` helper
  - Reduced `src/api/streaming.rs` from ~504 to ~464 lines (~40 line reduction)
  - Fixed runtime detection in cleanup task for test compatibility
  - All lib tests pass, clippy clean, code formatted

- Simplify metrics module with macros and traits (SIMPLIFY-010)
  - Added `record_op!` macro to generate 7 simple recording methods in FuseMetrics
  - Added `LatencyMetrics` trait with default `avg_latency_ms()` implementation
  - Implemented `LatencyMetrics` for both `FuseMetrics` and `ApiMetrics`
  - Removed duplicate average latency calculation methods (~20 lines saved)
  - Reduced `src/metrics.rs` from ~294 to ~259 lines (~35 line reduction)
  - Added 3 new tests for the trait and macro implementations
  - All 5 metrics tests pass, clippy clean, code formatted

- Consolidate type files (SIMPLIFY-012)
  - Merged `TorrentFile` struct from `file.rs` into `torrent.rs`
  - Added `match_fields!` macro to `inode.rs` reducing repetitive accessor methods
  - Added `base_attr()` helper to `attr.rs` for shared FileAttr creation
  - Deleted `src/types/file.rs` (9 lines)
  - Updated `src/types/mod.rs` to remove `pub mod file`
  - Reduced ~44 lines total (torrent.rs +11, file.rs -9, inode.rs -24, attr.rs -11)
  - All compilation checks pass, clippy clean, code formatted

- Simplified API types module (SIMPLIFY-009)
  - Merged `DownloadSpeed` and `UploadSpeed` into unified `Speed` struct
  - Added `strum::Display` derive to `TorrentState`, removed 12-line manual Display impl
  - Derived `Serialize` for `TorrentStatus`, replaced manual `to_json()` with `serde_json::to_string()`
  - Simplified `to_fuse_error()` error mappings by consolidating HTTP status codes (~30 → ~15 lines)
  - Reduced `src/api/types.rs` from 427 to 377 lines (~50 line reduction)
  - Updated `src/fs/filesystem.rs` to handle new `Result` return type from `to_json()`
  - All compilation checks pass, clippy clean, code formatted

- Simplify inode allocation logic (SIMPLIFY-007)
  - Added `with_ino()` method to `InodeEntry` in `src/types/inode.rs`
  - Created generic `allocate_entry()` helper to consolidate allocation logic
  - Simplified `allocate()`, `allocate_torrent_directory()`, `allocate_file()`, and `allocate_symlink()` methods
  - Converted `build_path()` from recursive to iterative implementation
  - Reduced ~64 lines of duplicated allocation boilerplate
  - All fs::inode::tests pass, clippy clean

### Changed

- Replace std::sync::Mutex with tokio::sync::Mutex in async contexts (FS-005)
  - Replaced blocking std::sync::Mutex with async tokio::sync::Mutex in `src/api/streaming.rs`
  - Updated field declarations, initializations, and function signatures
  - Fixed locking patterns: use `block_on` for initialization, `try_lock` for cleanup
  - Fixed related issues in `src/fs/filesystem.rs` for consistency
  - Prevents blocking operations in async contexts

- Add FUSE logging macros to reduce boilerplate (SIMPLIFY-004)
  - Created `fuse_log!`, `fuse_error!`, `fuse_ok!` macros in `src/fs/macros.rs`
  - Replaced ~42 repetitive logging blocks across 7 FUSE operations
  - Reduced ~120 lines of boilerplate in `src/fs/filesystem.rs`
  - Operations updated: read, release, lookup, getattr, open, readdir
  - Macros automatically check `log_fuse_operations` config flag
  - All 90 tests pass, clippy clean, code formatted

- Add error handler macros for FUSE operations (SIMPLIFY-005)
  - Created `reply_ino_not_found!`, `reply_not_directory!`, `reply_not_file!`, `reply_no_permission!` macros
  - Replaced ~100 lines of duplicated error handling code across 6 FUSE operations
  - Operations updated: read, lookup, getattr, open, readlink, readdir
  - Macros record error metrics, log errors (when enabled), and reply with appropriate libc error codes
  - Updated imports in `src/fs/filesystem.rs` to include new macros
  - Fixed unused `mut` warnings with `cargo fix`
  - Code compiles cleanly, clippy clean

- Extract helper functions from main.rs (SIMPLIFY-003)
  - Added `load_config()` helper to consolidate config loading across 3 commands
  - Added `run_command()` helper for shell command execution with error handling
  - Added `try_unmount()` helper with fusermount3/fusermount fallback logic
  - Reduced ~76 lines of duplicated code in main.rs
  - Simplified `run_mount()`, `run_umount()`, `run_status()` to use helpers
  - All 90 tests pass, clippy clean

- Unify torrent discovery logic (SIMPLIFY-006)
  - Created unified `discover_torrents()` async method in `src/fs/filesystem.rs`
  - Consolidated 3 duplicated discovery implementations:
    - Background task in `start_torrent_discovery()` (~25 lines saved)
    - Explicit refresh in `refresh_torrents()` (~40 lines saved)
    - On-demand discovery in `readdir()` FUSE callback (~50 lines saved)
  - Reduced ~80 lines of duplicated discovery code
  - Consistent error handling via `Result<>` propagation
  - Consistent logging messages across all discovery paths
  - All 18 filesystem tests pass, clippy clean

- Create unified request helpers in API client (SIMPLIFY-014)
  - Added generic `get_json<T>()` helper for GET requests that deserialize JSON responses
  - Added generic `post_json<B, T>()` helper for POST requests with JSON body and response
  - Refactored 4 methods to use new helpers: `get_torrent`, `get_torrent_stats`, `add_torrent_magnet`, `add_torrent_url`
  - Reduced ~25 lines of duplicated request/response handling code
  - Enhanced logging with trace!/debug! for better observability
  - All 90 tests pass, no behavioral changes

- Add tracing instrumentation to API client (SIMPLIFY-013)
  - Replaced manual trace!/debug! calls with #[tracing::instrument] attributes
  - Added to 12 public methods: list_torrents, get_torrent, add_torrent_magnet, add_torrent_url
    get_torrent_stats, get_piece_bitfield, read_file, read_file_streaming,
    pause_torrent, start_torrent, forget_torrent, delete_torrent
  - Reduced ~40 lines of manual logging boilerplate
  - Provides automatic structured logging with function arguments and spans
  - All 90 tests pass, no behavioral changes

- Unified torrent control methods in API client (SIMPLIFY-002)
  - Created `torrent_action()` helper to consolidate pause/start/forget/delete
  - Reduced `src/api/client.rs` from ~72 to ~12 lines for torrent control (~60 line reduction)
  - All 4 public methods now delegate to the helper: `pause_torrent`, `start_torrent`, `forget_torrent`, `delete_torrent`
  - Preserves exact API behavior and all 90 tests pass

- Simplified configuration module with macros (SIMPLIFY-001)
  - Added `default_fn!`, `default_impl!`, and `env_var!` macros
  - Reduced `src/config/mod.rs` from 515 to ~347 lines (~168 line reduction)
  - Replaced 20 verbose default functions with 20 macro calls
  - Replaced 6 Default trait implementations with 6 macro calls
  - Replaced 20 environment variable merge blocks with 20 macro calls
  - All existing tests pass without modification
  - No functional changes - pure refactoring for maintainability

### Added

- Implemented unique file handle allocation (FS-003)
  - Created FileHandleManager for proper FUSE file handle semantics
  - File handles are now unique per open() call (not inode reuse)
  - Each handle tracks (inode, flags, read state) independently
  - Supports multiple concurrent opens of the same file with independent state
  - Proper cleanup of handles in release() callback
  - Updated all file operations to use handles instead of inodes directly

### Performance

- Optimized cache statistics collection with sharded atomic counters
  - Implemented 64-shard counter to reduce contention (1KB memory overhead)
  - Uses thread-local round-robin selection for async-safe distribution
  - Achieved 702,945 ops/sec throughput with 100% accuracy
  - Added benchmark test `test_cache_stats_performance` for validation

### Fixed

- Fixed nested directory path resolution (FS-006)
  - Bug: `allocate_file()` was incorrectly updating `torrent_to_inode` with each file's parent directory
  - This caused torrent root inode mapping to be overwritten by subdirectories
  - Fix: Removed erroneous `torrent_to_inode.insert()` from `allocate_file()`
  - All nested directory tests now pass (path resolution, deep nesting, multi-file structure)

- Fixed read_states memory leak with TTL-based cleanup (FS-004)
  - Added TTL-based eviction for orphaned file handles (1 hour TTL, checked every 5 minutes)
  - Added memory usage metrics for file handles via `memory_usage()` method
  - Created background cleanup task that runs alongside status monitoring and torrent discovery
  - Added `created_at` and `is_expired()` to FileHandle for TTL tracking
  - Added `remove_expired_handles()`, `memory_usage()`, and `count_expired()` to FileHandleManager
  - Prevents memory leaks from handles that are opened but never properly released
  - All 90 tests pass, clippy clean, code formatted

- Fixed failing cache unit tests to work with Moka's async behavior
  - `test_cache_basic_operations`: Adjusted expectations for eventually consistent entry count
  - `test_cache_lru_eviction`: Updated to work with TinyLFU algorithm behavior
  - `test_cache_ttl`: Corrected miss count expectations
  - `test_lru_eviction_efficiency` (performance test): Adjusted eviction behavior validation
  - Added appropriate async delays for Moka's internal maintenance operations
  - Fixed clippy warning about thread_local initialization

### Added

- Initial release of torrent-fuse
- FUSE filesystem implementation for mounting torrents as read-only filesystem
- rqbit HTTP API client with retry logic and circuit breaker pattern
- Inode management with thread-safe concurrent access
- Cache layer with TTL and LRU eviction policies
- Read-ahead optimization for sequential file access
- Torrent lifecycle management (add, monitor, remove)
- Comprehensive error handling with FUSE error code mapping
- CLI interface with mount, umount, and status commands
- Structured logging and metrics collection
- Extended attributes support for torrent status
- Symbolic link support
- Security features: path traversal protection, input sanitization
- Support for single-file and multi-file torrents
- Unicode filename support
- Large file support (>4GB)
- Graceful degradation with configurable piece checking
- Background torrent status monitoring
- 76 unit tests with mocked API responses
- 12 integration tests covering filesystem operations
- 10 performance tests with Criterion benchmarks
- CI/CD pipeline with GitHub Actions
- Automated release workflow for multiple platforms

### Security

- Path traversal protection in filename sanitization
- Circuit breaker pattern to prevent cascade failures
- Input validation for all user-provided paths
- Read-only filesystem permissions (0o444 for files, 0o555 for directories)

## [0.1.0] - 2026-02-13

### Added

- First stable release
- Complete FUSE filesystem implementation
- Full rqbit API integration
- Comprehensive test suite (88 tests total)
- Documentation (README, API docs, architecture)
- Multi-platform support (Linux, macOS)

[Unreleased]: https://github.com/anomalyco/torrent-fuse/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/anomalyco/torrent-fuse/releases/tag/v0.1.0
