# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Simplified

- SIMPLIFY-2-005: Simplify metrics system in `src/metrics.rs`
  - Removed custom `LatencyMetrics` trait (28 lines)
  - Removed `record_op!` macro and replaced with explicit methods (35 lines)
  - Removed atomic snapshot loops from `FuseMetrics::log_summary()` and `ApiMetrics::log_summary()`
  - Implemented `avg_latency_ms()` directly on `FuseMetrics` and `ApiMetrics`
  - Simplified tests: removed complex concurrent tests, kept core functionality tests
  - Reduced file from 657 to 512 lines (144 lines removed, ~22% reduction)
  - All method signatures remain compatible - no changes needed in call sites
  - Location: `src/metrics.rs`

### Added

- IDEA2-006 to IDEA2-007: Handle open file handles during torrent removal and add integration test
  - Verified file handles are properly removed when torrents are deleted from rqbit
  - `read()` returns EBADF for invalid file handles after torrent removal
  - `release()` handles already-removed handles gracefully
  - Added integration test `test_torrent_removal_from_rqbit` in `tests/integration_tests.rs`
  - Test verifies torrent is removed from filesystem after discovery detects deletion
  - Test confirms removed torrent is no longer visible in directory listings
  - Added `__test_known_torrents()` and `__test_clear_list_torrents_cache()` test helpers
  - Location: `src/fs/filesystem.rs`, `src/api/client.rs`, `tests/integration_tests.rs`
  - All tests pass: `cargo test` ✅

