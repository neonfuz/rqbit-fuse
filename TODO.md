# TODO.md - rqbit-fuse Code Reduction Roadmap

Based on code review in `review-r4.md`, this roadmap reduces the codebase by ~70% while preserving core functionality.

---

## Phase 1: High Priority Removals (Immediate Impact)

These changes have the highest impact with lowest risk. Remove unused/abandoned features.

### 1.1 Remove Status Monitoring Background Task
- [x] **Task 1.1.1**: Research - Document current status monitoring implementation
  - See research/status-monitoring-analysis.md
  - **Key Finding**: Status monitoring provides NO critical functionality. All piece availability checking uses API client's separate cache. Safe to remove.
  
- [x] **Task 1.1.2**: Remove `start_status_monitoring()` method
  - Delete method from `src/fs/filesystem.rs`
  - Remove call to this method in filesystem initialization
  - Remove `monitor_handle` field from TorrentFS struct
  - Remove `stop_status_monitoring()` method
  
- [x] **Task 1.1.3**: Remove `TorrentStatus` and related types if unused
  - Check if `torrent_statuses: Arc<DashMap<u64, TorrentStatus>>` is used elsewhere
  - If only used by monitoring task, remove field and imports
  - Update struct initialization
  - **Completed**: Removed `torrent_statuses` field and all related code from filesystem.rs
  - Simplified `check_pieces_available()` method since status monitoring is no longer used
  - Removed early EAGAIN checks in read handler that depended on status cache
  - Updated `getxattr` to return ENOATTR since status monitoring removed
  - Removed `monitor_torrent()` and `unmonitor_torrent()` calls
  - Removed status-related imports (TorrentStatus, DashMap)

### 1.2 Remove Mount Info Display Feature
- [x] **Task 1.2.1**: Remove `get_mount_info()` function
  - Delete from `src/mount.rs` lines 106-143
  - Remove `MountInfo` struct
  - Remove import of `MountInfo` from `src/main.rs`
  - **Completed**: Removed MountInfo struct and get_mount_info() function from src/mount.rs
  - **Completed**: Updated src/main.rs to remove get_mount_info import
  - **Completed**: Updated run_status() text output to remove filesystem/size/used/available fields
  - **Completed**: Updated run_status() JSON output to remove MountInfoOutput struct and mount_info field
  
- [x] **Task 1.2.2**: Update Status command to remove mount info
  - Modify `run_status()` in `src/main.rs` to remove mount info display
  - Remove filesystem, size, used, available fields from text output
  - Keep only "MOUNTED" / "NOT MOUNTED" status
  - **Completed**: Removed `OutputFormat` enum (Text/Json) from `src/main.rs`
  - **Completed**: Removed `--format` CLI argument from Status subcommand
  - **Completed**: Removed JSON serialization structs (`StatusOutput`, `ConfigOutput`)
  - **Completed**: Simplified `run_status()` to only output text format
  - **Completed**: Status command now only shows "MOUNTED" / "NOT MOUNTED"

### 1.3 Remove JSON Status Output
- [x] **Task 1.3.1**: Remove JSON output format from Status command
  - **Completed**: JSON output format already removed as part of Task 1.2.2
  - `OutputFormat` enum removed from `src/main.rs`
  - `--format` CLI argument removed from Status subcommand
  - JSON serialization structs (`StatusOutput`, `ConfigOutput`, `MountInfoOutput`) removed
  - Only text format output remains
  
- [x] **Task 1.3.2**: Update CLI to remove format option
  - **Completed**: No format parameter exists in `Commands::Status` variant
  - `run_status()` has no format parameter

### 1.4 Remove DiscoveryResult Struct
- [x] **Task 1.4.1**: Replace DiscoveryResult with Vec<u64>
  - Changed `discover_torrents()` return type from `Result<DiscoveryResult>` to `Result<Vec<u64>>`
  - Updated all call sites to handle Vec<u64> directly
  - Removed `DiscoveryResult` struct definition and `#[allow(dead_code)]` attribute
  - Removed unused `new_count` variable and related logging

---

## Phase 2: Configuration Simplification

Reduce configuration complexity and validation overhead.

