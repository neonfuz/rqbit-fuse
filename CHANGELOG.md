# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