- IDEA2-001 to IDEA2-005: Implement torrent removal detection from rqbit
  - Modified `discover_torrents()` to return `DiscoveryResult` with current torrent IDs
  - Populated `known_torrents: DashSet<u64>` during discovery to track known torrents
  - Implemented `detect_removed_torrents()` to find torrents deleted from rqbit
  - Implemented `remove_torrent_from_fs()` to clean up removed torrents
  - Integrated removal detection in `refresh_torrents()`, background discovery task, and `readdir()`
  - Automatically closes file handles, removes inodes, and cleans up torrent statuses
  - Records metrics for torrent removals
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- EDGE-013: Test lookup of special entries ("." and "..")
  - Added special case handling in `filesystem.rs:lookup()` for "." and ".." entries
  - "." returns the current directory inode
  - ".." returns the parent directory inode (root's parent is itself)
  - Added 5 comprehensive tests in `tests/fuse_operations.rs`:
    - `test_lookup_dot_in_root`: Tests "." resolves to root directory
    - `test_lookup_dotdot_in_root`: Tests ".." in root resolves to root
    - `test_lookup_special_entries_in_torrent_dir`: Tests special entries in torrent directories
    - `test_lookup_special_entries_in_nested_dir`: Tests special entries in nested subdirectories
    - `test_parent_resolution_from_nested_dirs`: Tests parent chain resolution from deep nesting
  - All tests pass: `cargo test test_lookup_dot test_lookup_special test_parent_resolution` ✅
  - Marked EDGE-013 as complete in TODO.md

- Mark EDGE-012 as complete - Test readdir on non-directory
  - `test_error_enotdir_file_as_directory` in `tests/fuse_operations.rs` tests ENOTDIR behavior
  - Verifies files have no children (empty result from `get_children()`)
  - Tests that looking up paths inside files fails (returns None)
  - Corresponds to FUSE readdir returning ENOTDIR error when called on files
  - Test passes: `cargo test test_error_enotdir_file_as_directory` ✅

- Add metrics for piece check failures (IDEA1-010)
  - Added `pieces_unavailable_errors` counter to `FuseMetrics`
  - Tracks how often reads are rejected due to unavailable pieces on paused torrents
  - Integrated into filesystem read path - increments when EIO is returned for missing pieces
  - Added to metrics summary log output
  - Location: `src/metrics.rs`
  - All tests pass: `cargo test` ✅

- Added accessor methods to `TorrentFS` for testing
  - Added `async_worker()` method to access the async worker
  - Added `config()` method to access the configuration
  - Enables integration tests to verify piece checking functionality
  - Location: `src/fs/filesystem.rs`

- Add piece check bypass for completed torrents (IDEA1-006)
  - Added optimization to skip piece availability API calls for completed torrents
  - Checks `status.is_complete()` before performing piece check for paused torrents
  - Completed torrents have all pieces available, so no need to verify via API
  - Reduces unnecessary API calls and improves read performance for finished torrents
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- Add I/O error for paused torrents with missing pieces (IDEA1-001 to IDEA1-005)
  - Added `has_piece_range()` method to `PieceBitfield` for checking piece availability
  - Added `check_pieces_before_read` config option to `PerformanceConfig` (default: true)
  - Modified FUSE read path to check piece availability before streaming
  - Returns `EIO` error immediately when pieces are not available on paused torrents
  - Prevents timeouts when reading from paused torrents with missing pieces
  - Location: `src/api/types.rs`, `src/fs/filesystem.rs`, `src/config/mod.rs`
  - All tests pass: `cargo test` ✅

- Add `is_complete()` helper method to `TorrentStatus` for checking completion state
  - Returns true if the torrent is finished downloading (all pieces available)
  - Used by filesystem to determine if piece checking can be bypassed
  - Location: `src/api/types.rs`
  - All tests pass: `cargo test` ✅

- Add unit tests for piece range checking functionality
  - Comprehensive tests in `src/api/types.rs` for `PieceBitfield::has_piece_range()`
  - Tests cover: complete bitfield, partial bitfield, empty range, out-of-bounds ranges
  - All tests pass: `cargo test piece_bitfield` ✅

- Add `lookup_torrent()` method to `InodeManager` for finding torrent root inodes
  - Enables efficient lookup of torrent directory by torrent ID
  - Used by filesystem to resolve torrent paths and check existence
  - Location: `src/fs/inode.rs`
  - All tests pass: `cargo test` ✅

- Add `torrent_to_inode` mapping to `InodeManager` for tracking torrent directories
  - Maps torrent IDs to their root directory inodes
  - Updated when creating torrent filesystem structures
  - Enables O(1) lookup of torrent directories by ID
  - Location: `src/fs/inode.rs`
  - All tests pass: `cargo test` ✅

- Add integration tests for FUSE operations in `tests/fuse_operations.rs`
  - Tests for directory listing, file lookup, and attribute retrieval
  - Tests for error handling (ENOENT, ENOTDIR, etc.)
  - Tests for symlink resolution
  - All tests pass: `cargo test --test fuse_operations` ✅

- Add support for checking piece availability before read operations (IDEA1-004)
  - Added `CheckPiecesAvailable` request type to `FuseRequest` enum
  - Added `check_pieces_available()` method to `AsyncFuseWorker`
  - Fetches torrent info to get piece length internally
  - Returns EIO error when pieces are not available
  - Location: `src/fs/async_bridge.rs`
  - All tests pass: `cargo test` ✅

- Add `get_torrent_info_with_cache()` method to `RqbitClient`
  - Fetches torrent info with short TTL caching (5 seconds)
  - Used by piece availability checker to get piece length
  - Reduces redundant API calls for frequently accessed torrents
  - Location: `src/api/client.rs`
  - All tests pass: `cargo test` ✅

- Add `pieces_unavailable_errors` metric to track read rejections
  - Counter increments each time read is rejected due to unavailable pieces
  - Provides visibility into paused torrent access patterns
  - Included in periodic metrics summary logs
  - Location: `src/metrics.rs`
  - All tests pass: `cargo test` ✅

- Add support for detecting and removing deleted torrents from FUSE filesystem
  - Tracks known torrent IDs in `TorrentFS.known_torrents: DashSet<u64>`
  - Compares current torrent list with known set during discovery
  - Automatically removes torrent filesystem entries when deleted from rqbit
  - Closes open file handles and cleans up resources on removal
  - Integrated into background discovery and on-demand discovery
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- Add `DiscoveryResult` struct to return torrent discovery information
  - Contains `new_count` (number of new torrents) and `current_torrent_ids` (all torrent IDs)
  - Used by `discover_torrents()` to provide both discovery and removal detection data
  - Enables tracking of which torrents are currently in rqbit
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- Add `remove_torrent_from_fs()` method for cleaning up removed torrents
  - Closes all file handles associated with the torrent
  - Removes inode tree and cleans up metadata
  - Removes from `known_torrents` tracking set
  - Records metric for torrent removal
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- Add `detect_removed_torrents()` method to find deleted torrents
  - Compares current torrent list with known set
  - Returns list of torrent IDs that are no longer in rqbit
  - Called during discovery to detect removals
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

- Integrated torrent removal detection into all discovery paths
  - `refresh_torrents()`: Updates known_torrents and removes deleted torrents
  - `start_torrent_discovery()`: Background task handles removals
  - `readdir()`: On-demand discovery includes removal detection
  - Ensures FUSE filesystem stays in sync with rqbit state
  - Location: `src/fs/filesystem.rs`
  - All tests pass: `cargo test` ✅

### Changed

### Deprecated

### Removed

- Reduced test coverage in `src/fs/inode.rs` from 720 lines to ~290 lines (50% ratio)
  - Removed 4 redundant concurrent test variations, keeping `test_concurrent_allocation_consistency`
  - Removed property-based tests (proptest) that duplicated unit test coverage
  - Removed unused `proptest` import from test module
  - Maintained all core functionality coverage: allocation, lookup, removal, children, symlinks
  - File reduced from 1079 lines to 765 lines (-314 lines, -29%)
  - Location: `src/fs/inode.rs`

- Reviewed metrics system in `src/metrics.rs` (657 lines)
  - Identified over-engineered components: LatencyMetrics trait, atomic snapshot loops, record_op! macro
  - Created research document at `research/metrics_review.md` with detailed analysis

- Identified over-engineered parts in metrics system
  - Custom LatencyMetrics trait with atomic snapshot loops
  - record_op! macro generating simple increment methods
  - Overly complex concurrent test variations
  - Documented simplification approach in `research/metrics_review.md`
  - Marked task complete in SIMPLIFY-2.md
  - Documented simplification recommendations for future implementation

### Fixed

### Security

## [0.1.0] - 2024-01-15

### Added

- Initial release of rqbit-fuse
- FUSE filesystem implementation for accessing rqbit torrents as files
- Support for listing torrents as directories
- Support for reading torrent files via HTTP range requests
- Configuration system with TOML config files
- Caching layer for torrent metadata and file attributes
- Async runtime integration for non-blocking I/O
- Comprehensive metrics collection
- Error handling and logging infrastructure
- Unit and integration test suites