### 2.1 Simplify Configuration Validation
- [x] **Task 2.1.1**: Research - Document validation rules to keep
  - **Completed**: Analyzed 33 validation rules across 7 methods
  - **Key Finding**: 19 rules are essential, 14 rules are arbitrary upper bounds
  - **See**: `research/config-validation-analysis.md` for complete analysis
  - **Essential rules**: Non-empty URL, parseability, positive numbers, absolute paths, valid log levels
  - **Arbitrary rules**: All upper bounds (TTL < 86400, max_entries < 1M, etc.)
  
- [x] **Task 2.1.2**: Remove arbitrary upper bound validations
  - Remove max TTL checks (86400 limit)
  - Remove max entries checks (1,000,000 limit)
  - Remove max concurrent reads checks (1000 limit)
  - Remove readahead size checks (1GB limit)
  - Remove stalled timeout checks (86400 limit)
  - Remove metrics interval checks
  - Remove resource limit checks (10GB, 1000 streams, 10M inodes)
  
- [x] **Task 2.1.3**: Simplify URL validation
  - **Completed**: URL validation already simplified - only checks non-empty and parseable
  - `reqwest::Url::parse()` accepts any valid URL scheme (http, https, ftp, etc.)
  - No explicit scheme restriction exists in the code
  - Test on line 1118 confirms non-http schemes are accepted
  
- [x] **Task 2.1.4**: Consolidate validation methods
  - Merged 7 separate validation methods into single `validate()` method
  - Removed per-field validation methods (validate_api_config, validate_cache_config, validate_mount_config, validate_performance_config, validate_monitoring_config, validate_logging_config, validate_resources_config)
  - Consolidated essential validations: URL non-empty/parseable, mount point absolute, log level valid
  - Removed redundant >0 validations for fields with defaults
  - Removed UID/GID bounds checks (enforced by u32 type)
  - Removed mount point existence check (may not exist at config time)
  - Removed 6 obsolete tests from src/config/mod.rs
  - Removed 5 obsolete tests from tests/config_tests.rs
  - Reduced validation code from ~183 lines to ~25 lines (-86%)
  - All 185+ tests passing
  - **Completed**: See CHANGELOG.md SIMPLIFY-017

### 2.2 Reduce Configuration Surface Area
- [x] **Task 2.2.1**: Research - Identify config fields to remove
  - Analyze all 27 config fields in `src/config/mod.rs`
  - Document which fields are commonly changed vs never used
  - Write findings to `research/config-fields-usage.md`
  - Reference: "See research/config-fields-usage.md"
  
- [x] **Task 2.2.2**: Remove MountConfig options
  - Remove `allow_other` field - always use default (false)
  - Remove `auto_unmount` field - always use default (true)
  - Remove `uid` and `gid` fields - always use current user
  - Keep only: `mount_point`
  - **Completed**: Removed 4 fields from MountConfig (allow_other, auto_unmount, uid, gid)
  - **Completed**: Removed `--allow-other` and `--auto-unmount` CLI args from Mount command
  - **Completed**: Removed env var parsing for TORRENT_FUSE_ALLOW_OTHER and TORRENT_FUSE_AUTO_UNMOUNT
  - **Completed**: Updated filesystem.rs to hardcode AutoUnmount and use libc::geteuid/getegid directly
  - **Completed**: Updated all test files to remove references to removed fields
  - **Completed**: All 185+ tests passing
  
- [x] **Task 2.2.3**: Remove PerformanceConfig options
  - Remove `prefetch_enabled` - feature doesn't work well
  - Remove `check_pieces_before_read` - always check
  - Keep only: `read_timeout`, `max_concurrent_reads`, `readahead_size`
  - **Completed**: See CHANGELOG.md SIMPLIFY-020
  
- [x] **Task 2.2.4**: Remove MonitoringConfig
  - Remove entire `MonitoringConfig` struct
  - Remove `status_poll_interval` field
  - Remove `stalled_timeout` field
  - Remove related env var parsing
  - **Completed**: Removed `MonitoringConfig` struct and all related code from `src/config/mod.rs`
  - **Completed**: Removed `monitoring` field from `Config` struct
  - **Completed**: Removed `impl Default for MonitoringConfig`
  - **Completed**: Removed environment variable parsing for `TORRENT_FUSE_STATUS_POLL_INTERVAL` and `TORRENT_FUSE_STALLED_TIMEOUT`
  - **Completed**: Updated all documentation and examples to remove monitoring section
  - **Completed**: All 346 tests passing with zero clippy warnings
  
- [x] **Task 2.2.5**: Remove LoggingConfig options
  - Remove `log_fuse_operations` - always log at debug level
  - Remove `log_api_calls` - always log at debug level
  - Remove `metrics_enabled` - removing metrics system
  - Remove `metrics_interval_secs` - removing metrics system
  - Keep only: `level`
  
- [x] **Task 2.2.6**: Remove ResourceLimitsConfig
  - **Completed**: Removed entire `ResourceLimitsConfig` struct from `src/config/mod.rs`
  - **Completed**: Removed `resources` field from `Config` struct
  - **Completed**: Removed 3 fields (`max_cache_bytes`, `max_open_streams`, `max_inodes`)
  - **Completed**: Removed `impl Default for ResourceLimitsConfig`
  - **Completed**: Removed environment variable parsing for `TORRENT_FUSE_MAX_CACHE_BYTES`, `TORRENT_FUSE_MAX_OPEN_STREAMS`, `TORRENT_FUSE_MAX_INODES`
  - **Completed**: Updated `src/fs/filesystem.rs` to use hardcoded max_inodes (100000)
  - **Completed**: All 346+ tests passing with zero clippy warnings
  - **See**: CHANGELOG.md SIMPLIFY-023
  
- [x] **Task 2.2.7**: Simplify CacheConfig
  - Keep only: `metadata_ttl`, `max_entries`
  - Remove: `torrent_list_ttl`, `piece_ttl` (use metadata_ttl for all)
  - Update all usages to use simplified config
  - Removed 2 fields from CacheConfig (torrent_list_ttl, piece_ttl)
  - Removed environment variable parsing for TORRENT_FUSE_TORRENT_LIST_TTL and TORRENT_FUSE_PIECE_TTL
  - Updated doc comments and examples to reflect simplified config
  - Updated test_json_config_parsing to use max_entries instead of piece_ttl
  - Reduced CacheConfig from 4 fields to 2 fields (-50%)
  - All 346+ tests passing

### 2.3 Update CLI Arguments
- [x] **Task 2.3.1**: Remove CLI args for removed config options
  - Remove `--allow-other` from Mount command
  - Remove `--auto-unmount` from Mount command
  - Remove any other args corresponding to removed config fields
  - **Completed**: Already done as part of Task 2.2.2
  
- [x] **Task 2.3.2**: Update env var parsing
  - Remove env var parsing for all removed config fields
  - Keep only: API_URL, MOUNT_POINT, METADATA_TTL, MAX_ENTRIES, READ_TIMEOUT, LOG_LEVEL, AUTH credentials
  - **Completed**: Removed TORRENT_FUSE_MAX_CONCURRENT_READS and TORRENT_FUSE_READAHEAD_SIZE env var parsing
  - **Completed**: Updated documentation to reflect remaining 9 essential env vars
  - **Completed**: Updated test to remove references to removed env vars
  - **See**: CHANGELOG.md SIMPLIFY-025

---

## Phase 3: Error System Simplification

Reduce 28 error types to 8 essential types.

### 3.1 Research Error Usage Patterns
- [x] **Task 3.1.1**: Research - Document error type usage
  - Search for all usages of each RqbitFuseError variant
  - Group variants by how they're handled (mapped to errno)
  - Write findings to `research/error-usage-analysis.md`
  - Identify which variants can be merged
  - **See research/error-usage-analysis.md**
  - **Key Finding**: 32 error variants can be consolidated to 8 essential variants (75% reduction)
  - **Key Finding**: Only 11 distinct errno mappings needed
  - **Categories**: NotFound, PermissionDenied, TimedOut, NetworkError, ApiError, IoError, InvalidArgument, NotReady, IsDirectory, NotDirectory

### 3.2 Consolidate Error Types
- [x] **Task 3.2.1**: Create simplified error enum in src/error.rs
  - Define new minimal RqbitFuseError with 11 variants (see research/error-usage-analysis.md)
  - New variants: NotFound(String), PermissionDenied(String), TimedOut(String), NetworkError(String), ApiError{status, message}, IoError(String), InvalidArgument(String), ValidationError(Vec<ValidationIssue>), NotReady(String), ParseError(String), IsDirectory, NotDirectory
  - Update to_errno() method for new variants
  - Update is_transient() method for new variants
  - Update is_server_unavailable() method for new variants
  - Update From<std::io::Error> implementation
  - Update From<reqwest::Error> implementation
  - Update From<serde_json::Error> implementation (merge with ParseError)
  - Update From<toml::de::Error> implementation (merge with ParseError)
  - Update tests to use new variants

- [x] **Task 3.2.2**: Update error usage in src/config/mod.rs
  - Replace RqbitFuseError::ReadError with IoError
  - Replace RqbitFuseError::ParseError with ParseError
  - Replace RqbitFuseError::InvalidValue with InvalidArgument

- [x] **Task 3.2.3**: Update error usage in src/api/client.rs
  - Replace ClientInitializationError with IoError
  - Replace RetryLimitExceeded with NotReady
  - Replace AuthenticationError with PermissionDenied
  - Replace HttpError with IoError
  - Replace TorrentNotFound with NotFound
  - Replace FileNotFound with NotFound
  - Replace InvalidRange with InvalidArgument
  - Replace RequestCloneError with IoError
  - Replace ServerDisconnected with NetworkError
  - Replace ConnectionTimeout with TimedOut
  - Replace ReadTimeout with TimedOut
  - Replace CircuitBreakerOpen with NetworkError
  - Replace ServiceUnavailable with NetworkError
  - Update tests to use new variants

- [x] **Task 3.2.4**: Update error usage in src/api/streaming.rs
  - Replace HttpError with IoError

- [x] **Task 3.2.5**: Update error usage in src/fs/async_bridge.rs
  - Replace TimedOut (keep but use with String context if needed)
  - Replace WorkerDisconnected with IoError
  - Replace ChannelFull with IoError

---

## Phase 4: Metrics System Reduction

Replace 520 lines of metrics with 3 essential metrics.

### 4.1 Research Metrics Usage
- [x] **Task 4.1.1**: Research - Document metrics usage
  - Search for all calls to metrics recording methods
  - Identify which metrics are actually logged/displayed
  - Write findings to `research/metrics-usage-analysis.md`
  - Determine which 3 metrics are most valuable
  - **Reference**: `research/metrics-usage-analysis.md`
  - **Key Finding**: Only 4 metrics provide value: bytes_read, error_count, cache_hits, cache_misses
  - **Key Finding**: 520 lines of metrics code can be reduced to ~50 lines (-90%)
  - **Key Finding**: CacheMetrics is never populated - Cache has its own internal stats
  - **Commit**: f859d4d

### 4.2 Simplify Metrics System
- [x] **Task 4.2.1**: Create minimal metrics struct
  - Define new `Metrics` struct with only: bytes_read, error_count, cache_hits, cache_misses
  - Remove FuseMetrics, ApiMetrics, CacheMetrics structs
  - Keep only atomic counters, remove all helper methods
  - **Commit**: c970b3d
  
- [x] **Task 4.2.2**: Remove metrics recording calls
  - **Completed**: Verified metrics system already uses only 4 essential counters
  - **Completed**: Fixed test compilation errors in src/api/client.rs (removed obsolete metrics verification)
  - **Completed**: Fixed test compilation errors in src/fs/filesystem.rs (added missing metrics parameter)
  - **Completed**: Removed unused metrics variable warnings in filesystem.rs
  - **Completed**: All tests passing (180+ tests)
  - **Completed**: Zero clippy warnings
  - **Note**: The simplified Metrics struct already only has record_read, record_error, record_cache_hit, record_cache_miss
  - **Note**: No calls to record_getattr, record_setattr, record_lookup, etc. exist in the codebase
  
- [x] **Task 4.2.3**: Remove periodic logging
  - Removed `spawn_periodic_logging()` method (was never called)
  - Removed `log_periodic()` method (was never called)
  - Removed `log_full_summary()` method (was never called)
  - Kept only simple `log_summary()` method called on shutdown
  - No code changes required - methods were already removed in previous simplification
  - See CHANGELOG.md SIMPLIFY-031
  
- [x] **Task 4.2.4**: Update all metrics usages
  - Fixed test compilation errors in `src/api/client.rs`
  - Added missing `metrics` parameter (None) to all `RqbitClient::with_config()` test calls
  - All 180+ tests passing with zero clippy warnings
  - Metrics system already simplified to 4 essential counters

---

## Phase 5: File Handle Tracking Simplification

Remove prefetching code and state tracking.

### 5.1 Remove Prefetching Infrastructure
- [x] **Task 5.1.1**: Research - Document prefetching code
  - **Completed**: Analyzed `src/types/handle.rs` and related files
  - **Key Finding**: Prefetching infrastructure is DEAD CODE - never used in production
  - **Key Finding**: Only test code calls `set_prefetching()`, `is_prefetching()`, `update_state()`
  - **Key Finding**: No filesystem integration exists for prefetching
  - **Key Finding**: `prefetch_enabled` config option already removed in Task 2.2.3
  - **See**: `research/prefetching-analysis.md` for complete analysis
  - **Safe to remove**: ~145 lines of dead code with ZERO risk
  
- [x] **Task 5.1.2**: Remove FileHandleState struct
  - Deleted `FileHandleState` struct and all sequential tracking infrastructure from `src/types/handle.rs`
  - Removed `state: Option<FileHandleState>` field from FileHandle
  - Removed all state-related methods: `init_state()`, `update_state()`, `is_sequential()`, `sequential_count()`
  - Removed prefetching methods: `set_prefetching()`, `is_prefetching()`
  - Removed TTL-based expiration: `is_expired()`, `created_at` field
  - Removed `FileHandleManager` methods: `update_state()`, `set_prefetching()`, `remove_expired_handles()`, `memory_usage()`, `count_expired()`
  - Removed handle cleanup background task from `src/fs/filesystem.rs`
  - Deleted cleanup_handle field and related methods: `start_handle_cleanup()`, `stop_handle_cleanup()`
  - Removed 5 obsolete tests: `test_file_handle_state_tracking`, `test_prefetching_state`, `test_handle_ttl_expiration`, `test_handle_ttl_with_multiple_handles`, `test_handle_is_expired_method`
  - Code reduction: ~470 lines removed from handle.rs (734 → 264 lines, -64%)
  - All 175 tests passing with zero clippy warnings
  - See CHANGELOG.md SIMPLIFY-034
  
- [x] **Task 5.1.3**: Remove FileHandleManager state methods
  - **Completed**: Removed `update_state()` method
  - **Completed**: Removed `set_prefetching()` method
  - **Completed**: Removed all methods that operated on state (already done in Task 5.1.2)
  
- [x] **Task 5.1.4**: Remove prefetching from filesystem
  - **Completed**: No `track_and_prefetch()` or `do_prefetch()` methods exist in filesystem
  - **Completed**: No prefetch-related code to remove
  - **Note**: Prefetching was never integrated into the filesystem layer

### 5.2 Simplify File Handle Cleanup
- [x] **Task 5.2.1**: Remove TTL-based cleanup
  - **Completed**: Removed `created_at` field from FileHandle
  - **Completed**: Removed `is_expired()` method
  - **Completed**: Removed `remove_expired_handles()` from FileHandleManager
  - **Completed**: Removed `count_expired()` method
  - **Completed**: Removed `start_handle_cleanup()` from filesystem
  - **Completed**: Removed `stop_handle_cleanup()` from filesystem
  - **Completed**: Removed cleanup_handle field from TorrentFS
  - All changes completed as part of Task 5.1.2
  
- [x] **Task 5.2.2**: Remove memory tracking
  - **Completed**: Already removed as part of SIMPLIFY-034 (Task 5.1.2)
  - The `memory_usage()` method was removed from FileHandleManager along with other state methods
  - No remaining memory tracking code exists in FileHandleManager
  
- [x] **Task 5.2.3**: Simplify FileHandle struct
  - **Completed**: FileHandle struct already has only 4 essential fields
  - Current fields: fh, inode, torrent_id, flags
  - Only method: new() constructor
  - Struct is already minimal with no unnecessary fields or methods

---

## Phase 6: FUSE Logging Simplification

Replace macros with direct tracing calls.

### 6.1 Replace FUSE Macros
- [x] **Task 6.1.1**: Research - Document macro usage
  - **Completed**: Found 33 macro call sites across 4 macro types
  - **Summary**: 7 fuse_log!, 8 fuse_error!, 7 fuse_ok!, 11 reply_* macros
  - **See**: `research/fuse-macro-usage.md` for complete analysis
  - **Key Finding**: All macros are simple wrappers around tracing::debug! - safe to replace
  - **Key Finding**: 98 lines in macros.rs can be removed entirely
  - **Estimated replacement effort**: ~45 minutes for all call sites
  
- [x] **Task 6.1.2**: Replace fuse_log! macro
  - Replaced all 7 `fuse_log!` calls with direct `tracing::debug!` calls in filesystem.rs
  - All operation start logging now uses direct tracing calls (e.g., `tracing::debug!(fuse_op = "read", ...)`)
  - Tracing handles filtering automatically, no conditional check needed
  - Macro definition remains in macros.rs for other macros (will be removed in Task 6.1.6)
  
- [x] **Task 6.1.3**: Replace fuse_error! macro
  - Replaced 2 direct `fuse_error!` calls with `tracing::debug!` in filesystem.rs
  - Line 1172 (symlink check): `tracing::debug!(fuse_op = "open", result = "error", error = "ELOOP")`
  - Line 1199 (handle limit): `tracing::debug!(fuse_op = "open", result = "error", error = "EMFILE", reason = "handle_limit_reached")`
  - reply_* macros will be replaced in Task 6.1.5
  - All tests passing
  
- [x] **Task 6.1.4**: Replace fuse_ok! macro
  - Replaced all 7 `fuse_ok!` calls with `tracing::debug!` in filesystem.rs
  - Each call now includes `result = "success"` field
  - Removed unused `fuse_ok` import from filesystem.rs
  - All 346 tests passing with zero clippy warnings
  - See CHANGELOG.md SIMPLIFY-035
  
- [x] **Task 6.1.5**: Replace reply_* macros
  - **Completed**: Reply macros were already replaced with direct code in previous tasks
  - Verified all reply_* macros (`reply_ino_not_found!`, `reply_not_directory!`, `reply_not_file!`, `reply_no_permission!`) are no longer used
  - All error handling now uses direct `self.metrics.record_error()`, `tracing::debug!()`, and `reply.error()` calls
  - Deleted unused `src/fs/macros.rs` file (was not imported anywhere)
  - All tests passing, zero clippy warnings
  
- [x] **Task 6.1.6**: Remove macro definitions
  - **Completed**: `src/fs/macros.rs` file already deleted in SIMPLIFY-039
  - **Completed**: No `mod macros;` declaration found in `src/fs/mod.rs`
  - **Completed**: No macro imports found in `src/fs/filesystem.rs`
  - **Completed**: All FUSE macros (fuse_log!, fuse_error!, fuse_ok!, reply_* macros) fully removed
  - All tests passing with zero clippy warnings

---

## Phase 7: Cache Layer Simplification

Remove redundant caching and simplify cache implementation.

### 7.1 Remove Bitfield Cache
- [x] **Task 7.1.1**: Research - Document bitfield cache usage
  - Analyze `status_bitfield_cache` in `src/api/client.rs`
  - Check if it's actually providing value vs fetching fresh
  - Write findings to `research/bitfield-cache-analysis.md`
  - **Reference**: See `research/bitfield-cache-analysis.md`
  - **Key Finding**: Bitfield cache provides minimal value due to 5-second TTL and adds complexity/memory leak risk. Safe to remove.
  
- [x] **Task 7.1.2**: Remove bitfield cache fields
  - Removed `status_bitfield_cache` field from RqbitClient
  - Removed `status_bitfield_cache_ttl` field
  - Removed `TorrentStatusWithBitfield` struct (no longer needed)
  - Simplified `check_range_available()` to fetch bitfield directly without caching
  - Removed `get_torrent_status_with_bitfield()` method
  - Removed unused `HashMap` import
  - All 180+ tests passing with zero clippy warnings
  
- [x] **Task 7.1.3**: Update get_torrent_status_with_bitfield
  - **Completed**: Method was removed in Task 7.1.2 (was only used for caching)
  - No call sites to update - method had no external callers
  
- [x] **Task 7.1.4**: Update check_range_available
  - **Completed**: Already updated in Task 7.1.2 to fetch bitfield directly
  - Uses `self.get_piece_bitfield(torrent_id).await?` at line 563
  - No synchronous fetching needed - works correctly with async bitfield fetch

### 7.2 Simplify Cache Implementation
- [x] **Task 7.2.1**: Research - Document cache wrapper value
  - **Completed**: Cache wrapper is DEAD CODE - never used in production
  - **Finding**: RqbitClient uses its own ad-hoc cache instead
  - **Finding**: All 68 Cache usages are only in cache.rs tests
  - **Recommendation**: Remove entire module (~1214 lines)
  - **See**: `research/cache-wrapper-analysis.md`
  
- [x] **Task 7.2.2**: Remove Cache module
  - **Completed**: Deleted `src/cache.rs` (~1214 lines)
  - **Completed**: Removed `pub mod cache` and `pub use cache` from `src/lib.rs`
  - **Completed**: Removed cache tests from `tests/performance_tests.rs`
  - **Completed**: All tests passing (zero cache-related tests remain)
  - **Completed**: Zero clippy warnings
  - **Code reduction**: -1214 lines (-22% of codebase)
  - **See**: `research/cache-wrapper-analysis.md` for rationale

---

## Phase 8: Test Suite Trimming

Remove tests that verify external crate behavior.

### 8.1 Trim Cache Tests
- [ ] **Task 8.1.1**: Identify tests to remove
  - Review all tests in `src/cache.rs` lines 221-1214
  - Mark tests that verify moka behavior vs our code
  - Keep only: basic operations, one TTL test, one concurrent test
  
- [ ] **Task 8.1.2**: Remove redundant cache tests
  - Remove LRU eviction tests (moka's responsibility)
  - Remove edge case tests (expiration during access, race conditions)
  - Remove performance tests (hardware dependent)
  - Remove memory limit tests
  - Keep ~200 lines of tests

### 8.2 Trim Streaming Tests
- [x] **Task 8.2.1**: Identify tests to remove
  - Reviewed all 33 tests in `src/api/streaming.rs` (lines 593-1984)
  - Marked 28 tests for removal that verify reqwest/wiremock behavior
  - Keeping only 5 essential tests: basic stream, 200-vs-206, concurrent access, invalid stream, normal response
  - **See**: `research/streaming-tests-analysis.md` for complete analysis
  - **Key Finding**: 85% of streaming tests can be removed (-1163 lines)
  
- [x] **Task 8.2.2**: Remove redundant streaming tests
  - Removed 28 redundant tests from `src/api/streaming.rs`
  - Kept 5 essential tests:
    1. `test_concurrent_stream_access` - Tests race condition fix in stream locking
    2. `test_sequential_reads_reuse_stream` - Tests stream reuse for sequential reads
    3. `test_edge_021_server_returns_200_instead_of_206` - Tests rqbit bug workaround
    4. `test_edge_023_stream_marked_invalid_after_error` - Tests invalid stream handling
    5. `test_edge_024_normal_server_response` - Tests normal operation path
  - Code reduction: 1984 lines → 821 lines (-1163 lines, -59%)
  - All 5 tests passing
  - Zero clippy warnings

### 8.3 Review Other Test Files
- [x] **Task 8.3.1**: Review `tests/performance_tests.rs`
  - **Completed**: Removed entire file
  - File contained only one test (`test_read_operation_timeout`) that tested tokio::time::timeout, not our code
  - No value for CI - was testing external crate behavior
  
- [x] **Task 8.3.2**: Review `benches/performance.rs`**
  - **Completed**: Removed entire file
  - Benchmarks referenced `rqbit_fuse::cache::Cache` which was removed in Task 7.2.2
  - File would not compile after cache module removal
  - Code reduction: -367 lines

---

## Phase 9: Documentation Trimming

Remove verbose documentation that duplicates README.

### 9.1 Trim Module Documentation
- [ ] **Task 9.1.1**: Simplify `src/lib.rs` documentation
  - Remove ASCII art architecture diagram
  - Keep one-paragraph module description
  - Remove detailed usage examples (keep in README)
  - Remove troubleshooting section (keep in README)
  - Remove performance tips section (keep in README)
  - Remove security considerations section (keep in README)
  
- [ ] **Task 9.1.2**: Simplify `src/config/mod.rs` documentation
  - Remove 174 lines of doc comments before Config struct
  - Keep one-sentence description per config section
  - Remove TOML example (keep in README)
  - Remove JSON example (keep in README)
  - Remove minimal configuration example
  - Remove environment variable examples
  
- [ ] **Task 9.1.3**: Review other module docs
  - Ensure all modules have <= 5 lines of documentation
  - Remove verbose examples from inline docs
  - Keep API documentation minimal

---

## Phase 10: Final Cleanup

Remove dead code and consolidate.

### 10.1 Remove Dead Code
- [ ] **Task 10.1.1**: Run cargo dead code detection
  - Use `cargo +nightly rustc -- -Zlints` or similar
  - Identify unused functions, structs, enums
  - Remove all dead code
  
- [ ] **Task 10.1.2**: Remove unused imports
  - Run `cargo clippy` and fix warnings
  - Remove all unused imports across codebase
  
- [ ] **Task 10.1.3**: Remove unused dependencies
  - Check `Cargo.toml` for unused crates
  - Remove dependencies that are no longer needed

### 10.2 Consolidate Remaining Code
- [ ] **Task 10.2.1**: Review module structure
  - Consider merging small modules
  - Ensure logical organization
  
- [ ] **Task 10.2.2**: Final review
  - Read through entire codebase
  - Identify any remaining complexity that could be removed
  - Ensure consistency in style and approach

### 10.3 Update Documentation
- [ ] **Task 10.3.1**: Update README.md
  - Document simplified configuration options
  - Remove references to removed features
  - Update examples to use minimal config
  
- [ ] **Task 10.3.2**: Update CHANGELOG.md
  - Document all breaking changes
  - List removed features
  - Explain migration path

---

## Research Tasks Summary

Research tasks should be completed before their dependent tasks:

1. **Task 1.1.1**: Status monitoring analysis → research/status-monitoring-analysis.md
2. **Task 2.1.1**: Config validation analysis → research/config-validation-analysis.md
3. **Task 2.2.1**: Config fields usage → research/config-fields-usage.md
4. **Task 3.1.1**: Error usage analysis → research/error-usage-analysis.md
5. **Task 4.1.1**: Metrics usage analysis → research/metrics-usage-analysis.md
6. **Task 5.1.1**: Prefetching analysis → research/prefetching-analysis.md
7. **Task 6.1.1**: FUSE macro usage → research/fuse-macro-usage.md
8. **Task 7.1.1**: Bitfield cache analysis → research/bitfield-cache-analysis.md
9. **Task 7.2.1**: Cache wrapper analysis → research/cache-wrapper-analysis.md

---

## Definition of Done

Each task is complete when:
- [ ] Code changes are implemented
- [ ] Tests pass: `nix-shell --run 'cargo test'`
- [ ] No warnings: `nix-shell --run 'cargo clippy'`
- [ ] Formatted: `nix-shell --run 'cargo fmt'`
- [ ] Changes are documented in commit message
- [ ] Checkbox is checked off in this TODO.md

---

## Estimated Timeline

- **Phase 1** (High Priority Removals): 1-2 days
- **Phase 2** (Configuration): 2-3 days
- **Phase 3** (Error System): 1-2 days
- **Phase 4** (Metrics): 1-2 days
- **Phase 5** (File Handles): 1-2 days
- **Phase 6** (FUSE Logging): 1 day
- **Phase 7** (Cache): 1-2 days
- **Phase 8** (Tests): 1-2 days
- **Phase 9** (Documentation): 1 day
- **Phase 10** (Cleanup): 1-2 days

**Total: 12-19 days** (depending on complexity discovered)

---

## Success Metrics

- [ ] Codebase reduced from ~5400 lines to ~1600 lines (70% reduction)
- [ ] Binary size reduced by ~30%
- [ ] Compilation time reduced by ~40%
- [ ] Test execution time reduced by ~50%
- [ ] All existing functionality preserved
- [ ] No regression in read performance
- [ ] Simpler configuration (6 fields vs 27)
- [ ] Fewer background tasks (0 vs 3)

---

*Created: 2026-02-23*
*Based on: review-r4.md*
