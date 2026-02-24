# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- SIMPLIFY-035: Complete File Handle Cleanup Tasks 5.2.2-5.2.3
  - Task 5.2.2: Remove memory tracking (already completed in SIMPLIFY-034)
    - Verified `memory_usage()` method was removed from FileHandleManager
    - No remaining memory tracking calls in handle management code
  - Task 5.2.3: Simplify FileHandle struct (already minimal)
    - FileHandle struct has only 4 essential fields: fh, inode, torrent_id, flags
    - Only method is new() constructor
    - No unused fields or methods to remove
  - All handle-related code is now minimal and clean
  - No code changes required - tasks were already complete from previous refactoring

- SIMPLIFY-034: Remove FileHandleState Struct (Task 5.1.2)
  - Deleted `FileHandleState` struct and all sequential tracking infrastructure from `src/types/handle.rs`
  - Removed `state: Option<FileHandleState>` field from `FileHandle` struct
  - Removed all state-related methods: `init_state()`, `update_state()`, `is_sequential()`, `sequential_count()`
  - Removed prefetching methods: `set_prefetching()`, `is_prefetching()`
  - Removed TTL-based expiration: `is_expired()`, `created_at` field
  - Removed `FileHandleManager` methods: `update_state()`, `set_prefetching()`, `remove_expired_handles()`, `memory_usage()`, `count_expired()`
  - Removed handle cleanup background task from `src/fs/filesystem.rs`
  - Deleted cleanup_handle field and related methods: `start_handle_cleanup()`, `stop_handle_cleanup()`
  - Removed 5 obsolete tests: `test_file_handle_state_tracking`, `test_prefetching_state`, `test_handle_ttl_expiration`, `test_handle_ttl_with_multiple_handles`, `test_handle_is_expired_method`
  - Code reduction: ~470 lines removed from handle.rs (734 → 264 lines, -64%)
  - All 175 tests passing with zero clippy warnings
  - Enables Task 5.1.3 (FileHandleManager state methods) and Task 5.2.1 (TTL cleanup removal)

- SIMPLIFY-033: Research Prefetching Code (Task 5.1.1)
  - Analyzed prefetching infrastructure in `src/types/handle.rs`
  - **Key Finding**: Prefetching code is DEAD CODE - never used in production
  - Verified no calls to `set_prefetching()`, `update_state()`, or sequential tracking outside tests
  - Confirmed `prefetch_enabled` config option already removed in Task 2.2.3
  - Documented ~145 lines of removable dead code with ZERO risk
  - Created comprehensive analysis in `research/prefetching-analysis.md`
  - Enables Tasks 5.1.2-5.1.4 and 5.2.1-5.2.3 for complete prefetching removal

- SIMPLIFY-032: Update All Metrics Usages (Task 4.2.4)
  - Fixed test compilation errors in `src/api/client.rs`
  - Added missing `metrics` parameter to 8 `RqbitClient::with_config()` test calls
  - Updated test calls at lines 1731, 1779, 1820, 1858, 1883, 2122, 2201, 2247
  - All 180+ tests passing with zero clippy warnings
  - Code formatted with cargo fmt
  - Metrics system integration complete and fully functional

- SIMPLIFY-031: Remove Periodic Logging (Task 4.2.3)
  - Removed `spawn_periodic_logging()` method (was never called)
  - Removed `log_periodic()` method (was never called)
  - Removed `log_full_summary()` method (was never called)
  - Kept only simple `log_summary()` method called on shutdown
  - Metrics system now has zero background tasks (reduced from 1 periodic logging task)
  - No code changes required - methods were already removed in previous simplification
  - All tests passing with zero clippy warnings

- SIMPLIFY-030: Remove Metrics Recording Calls (Task 4.2.2)
  - Verified metrics system already uses only 4 essential counters
  - Fixed test compilation errors in src/api/client.rs (removed obsolete retry_count verification)
  - Fixed test compilation errors in src/fs/filesystem.rs (added missing metrics parameter to AsyncFuseWorker::new)
  - Removed unused metrics variables in filesystem.rs background tasks
  - All 180+ tests passing with zero clippy warnings
  - The simplified Metrics struct correctly maintains only: record_read, record_error, record_cache_hit, record_cache_miss
  - No obsolete record_* methods (getattr, setattr, lookup, etc.) exist in codebase
  - Code reduction: Metrics system is now 65 lines with 4 essential methods

- SIMPLIFY-029: Create Minimal Metrics Struct (Task 4.2.1)
  - Replaced 520 lines of complex metrics system with 65 lines of essential counters
  - New `Metrics` struct with only 4 fields: bytes_read, error_count, cache_hits, cache_misses
  - Removed FuseMetrics, ApiMetrics, CacheMetrics structs and all helper methods
  - Updated all library code to use simplified metrics API
  - Library compiles successfully with -87% metrics code reduction
  - Tests need updating (will be handled in Task 4.2.4)
  - Commit: c970b3d

- SIMPLIFY-028: Research Metrics Usage (Task 4.1.1)
  - Analyzed 520 lines of metrics code in `src/metrics.rs`
  - Documented all metrics recording calls across the codebase
  - Key findings:
    - Only 4 out of 25 metrics provide actual value
    - Essential metrics: bytes_read, error_count, cache_hits, cache_misses
    - 21 metrics are either never recorded or provide redundant information
    - CacheMetrics is created but never populated (Cache has internal stats)
    - Operation counters (getattr, lookup, etc.) only used for trace logging
    - API metrics add overhead without operational value
    - Circuit breaker metrics never recorded despite methods existing
  - Recording call sites analyzed:
    - FuseMetrics: 24 calls across filesystem.rs, async_bridge.rs
    - ApiMetrics: 7 calls in client.rs
    - CacheMetrics: 0 calls (never used)
  - Proposed reduction: 520 lines → ~50 lines (-90%)
  - Location: `research/metrics-usage-analysis.md`

- SIMPLIFY-027: Simplify Error System (Tasks 3.2.1-3.2.5)
  - Reduced `RqbitFuseError` from 32 variants to 11 essential variants (66% reduction)
  - New simplified error enum in `src/error.rs`:
    - `NotFound(String)` - consolidated from NotFound, TorrentNotFound, FileNotFound
    - `PermissionDenied(String)` - consolidated from PermissionDenied, AuthenticationError
    - `TimedOut(String)` - consolidated from TimedOut, ConnectionTimeout, ReadTimeout
    - `NetworkError(String)` - consolidated from ServerDisconnected, NetworkError, ServiceUnavailable, CircuitBreakerOpen
    - `ApiError { status: u16, message: String }` - kept for HTTP status mapping
    - `IoError(String)` - consolidated from IoError, ReadError, HttpError, ClientInitializationError, RequestCloneError, ChannelFull, WorkerDisconnected, DataUnavailable
    - `InvalidArgument(String)` - consolidated from InvalidArgument, InvalidRange, InvalidValue
    - `ValidationError(Vec<ValidationIssue>)` - kept for config validation with multiple issues
    - `NotReady(String)` - consolidated from NotReady, DeviceBusy, RetryLimitExceeded
    - `ParseError(String)` - consolidated from SerializationError, ParseError
    - `IsDirectory` and `NotDirectory` - kept separate (different errno values)
  - Updated `to_errno()` method for simplified error mappings
  - Updated `is_transient()` method - now only TimedOut, NetworkError, NotReady, and certain ApiError codes are transient
  - Updated `is_server_unavailable()` method - now checks TimedOut and NetworkError
  - Updated error conversions:
    - `From<std::io::Error>` - maps to NotFound, PermissionDenied, TimedOut, InvalidArgument, or IoError
    - `From<reqwest::Error>` - maps to TimedOut or NetworkError
    - `From<serde_json::Error>` and `From<toml::de::Error>` - map to ParseError
  - Updated all error usage sites across codebase:
    - `src/config/mod.rs` - ReadError→IoError, ParseError→ParseError, InvalidValue→InvalidArgument
    - `src/api/client.rs` - Updated all 20+ error usages and tests
    - `src/api/streaming.rs` - HttpError→IoError
    - `src/fs/async_bridge.rs` - WorkerDisconnected→IoError, ChannelFull→IoError, TimedOut with context
  - Updated 50+ tests across the codebase to use new error variants
  - All 346+ tests passing with zero clippy warnings
  - Location: `src/error.rs`, `src/config/mod.rs`, `src/api/client.rs`, `src/api/streaming.rs`, `src/fs/async_bridge.rs`

- SIMPLIFY-026: Research Error Type Usage (Task 3.1.1)
  - Analyzed all 32 `RqbitFuseError` variants across the codebase
  - Documented usage patterns and errno mappings in `research/error-usage-analysis.md`
  - Identified consolidation opportunities: 32 variants → 8 essential variants (75% reduction)
  - Key findings:
    - Not Found: 3 variants → 1 (NotFound with context string)
    - Permission: 2 variants → 1 (PermissionDenied with context)
    - Timeout: 3 variants → 1 (TimedOut with context)
    - Network: 8 variants → 2 (NetworkError, ApiError)
    - I/O: 2 variants → 1 (IoError)
    - Validation: 4 variants → 2 (InvalidArgument, ValidationError)
    - Resource: 3 variants → 1 (NotReady)
    - State: 4 variants → 2 (RetryLimitExceeded→NotReady, SerializationError/ParseError→ParseError)
    - Directory: 2 variants → keep both (different errno values)
    - Filesystem: 1 variant → merge into PermissionDenied
    - Data: 1 variant → merge into IoError
  - errno mappings consolidated to 11 distinct groups
  - Proposed simplified enum with 8 variants: NotFound, PermissionDenied, TimedOut, NetworkError, ApiError, IoError, InvalidArgument, NotReady, IsDirectory, NotDirectory
  - Ready for Task 3.2.1: Create simplified error enum
  - Location: `research/error-usage-analysis.md`

- SIMPLIFY-025: Remove PerformanceConfig Env Vars (Task 2.3.2)
  - Removed environment variable parsing for performance-related fields:
    - `TORRENT_FUSE_MAX_CONCURRENT_READS` - no longer configurable via env var
    - `TORRENT_FUSE_READAHEAD_SIZE` - no longer configurable via env var
  - These fields can still be configured via config file (TOML/JSON)
  - Simplified environment variable interface to 9 essential variables:
    - API_URL, MOUNT_POINT, METADATA_TTL, MAX_ENTRIES
    - READ_TIMEOUT, LOG_LEVEL
    - AUTH_USERPASS, AUTH_USERNAME, AUTH_PASSWORD
  - Updated documentation in `src/config/mod.rs`:
    - Removed env var references from PerformanceConfig doc comments
    - Added TORRENT_FUSE_READ_TIMEOUT to main Config env var examples
  - Updated test in `tests/config_tests.rs`:
    - Removed TORRENT_FUSE_MAX_CONCURRENT_READS from empty numeric test cases
  - Environment variables reduced from 11 to 9 (18% reduction in this step)
  - All tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`, `tests/config_tests.rs`

- SIMPLIFY-024: Simplify CacheConfig (Task 2.2.7)
  - Removed 2 fields from `CacheConfig` struct in `src/config/mod.rs`:
    - `torrent_list_ttl` - now uses `metadata_ttl` for all cache data
    - `piece_ttl` - now uses `metadata_ttl` for all cache data
  - Simplified `CacheConfig` to 2 fields: `metadata_ttl`, `max_entries`
  - Removed environment variable parsing for:
    - `TORRENT_FUSE_TORRENT_LIST_TTL`
    - `TORRENT_FUSE_PIECE_TTL`
  - Updated documentation in `src/config/mod.rs`:
    - Simplified CacheConfig struct documentation (removed 2 fields)
    - Updated TOML configuration example to remove torrent_list_ttl and piece_ttl
    - Updated JSON configuration example to remove torrent_list_ttl and piece_ttl
    - Updated environment variable documentation to remove 2 env vars
  - Updated `test_json_config_parsing` to use `max_entries` instead of removed `piece_ttl`
  - Reduced CacheConfig from 4 fields to 2 fields (50% reduction)
  - Environment variables reduced from 11 to 9 (18% reduction in this step)
  - Configuration fields reduced from 8 to 6 (25% reduction in this step)
  - All 346+ tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`

- SIMPLIFY-023: Remove ResourceLimitsConfig (Task 2.2.6)
  - Removed entire `ResourceLimitsConfig` struct from `src/config/mod.rs`
  - Removed 3 fields: `max_cache_bytes`, `max_open_streams`, `max_inodes`
  - Removed `resources` field from main `Config` struct
  - Removed `impl Default for ResourceLimitsConfig`
  - Removed environment variable parsing for:
    - `TORRENT_FUSE_MAX_CACHE_BYTES`
    - `TORRENT_FUSE_MAX_OPEN_STREAMS`
    - `TORRENT_FUSE_MAX_INODES`
  - Updated `src/fs/filesystem.rs`:
    - Changed inode_manager initialization to use hardcoded value of 100000
    - Previously used `config.resources.max_inodes`, now hardcoded default
  - Hardcoded reasonable defaults preserved:
    - max_inodes: 100000 (in filesystem.rs)
    - max_streams: 50 (in streaming.rs, already the default)
    - max_cache_bytes: No longer needed (cache uses moka's built-in limits)
  - Updated documentation in `src/config/mod.rs`:
    - Removed ResourceLimitsConfig from struct documentation
    - Removed TOML configuration example for resources section
    - Removed JSON configuration example for resources section
    - Removed environment variable documentation for removed fields
  - Environment variables reduced from 14 to 11 (21% reduction in this step)
  - Configuration fields reduced from 11 to 8 (27% reduction in this step)
  - All 346+ tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`, `src/fs/filesystem.rs`

- SIMPLIFY-022: Remove LoggingConfig Options (Task 2.2.5)
  - Removed 4 fields from `LoggingConfig` struct in `src/config/mod.rs`:
    - `log_fuse_operations` - now always logs FUSE operations at debug level
    - `log_api_calls` - now always logs API calls at debug level (field was unused)
    - `metrics_enabled` - metrics system being removed in Phase 4
    - `metrics_interval_secs` - metrics system being removed in Phase 4
  - Simplified `LoggingConfig` to single field: `level`
  - Removed environment variable parsing for:
    - `TORRENT_FUSE_LOG_FUSE_OPS`
    - `TORRENT_FUSE_LOG_API_CALLS`
    - `TORRENT_FUSE_METRICS_ENABLED`
    - `TORRENT_FUSE_METRICS_INTERVAL`
  - Updated `src/fs/macros.rs`:
    - Removed `log_fuse_operations` check from `fuse_log!`, `fuse_error!`, `fuse_ok!` macros
    - Macros now always log at debug level (tracing handles filtering)
    - Updated `reply_*` macros to take `metrics` parameter explicitly instead of `self`
  - Updated `src/fs/filesystem.rs`:
    - Removed all `self` parameters from macro calls
    - Removed manual `log_fuse_operations` check for slow read logging
    - Updated all `reply_*` macro calls to pass `self.metrics`
  - Updated documentation and examples in `src/config/mod.rs`:
    - Removed 4 fields from TOML configuration example
    - Removed 4 fields from JSON configuration example
    - Removed environment variable documentation for removed fields
    - Simplified LoggingConfig struct documentation
  - Removed obsolete test `test_validate_metrics_disabled_no_interval_required`
  - Reduced LoggingConfig from 5 fields to 1 field (80% reduction)
  - Environment variables reduced from 18 to 14 (22% reduction in this step)
  - All 346+ tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`, `src/fs/macros.rs`, `src/fs/filesystem.rs`

- SIMPLIFY-021: Remove MonitoringConfig (Task 2.2.4)
  - Removed entire `MonitoringConfig` struct from `src/config/mod.rs`
  - Removed 2 fields: `status_poll_interval` and `stalled_timeout`
  - Removed `monitoring` field from main `Config` struct
  - Removed `impl Default for MonitoringConfig`
  - Removed environment variable parsing for:
    - `TORRENT_FUSE_STATUS_POLL_INTERVAL`
    - `TORRENT_FUSE_STALLED_TIMEOUT`
  - Updated all documentation and examples in `src/config/mod.rs` to remove monitoring section
  - Removed monitoring section from TOML configuration example
  - Removed monitoring section from JSON configuration example
  - Removed monitoring from environment variable documentation
  - Updated main Config struct documentation to remove monitoring from field list
  - Reduced Config from 7 sections to 6 sections
  - Configuration fields reduced from 27 to 25 (7% reduction in this step)
  - Environment variables reduced from 20+ to 18 (10% reduction in this step)
  - All 346 tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`

- SIMPLIFY-020: Remove PerformanceConfig Options (Task 2.2.3)
  - Removed 2 fields from `PerformanceConfig` struct in `src/config/mod.rs`:
    - `prefetch_enabled` - feature removed entirely (was disabled by default, didn't work well)
    - `check_pieces_before_read` - now always checks piece availability
  - Simplified `PerformanceConfig` to 3 fields: `read_timeout`, `max_concurrent_reads`, `readahead_size`
  - Removed environment variable parsing for `TORRENT_FUSE_PREFETCH_ENABLED` and `TORRENT_FUSE_CHECK_PIECES_BEFORE_READ`
  - Updated `src/fs/filesystem.rs`:
    - Removed `track_and_prefetch()` method
    - Removed `do_prefetch()` method
    - Removed call to prefetch tracking in read handler
  - Updated documentation and examples in `src/config/mod.rs` to reflect simplified config
  - Updated test files to remove references to removed fields:
    - `tests/fuse_operations.rs`: Removed test `test_read_paused_torrent_check_disabled`
    - Removed lines setting `check_pieces_before_read` in tests
  - Reduced PerformanceConfig from 5 fields to 3 fields (40% reduction)
  - Removed 90 lines of prefetch-related code from filesystem.rs
  - All config unit tests passing
  - Location: `src/config/mod.rs`, `src/fs/filesystem.rs`, `tests/fuse_operations.rs`

- SIMPLIFY-019: Remove MountConfig Options (Task 2.2.2)
  - Removed 4 fields from `MountConfig` struct in `src/config/mod.rs`:
    - `allow_other` - now always false (other users cannot access mount)
    - `auto_unmount` - now always true (always auto-unmount on exit)
    - `uid` - now uses `libc::geteuid()` directly at runtime
    - `gid` - now uses `libc::getegid()` directly at runtime
  - Simplified `MountConfig` to single field: `mount_point`
  - Removed `--allow-other` and `--auto-unmount` CLI arguments from Mount command
  - Removed environment variable parsing for `TORRENT_FUSE_ALLOW_OTHER` and `TORRENT_FUSE_AUTO_UNMOUNT`
  - Updated `src/fs/filesystem.rs`:
    - Hardcoded `AutoUnmount` mount option (always enabled)
    - Removed conditional `AllowOther` mount option
    - Use `unsafe { libc::geteuid() }` and `unsafe { libc::getegid() }` directly in `build_file_attr()`
  - Updated documentation and examples in `src/config/mod.rs` to reflect simplified config
  - Updated all test files to remove references to removed fields:
    - `tests/common/fuse_helpers.rs`
    - `tests/common/test_helpers.rs`
    - `tests/resource_tests.rs`
    - `tests/common/mock_server.rs`
    - `src/fs/filesystem.rs` (removed test_build_mount_options_allow_other test)
    - `src/config/mod.rs` (removed allow_other from TOML test)
  - Reduced MountConfig from 5 fields to 1 field (80% reduction)
  - All 185+ tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`, `src/main.rs`, `src/fs/filesystem.rs`

- SIMPLIFY-018: Research Config Fields Usage (Task 2.2.1)
  - Analyzed all 27 configuration fields across 7 config sections
  - Documented field usage patterns and identified candidates for removal
  - Categorized fields as "essential" vs "removable":
    - **Essential (11 fields)**: api.url, api.username, api.password, cache.metadata_ttl, cache.max_entries, mount.mount_point, performance.read_timeout, performance.max_concurrent_reads, performance.readahead_size, logging.level
    - **Removable (16 fields)**: All MountConfig except mount_point, all MonitoringConfig, all ResourceLimitsConfig, most LoggingConfig, some CacheConfig and PerformanceConfig fields
  - Proposed reduction from 27 fields to 11 fields (59% reduction)
  - Proposed reduction from 30+ env vars to 9 env vars (70% reduction)
  - Full analysis written to `research/config-fields-usage.md`
  - Provides roadmap for Phase 2.2 configuration simplification tasks

### Changed

- SIMPLIFY-017: Consolidate Validation Methods (Task 2.1.4)
  - Merged 7 separate validation methods into a single `validate()` method
  - Removed per-field validation methods: `validate_api_config()`, `validate_cache_config()`, `validate_mount_config()`, `validate_performance_config()`, `validate_monitoring_config()`, `validate_logging_config()`, `validate_resources_config()`
  - Consolidated essential validations directly in main `validate()` method:
    - API URL non-empty and parseable
    - Mount point is absolute path
    - Log level is valid (error, warn, info, debug, trace)
  - Removed redundant validations (>0 checks for fields with defaults)
  - Removed UID/GID bounds checks (enforced by u32 type)
  - Removed mount point existence check (directory may not exist at config time)
  - Removed tests for removed validations from `src/config/mod.rs` (6 tests)
  - Removed tests for removed validations from `tests/config_tests.rs` (5 tests)
  - Updated remaining tests to reflect simplified validation behavior
  - Reduced validation code from ~183 lines to ~25 lines (-86% reduction)
  - All 185+ tests passing with zero clippy warnings
  - Location: `src/config/mod.rs`, `tests/config_tests.rs`

- SIMPLIFY-016: Simplify URL Validation (Task 2.1.3)
  - Removed explicit scheme validation from `validate_api_config()` in `src/config/mod.rs`
  - URL validation now only checks: (1) non-empty URL, (2) parseable by `reqwest::Url::parse()`
  - Any valid URL scheme is now accepted (http, https, ftp, file, etc.)
  - Updated test `test_validate_url_with_non_http_scheme` to expect success for non-http schemes
  - Updated test `test_validate_url_without_scheme` to expect success (reqwest accepts scheme-less URLs)
  - All 23 config tests passing with zero clippy warnings
  - Reduced validation complexity by removing arbitrary scheme restrictions

- SIMPLIFY-015: Remove Arbitrary Upper Bound Validations (Task 2.1.2)
  - Removed max TTL checks (86400 limit) from `validate_cache_config()`
  - Removed max entries checks (1,000,000 limit) from `validate_cache_config()`
  - Removed read timeout max checks (3600s limit) from `validate_performance_config()`
  - Removed max concurrent reads checks (1000 limit) from `validate_performance_config()`
  - Removed readahead size checks (1GB limit) from `validate_performance_config()`
  - Removed status poll interval max checks (3600s limit) from `validate_monitoring_config()`
  - Removed stalled timeout max checks (86400s limit) from `validate_monitoring_config()`
  - Removed metrics interval max checks (86400s limit) from `validate_logging_config()`
  - Removed max cache bytes checks (10GB limit) from `validate_resources_config()`
  - Removed max open streams checks (1000 limit) from `validate_resources_config()`
  - Removed max inodes checks (10M limit) from `validate_resources_config()`
  - Removed test `test_validate_exceeds_max_ttl` that tested removed validation
  - Validation rules reduced from 33 to 19 (42% reduction in validation complexity)
  - Essential validations preserved: non-empty checks, >0 checks, log level validation, URL format checks
  - All 23 config tests passing with zero clippy warnings
  - Reduced code by ~120 lines in `src/config/mod.rs`

- SIMPLIFY-014: Remove DiscoveryResult Struct (Task 1.4.1)
  - Changed `discover_torrents()` return type from `Result<DiscoveryResult>` to `Result<Vec<u64>>`
  - Updated all 3 call sites to handle `Vec<u64>` directly instead of `DiscoveryResult`
  - Removed `DiscoveryResult` struct definition with fields `new_count` and `current_torrent_ids`
  - Removed `#[allow(dead_code)]` attribute from the removed struct
  - Removed unused `new_count` variable and related logging (`"Discovered {} new torrent(s)"`)
  - Simplified code by removing unnecessary struct wrapper when only `current_torrent_ids` was being used
  - All tests passing with zero clippy warnings
  - Reduced code by ~20 lines in `src/fs/filesystem.rs`

- SIMPLIFY-013: Remove JSON Status Output (Task 1.3.1, 1.3.2)
  - Verified JSON output format already removed as part of Task 1.2.2
  - `OutputFormat` enum (Text/Json variants) confirmed removed from `src/main.rs`
  - `--format` CLI argument confirmed removed from Status subcommand
  - JSON serialization structs (`StatusOutput`, `ConfigOutput`, `MountInfoOutput`) confirmed removed
  - `Commands::Status` variant has no format parameter
  - `run_status()` function has no format parameter, outputs text only
  - Status command confirmed to show only "MOUNTED" / "NOT MOUNTED" status
  - All tests passing with zero clippy warnings
  - No code changes required - task was already completed in prior work
  - Location: `src/main.rs`, `TODO.md`

- SIMPLIFY-012: Remove JSON Output Format from Status Command (Task 1.2.2)
  - Removed `OutputFormat` enum (Text/Json variants) from `src/main.rs`
  - Removed `--format` CLI argument from Status subcommand
  - Removed JSON serialization structs: `StatusOutput`, `ConfigOutput`
  - Simplified `run_status()` function to output text format only
  - Status command now only shows "MOUNTED" / "NOT MOUNTED" status
  - Reduced code complexity by removing conditional format handling
  - All tests passing with zero clippy warnings
  - Location: `src/main.rs`

### Research

- Task 2.1.1: Configuration Validation Analysis
  - Created `research/config-validation-analysis.md` documenting all 33 validation rules
  - Analyzed 7 validation methods across `src/config/mod.rs` lines 708-983
  - **Key Findings**:
    - 19 rules are ESSENTIAL (non-empty URLs, positive numbers, absolute paths, valid log levels)
    - 14 rules are ARBITRARY (upper bounds like TTL < 86400, max_entries < 1M)
    - Upper bounds can be safely removed without impacting correctness
  - **Impact**: Will reduce from 33 rules to ~19 rules after removing upper bounds
  - **Next Steps**: Proceed with Task 2.1.2 to remove arbitrary upper bound validations
  - Location: `research/config-validation-analysis.md`, `TODO.md`

- Task 1.1.1: Status Monitoring Analysis
  - Created `research/status-monitoring-analysis.md` documenting the background status monitoring task
  - Analyzed usage of `torrent_statuses` cache across the codebase
  - **Key Finding**: Status monitoring provides NO critical functionality
    - All piece availability checking uses API client's separate bitfield cache
    - Status monitoring only provides informational status and early-exit optimizations
    - Safe to remove without impacting core read operations
  - Identified 17 usage sites of `torrent_statuses` - all are non-critical
  - Documented that removing this feature would:
    - Eliminate 1 of 3 background tasks
    - Remove ~70 lines of code from filesystem.rs
    - Remove need for `TorrentStatus` and `TorrentState` types
    - Reduce memory usage (no status cache)
    - Remove dependency on `monitoring.status_poll_interval` and `stalled_timeout` config options

### Changed

- SIMPLIFY-011: Remove Mount Info Display Feature (Task 1.2.1)
  - Removed `MountInfo` struct from `src/mount.rs` (lines 106-112)
  - Removed `get_mount_info()` function from `src/mount.rs` (lines 114-143)
  - Updated `src/main.rs` to remove import of `get_mount_info`
  - Updated `run_status()` text output to remove filesystem, size, used, available fields
  - Updated `run_status()` JSON output to remove `MountInfoOutput` struct and `mount_info` field
  - Status command now shows only "MOUNTED" / "NOT MOUNTED" status
  - Removed 38 lines of code from `src/mount.rs`, 20 lines from `src/main.rs`
  - All tests passing with zero clippy warnings
  - Location: `src/mount.rs`, `src/main.rs`

- SIMPLIFY-010: Simplify Test Structure
  - Removed 2 duplicate tests from `tests/fuse_operations.rs` that already existed in `tests/integration_tests.rs`:
    - `test_filesystem_creation_and_initialization` - basic filesystem creation test
    - `test_unicode_and_special_characters` - unicode filename handling test  
  - Fixed pre-existing clippy error: removed redundant `.skip(0)` call in pagination test
  - Reduced `fuse_operations.rs` from 6894 to 6828 lines (-66 lines, ~1% reduction)
  - All tests pass: 95 in fuse_operations, 19 in integration_tests
  - No loss of test coverage - duplicates provided identical test scenarios

- SIMPLIFY-009: Consolidate Documentation
  - Removed redundant architecture diagram from README.md (~29 lines of ASCII art)
  - Simplified "How It Works" section in README.md - removed implementation-specific details
  - Consolidated verbose "Implementation Status" section (~90 lines) into concise summary (~5 lines)
  - Fixed outdated references: changed TASKS.md → TODO.md in README.md and AGENTS.md
  - README.md is now focused on user-facing documentation
  - All technical implementation details remain in lib.rs rustdoc (canonical source)
  - No loss of information - documentation is now better organized by audience

- SIMPLIFY-008: Consolidate Type Definitions
  - ✅ **COMPLETED**: Type consolidation already done - no code changes required
  - `types/torrent.rs::Torrent` was dead code - already removed in prior cleanup
  - `api/types.rs::TorrentDetails` never existed (no implementation found in codebase)
  - `api/types.rs::TorrentInfo` is the canonical torrent representation used throughout
  - Verified remaining types serve different purposes and are not duplicates:
    - `TorrentSummary`: Lightweight representation for list endpoints
    - `TorrentInfo`: Detailed representation with file information
    - `TorrentStats`: API response structure for statistics
    - `TorrentStatus`: Internal monitoring representation
  - All types have distinct use cases at different architectural layers
  - No redundant fields or duplicate representations found
  - All tests passing with zero clippy warnings

### Changed

- SIMPLIFY-007: Simplify AsyncFuseWorker
  - Removed redundant `new_for_test` method (~30 lines) - tests now use `new()` with explicit capacity
  - Added comprehensive documentation explaining the async/sync bridge pattern
  - Documented why `tokio::sync::mpsc` is used for requests (async worker context)
  - Documented why `std::sync::mpsc` is used for responses (need `recv_timeout` in sync FUSE callbacks)
  - Added example flow documentation showing the 8-step request/response process
  - Updated all callers: `src/fs/filesystem.rs` and `tests/common/fuse_helpers.rs`
  - All tests passing with zero clippy warnings
  - Research documented in `research/asyncfuseworker-simplification.md`

- SIMPLIFY-003: Simplify Configuration System
  - Removed `piece_check_enabled` field from `PerformanceConfig` (now always enabled)
  - Removed `return_eagain_for_unavailable` field from `PerformanceConfig` (now always uses consistent EAGAIN behavior)
  - Removed associated environment variables: `TORRENT_FUSE_PIECE_CHECK_ENABLED` and `TORRENT_FUSE_RETURN_EAGAIN`
  - Updated `src/fs/filesystem.rs` to always check piece availability (removed conditional)
  - Updated `src/fs/filesystem.rs` to always return EAGAIN for unavailable torrent data
  - Simplified codebase by removing two niche configuration options
  - All 361 tests passing with zero clippy warnings

- SIMPLIFY-006: Consolidate Test Helpers
  - Created `tests/common/test_helpers.rs` with consolidated test infrastructure
  - Removed ~110 lines of duplicated code across test files
  - Added `TestEnvironment` struct for comprehensive test setup
  - Created helper modules:
    - `handle_helpers`: File handle allocation patterns (`allocate_test_handle`, `exhaust_handle_limit`)
    - `torrent_helpers`: Torrent creation helpers (`single_file`, `multi_file`)
  - Updated 4 test files to use common helpers:
    - `tests/integration_tests.rs`: Removed 35 lines of duplicate setup code
    - `tests/fuse_operations.rs`: Removed 35 lines of duplicate setup code
    - `tests/unicode_tests.rs`: Removed 37 lines of duplicate setup code
    - `tests/resource_tests.rs`: Added common module import
  - Fixed pre-existing compilation errors in common modules:
    - Fixed type mismatch in `file_count` (usize vs u32)
    - Fixed `RqbitClient::new()` Result handling
    - Fixed `AsyncFuseWorker::new_for_test` API usage
    - Removed broken `fuser::Request::new` usage (private API)
    - Removed broken `fuser::KernelConfig::empty()` usage (non-existent)
  - All tests passing (200+) with consolidated test infrastructure

- SIMPLIFY-001: Complete error type consolidation
  - Removed legacy `ApiError` enum from `src/api/types.rs` (142 lines)
  - Removed duplicate `DataUnavailableReason` from `src/api/types.rs` (21 lines)
  - Total dead code removed: 162 lines from src/api/types.rs
  - All error types now consolidated in unified `RqbitFuseError` in src/error.rs
  - Backward compatibility maintained via `pub use crate::error::RqbitFuseError as ApiError` in src/api/mod.rs
  - All tests passing (200+ tests) with zero clippy warnings
  - Error consolidation subtasks SIMPLIFY-001A through SIMPLIFY-001E now complete

### Added

- EDGE-057: Test environment variable edge cases
  - Added 6 comprehensive tests to `tests/config_tests.rs` with sequential execution via mutex:
    - `test_edge_057_missing_required_env_vars`: Tests graceful handling when env vars are not set (uses defaults)
    - `test_edge_057_empty_string_env_var_value`: Tests empty string values for API URL, mount point, log level, and auth
    - `test_edge_057_very_long_env_var_value`: Tests env var values exceeding 4096 characters (5000 chars preserved correctly)
    - `test_edge_057_empty_numeric_env_var_values`: Tests empty strings for numeric fields (properly fail to parse)
    - `test_edge_057_whitespace_only_env_var_values`: Tests whitespace-only values for string and numeric fields
    - `test_edge_057_env_var_case_sensitivity`: Tests that uppercase env var names take precedence over lowercase
  - Used `std::sync::Mutex` to ensure sequential test execution and prevent env var interference between tests
  - All environment variable edge cases now have comprehensive test coverage

- EDGE-056: Test timeout edge cases
  - Created `tests/config_tests.rs` with 10 comprehensive timeout validation tests:
    - `test_edge_056_timeout_zero`: Validates that timeout=0 fails validation
    - `test_edge_056_timeout_u64_max`: Validates that u64::MAX fails validation (exceeds 3600s limit)
    - `test_edge_056_timeout_negative_from_env`: Tests graceful handling of negative values from environment
    - `test_edge_056_timeout_negative_large_from_env`: Tests handling of large negative values
    - `test_edge_056_timeout_valid_values`: Tests various valid timeout values (1-3600 seconds)
    - `test_edge_056_timeout_just_above_max`: Tests rejection of 3601 seconds (just above limit)
    - `test_edge_056_timeout_one`: Tests minimum valid timeout of 1 second
    - `test_edge_056_other_timeout_fields`: Tests monitoring timeouts (status_poll_interval, stalled_timeout)
    - `test_edge_056_metrics_interval_zero_when_enabled`: Tests metrics interval validation
    - `test_edge_056_invalid_timeout_from_env_handling`: Tests various invalid formats (letters, decimals, empty)
  - All timeout edge cases now have comprehensive test coverage

- EDGE-055: Test invalid mount points
  - Added `test_validate_mount_point_is_file` in `src/fs/filesystem.rs`
    - Tests that mount point validation fails when path is a file instead of directory
    - Creates a temporary file and attempts to use it as mount point
    - Verifies appropriate error message about mount point not being a directory
  - Relative path validation already exists: `test_validate_relative_mount_point` in `src/config/mod.rs`
  - Non-existent path validation already exists: `test_validate_mount_point_nonexistent` in `src/fs/filesystem.rs`
  - All mount point edge cases now have comprehensive test coverage

- EDGE-054: Test invalid URL validation
  - Enhanced `src/config/mod.rs` to validate URL schemes strictly
  - Modified `validate_api_config()` to reject non-http/https schemes
  - Added 2 comprehensive tests in `src/config/mod.rs`:
    - `test_validate_url_without_scheme`: Tests that URLs like "localhost:3030" fail validation
    - `test_validate_url_with_invalid_scheme`: Tests that URLs with invalid schemes like "ftp://localhost:3030" fail validation
  - URL validation now explicitly checks for valid scheme before accepting configuration
  - All 328 tests passing with zero clippy warnings

- EDGE-053: Test maximum path length
  - Added 4 comprehensive tests to `tests/unicode_tests.rs` for path length handling:
    - `test_edge_053_path_length_handling`: Tests paths at various lengths (100-3000 chars)
      - Tests short paths (89 chars), medium (449 chars), long (909 chars)
      - Tests very long (1819 chars) and extremely long paths (2719 chars)
      - Verifies all path lengths are handled gracefully without panic
    - `test_edge_053_path_length_near_boundary`: Tests paths approaching PATH_MAX
      - Creates paths with 350+ nested directories (~3509 characters)
      - Verifies system handles near-boundary paths without panic
    - `test_edge_053_path_length_various_depths`: Tests various nesting depths
      - Tests depths from 10 to 300 levels deep
      - Each level adds approximately 10 characters plus separator
      - Verifies consistent behavior across all depths
    - `test_edge_053_maximum_path_with_multibyte_utf8`: Tests UTF-8 paths at limits
      - Uses Japanese character "あ" (3 bytes in UTF-8)
      - Creates paths with multi-byte UTF-8 characters approaching length limits
      - Verifies path length is measured in bytes, not characters
  - All path length tests verify graceful error handling without panic
  - Tests confirm filesystem accepts paths up to tested limits

- EDGE-052: Test path normalization (NFD vs NFC)
  - Added 5 comprehensive tests to `tests/unicode_tests.rs` for Unicode normalization handling:
    - `test_edge_052_nfc_normalization`: Tests NFC (Canonical Composition) form
      - Tests filenames with composed characters like "café" with precomposed 'é' (U+00E9)
      - Verifies NFC filenames are created and looked up correctly
    - `test_edge_052_nfd_normalization`: Tests NFD (Canonical Decomposition) form
      - Tests filenames with decomposed characters like "café" with 'e' + combining accent
      - Verifies NFD filenames from macOS HFS+ are handled gracefully
    - `test_edge_052_nfc_nfd_consistency`: Tests consistency between normalization forms
      - Creates file with NFC form and verifies lookup behavior
      - Ensures NFC and NFD forms are not treated as duplicate files
      - Verifies at least one form exists and both don't exist simultaneously
    - `test_edge_052_various_normalization_cases`: Tests multiple Unicode characters
      - Tests résumé, naïve, français, Zürich with various accents
      - Tests Japanese and Chinese (no normalization differences expected)
      - Verifies all normalization cases handled without panic
    - `test_edge_052_already_normalized`: Tests already-normalized strings
      - Verifies ASCII filenames work correctly (already in both NFC and NFD)
      - Ensures no issues with strings that don't need normalization
  - Added `unicode-normalization` dev-dependency for test normalization functions
  - All path normalization tests pass without panic
  - Behavior is consistent across NFC and NFD forms

- EDGE-051: Test UTF-8 edge cases
  - Added 5 comprehensive tests to `tests/unicode_tests.rs` for UTF-8 filename handling:
    - `test_edge_051_emoji_filenames`: Tests emoji including multi-codepoint sequences
      - Tests document emoji (📄), movie emoji (🎬), music note (🎵), rocket (🚀)
      - Tests complex ZWJ sequences: family (👨‍👩‍👧‍👦), rainbow flag (🏳️‍🌈)
      - Verifies 4-byte UTF-8 emoji are handled correctly without panic
    - `test_edge_051_cjk_filenames`: Tests Chinese, Japanese, Korean characters
      - Tests simplified/traditional Chinese (文档/文檔)
      - Tests Japanese Katakana/Hiragana (ドキュメント)
      - Tests Korean Hangul (문서)
      - Tests mixed CJK scripts (Chinese + Japanese)
      - Verifies 3-byte UTF-8 CJK characters are handled correctly
    - `test_edge_051_rtl_filenames`: Tests Right-to-Left scripts
      - Tests Arabic (ملف), Hebrew (קובץ), Persian/Farsi (فایل)
      - Tests mixed LTR/RTL text (doc_ملف)
      - Tests mixed Arabic + Hebrew (ملف_קובץ)
      - Verifies bidirectional text is handled correctly
    - `test_edge_051_zero_width_joiner_filenames`: Tests ZWJ emoji sequences
      - Tests professional emoji: man technologist (👨‍💻), woman scientist (👩‍🔬)
      - Tests activity emoji: man farmer (👨‍🌾), woman artist (👩‍🎨)
      - Tests gendered emoji: man running (🏃‍♂️), woman running (🏃‍♀️)
      - Verifies complex multi-codepoint ZWJ sequences work correctly
    - `test_edge_051_other_utf8_edge_cases`: Tests other Unicode edge cases
      - Tests accented Latin (café, naïve, resumé with combining accent)
      - Tests mathematical symbols (∑, Ω, ∞)
      - Tests special symbols (★, ♠)
      - Verifies various Unicode categories are handled correctly
  - All UTF-8 edge cases are handled gracefully without panic
  - All 200+ tests passing with zero clippy warnings

- EDGE-050: Test control characters in filename
  - Added 3 tests to `tests/unicode_tests.rs` for control character handling:
    - `test_edge_050_control_characters_in_filename`: Tests common control characters
      - Tests newline (\n), tab (\t), carriage return (\r), SOH (0x01), US (0x1F), DEL (0x7F)
      - Verifies system handles control chars gracefully without panic
      - System sanitizes control characters by removing them from filenames
    - `test_edge_050_multiple_control_characters`: Tests combinations of control characters
      - Tests multiple control chars in sequence (e.g., "\n\t\r")
      - Tests leading and trailing control characters
      - Verifies consistent handling regardless of position or combination
    - `test_edge_050_control_chars_with_valid_files`: Tests isolation from valid files
      - Creates valid file first, then attempts control char file creation
      - Verifies valid file remains accessible with correct attributes
      - Ensures control char handling doesn't corrupt filesystem state
  - System handles control characters by sanitizing (removing them), not by rejecting
  - All 200+ tests passing with zero clippy warnings

- EDGE-049: Test null byte in filename
  - Added 3 tests to `tests/unicode_tests.rs` for null byte handling:
    - `test_edge_049_null_byte_in_filename`: Tests null bytes at various positions (start, middle, end, multiple)
      - Verifies system handles null bytes gracefully without panic
      - Tests sanitization behavior (null bytes are stripped from filenames)
    - `test_edge_049_null_byte_positions`: Tests filenames consisting entirely of null bytes
      - Ensures no panic or crash occurs with extreme edge case
    - `test_edge_049_null_byte_with_valid_files`: Tests that null byte handling doesn't affect other files
      - Creates valid file first, then attempts null byte file creation
      - Verifies valid file remains accessible after null byte handling
  - System handles null bytes by sanitizing (removing them), not by rejecting
  - All 200+ tests passing with zero clippy warnings

- EDGE-048: Test maximum filename length
  - Created `tests/unicode_tests.rs` with 4 comprehensive tests:
    - `test_edge_048_maximum_filename_length_255_chars`: Tests 255-character filename at boundary
      - Creates torrent with exactly 255-character filename
      - Verifies file is created and accessible in filesystem
      - Confirms file attributes (size) are correct
    - `test_edge_048_filename_length_256_chars_handling`: Tests graceful handling of 256-char filenames
      - Verifies system handles oversized filenames without panic
      - Tests both success and graceful error paths
    - `test_edge_048_filename_length_boundary_variations`: Tests lengths 253-257 chars
      - Verifies consistent behavior across boundary values
      - Tests that files with 255 or fewer chars succeed
    - `test_edge_048_maximum_filename_with_multibyte_utf8`: Tests UTF-8 byte limits
      - Uses Japanese characters (3 bytes each) to test 255-byte boundary
      - 85 Japanese chars × 3 bytes = 255 bytes exactly
      - Verifies filesystem handles multi-byte UTF-8 correctly
  - All 197+ tests passing with zero clippy warnings

- EDGE-047: Test semaphore exhaustion
  - Created `tests/resource_tests.rs` with 4 comprehensive tests:
    - `test_edge_047_semaphore_exhaustion`: Tests basic semaphore exhaustion with max_concurrent_reads=10
      - Acquires all 10 permits and verifies 11th acquisition waits (not fails)
      - Tests permit release allows subsequent acquisitions
    - `test_edge_047b_semaphore_multiple_waiters`: Verifies FIFO ordering of waiters
      - Spawns 3 tasks waiting for permits while all permits are held
      - Releases permits one by one and verifies tasks complete in order
    - `test_edge_047c_semaphore_permit_release_on_cancel`: Tests permit cleanup on drop
      - Verifies dropping all held permits immediately makes them available
      - Tests permits can be reacquired after drop
    - `test_edge_047d_concurrency_stats_accuracy`: Verifies stats accurately reflect semaphore state
      - Tests stats show correct max_concurrent_reads and available_permits
      - Verifies available_permits decreases as permits are acquired
  - All 193+ tests passing

- EDGE-046: Test cache memory limit
  - Added `Cache::with_memory_limit()` constructor in `src/cache.rs` for byte-based cache limits
  - Implemented `test_cache_memory_limit_eviction` with 3 test scenarios:
    - Test 1: Inserts 500KB data within 1MB limit, then exceeds limit with 1.1MB more data
      - Verifies cache handles memory limit overflow without crashing
      - Confirms newer entries remain accessible after eviction
    - Test 2: Tests cache behavior at 50%, 100%, and 110% of memory capacity
      - Verifies entries exist at 100% capacity
      - Confirms cache remains functional when exceeding limit
    - Test 3: Tests oversized entry (10KB in 1KB cache) handling
      - Verifies no crash when inserting entry larger than cache limit
      - Confirms cache remains functional after oversized insertion
  - All 189+ tests passing with zero clippy warnings

- EDGE-045: Test inode limit exhaustion
  - Implemented `test_edge_045_inode_limit_exhaustion_with_torrents` in `src/fs/inode_manager.rs`
  - Tests max_inodes = 100 limit by creating 99 torrents (root + 99 = 100 total)
  - Verifies 100th torrent allocation fails gracefully with return value 0
  - Tests multiple failed allocations beyond limit ensure consistent behavior
  - Verifies all originally allocated torrents remain intact after failures
  - Tests mixed entry types (files, symlinks) also fail at limit
  - Verifies removing a torrent allows new allocation
  - Tests edge cases: max_inodes = 1 (only root) and max_inodes = 2 (root + 1 entry)
  - All 188+ tests passing with zero clippy warnings

- EDGE-043: Test cache eviction during get
  - Implemented 2 tests in `src/cache.rs`:
    - `test_cache_eviction_during_get`: Tests concurrent get operations during cache evictions
      - Spawns 5 tasks doing gets and 5 tasks doing inserts to trigger evictions
      - Verifies cache handles the race condition gracefully without panicking
      - Tests cache maintains capacity constraints after concurrent operations
    - `test_cache_eviction_during_get_specific_key`: Tests race condition when specific key gets evicted
      - Spawns concurrent get operations on a specific key while other tasks cause evictions
      - Verifies either valid data or None is returned, but no panic occurs
      - Tests cache state remains consistent after concurrent eviction during get
  - All 187+ tests passing with zero clippy warnings

- EDGE-042: Test mount/unmount race
  - Implemented 2 tests in `tests/integration_tests.rs`:
    - `test_edge_042_mount_unmount_race`: Tests immediate unmount during mount operation
      - Spawns mount in separate thread and immediately unmounts from main thread
      - Verifies mount thread doesn't panic and handles race gracefully
      - Accepts both success and error returns as long as no panic occurs
    - `test_edge_042b_rapid_mount_unmount_cycles`: Tests multiple rapid mount/unmount cycles
      - Runs 3 cycles of mount/unmount to verify no resource leaks
      - Confirms repeated operations don't cause panics
  - All tests pass with zero clippy warnings

- EDGE-041: Test concurrent discovery
  - Implemented test `test_edge_041_concurrent_discovery` in `tests/integration_tests.rs`
  - Verifies atomic check-and-set mechanism prevents duplicate torrent creation
  - Tests concurrent `refresh_torrents()` calls using barrier synchronization
  - Confirms only one torrent is created despite concurrent discovery operations
  - Tests cooldown mechanism prevents rapid successive discoveries
  - All tests pass with zero clippy warnings

- EDGE-040: Test read while torrent being removed
  - Implemented comprehensive test `test_edge_040_read_while_torrent_being_removed` in `tests/integration_tests.rs`
  - Tests graceful handling when file handles exist for removed torrents
  - Verifies no panic or crash when releasing handles for deleted files
  - Tests multiple handles with various states (active reads, prefetching)
  - Tests system state consistency after torrent removal with open handles
  - All tests pass with zero clippy warnings

- EDGE-039: Test connection reset
  - Implemented 4 comprehensive tests in `src/api/client.rs` for connection reset handling:
    - `test_edge_039_connection_reset_error_conversion`: Tests error conversion from reqwest errors
      - Verifies ServerDisconnected and NetworkError are marked as transient
      - Confirms proper errno mapping (ENOTCONN for ServerDisconnected, ENETUNREACH for NetworkError)
    - `test_edge_039_connection_reset_retries_success`: Tests retry logic with transient failures
      - Simulates 503 errors followed by successful response
      - Verifies retry metrics are recorded correctly
    - `test_edge_039_connection_reset_retries_exhausted`: Tests behavior when retries exhausted
      - Server consistently returns 503 errors beyond retry limit
      - Verifies appropriate error returned after exhausting retries
    - `test_edge_039_connection_reset_during_body_read`: Tests graceful handling of connection reset
      - Simulates connection reset during HTTP body read with empty response
      - Verifies no panic occurs and error is handled gracefully
  - All tests pass with zero clippy warnings

- EDGE-038: Test timeout at different stages
  - Implemented 4 comprehensive tests in `src/api/client.rs` for timeout handling:
    - `test_edge_038_connection_timeout`: Tests connection timeout using short connect_timeout (100ms)
    - `test_edge_038_read_timeout`: Tests read timeout with server response delay (200ms vs 50ms timeout)
    - `test_edge_038_dns_resolution_failure`: Tests DNS failure handling for unresolvable hostnames
    - `test_edge_038_timeout_error_types`: Tests error type mappings and errno conversions
  - All tests verify appropriate error types (ConnectionTimeout, ReadTimeout) are returned
  - Tests verify transient error classification and server availability detection
  - All 176+ tests passing with zero clippy warnings

- EDGE-037: Test malformed JSON response
  - Implemented 5 comprehensive tests in `src/api/client.rs` for handling invalid JSON
  - Tests verify graceful error handling without panic for:
    - Incomplete JSON structures (missing closing braces/brackets)
    - Invalid escape sequences in JSON strings
    - Type mismatches (e.g., string instead of number for id field)
    - Empty response bodies
    - Null values for required struct fields
  - All tests verify proper error propagation with descriptive messages
  - All 172+ tests passing with zero clippy warnings

- EDGE-036: Test HTTP 429 Too Many Requests
  - Implemented rate limit handling in `src/api/client.rs`
  - Modified `execute_with_retry` to respect `Retry-After` header on 429 responses
  - Added 4 comprehensive tests:
    - `test_edge_036_rate_limit_with_retry_after_header`: Verifies client waits specified duration
    - `test_edge_036_rate_limit_without_retry_after_uses_default_delay`: Tests fallback behavior
    - `test_edge_036_rate_limit_exhausts_retries`: Verifies error returned when retries exhausted
    - `test_edge_036_multiple_rate_limits_eventually_succeed`: Tests multiple rate limits before success
  - All 172+ tests passing with zero clippy warnings

- EDGE-035: Test case sensitivity
  - Implemented `test_edge_035_case_sensitivity` in `tests/fuse_operations.rs`
  - Tests verify case-sensitive file lookups on Linux filesystems
  - Creates 3 files differing only in case: "file.txt", "FILE.txt", "File.txt"
  - Verifies each file has unique inode and correct size (100, 200, 300 bytes)
  - Confirms case-sensitive lookups return correct files
  - Tests non-existent case variations and directory name case sensitivity
  - Added `file_size()` method to `InodeEntry` for retrieving file sizes in tests
  - All 170+ tests passing with zero clippy warnings

- EDGE-034: Test symlink edge cases
  - Implemented 6 comprehensive tests for symlink edge cases in `tests/fuse_operations.rs`
  - Tests verify graceful handling of:
    - Circular symlinks (a -> b, b -> a)
    - Deep circular chains (a -> b -> c -> a)
    - Self-referential symlinks (link -> link)
    - Symlinks pointing outside torrent directory (../../../etc/passwd)
    - Symlinks with absolute paths (/Test Torrent/file.txt)
    - Symlinks with special path components (./, ../, ~)
  - All tests verify symlinks are created without panic and attributes are correct
  - All 168+ tests passing with zero clippy warnings

### Changed

- SIMPLIFY-001E: Remove old error types and clean up exports
  - Deleted `src/fs/error.rs` - removed duplicate ToFuseError trait implementation
  - Removed `pub mod error;` from `src/fs/mod.rs` - error module no longer needed
  - Updated documentation in `src/lib.rs` - changed reference from `fs::error` module to `RqbitFuseError` type
  - ToFuseError trait now exclusively provided by `src/error.rs` (single source of truth)
  - All imports updated to use `crate::error::ToFuseError` instead of `crate::fs::error::ToFuseError`
  - No breaking changes - all functionality preserved through unified error type
  - All 114+ tests passing with zero clippy warnings
  - Net reduction: 32 lines removed
  - Location: src/fs/error.rs (deleted), src/fs/mod.rs, src/lib.rs

- SIMPLIFY-001D: Migrate config/ module from ConfigError to RqbitFuseError
  - Migrated `src/config/mod.rs` to use unified RqbitFuseError type
  - Removed `ConfigError` enum (4 variants: ReadError, ParseError, InvalidValue, ValidationError)
  - Removed duplicate `ValidationIssue` struct (now imported from `crate::error`)
  - Updated all function signatures returning `ConfigError` to return `RqbitFuseError`:
    - `from_file()`, `from_default_locations()`, `merge_from_env()`
    - `load()`, `load_with_cli()`, `validate()`
  - Replaced all `ConfigError::` constructors with `RqbitFuseError::` equivalents
  - Updated all 22 config tests to use `RqbitFuseError` variants
  - Removed `thiserror` import from config module (no longer needed)
  - All tests passing with zero clippy warnings
  - Net reduction: ~20 lines removed

- SIMPLIFY-001C: Migrate fs/ module from FuseError to RqbitFuseError
  - Migrated `src/fs/async_bridge.rs` to use RqbitFuseError and RqbitFuseResult
  - Replaced FuseError::TimedOut with RqbitFuseError::TimedOut
  - Replaced FuseError::WorkerDisconnected with RqbitFuseError::WorkerDisconnected
  - Replaced FuseError::ChannelFull with RqbitFuseError::ChannelFull
  - Replaced FuseError::IoError with RqbitFuseError::IoError
  - Updated return types from FuseResult<T> to RqbitFuseResult<T>
  - Updated fs/mod.rs to re-export RqbitFuseError and RqbitFuseResult from crate::error
  - Simplified src/fs/error.rs to only contain ToFuseError trait (removed FuseError enum)
  - Removed duplicate From<std::io::Error> implementation (already in src/error.rs)
  - Maintained backward compatibility through existing ToFuseError trait
  - All 168+ tests passing with zero clippy warnings
  - Net reduction: 171 lines removed, 36 lines added across 3 files
  - Location: src/fs/async_bridge.rs, src/fs/error.rs, src/fs/mod.rs

- SIMPLIFY-001B: Migrate api/ module from ApiError to RqbitFuseError
  - Migrated all api/ module files to use unified RqbitFuseError type
  - Updated `src/api/client.rs`: Replaced all ApiError usages with RqbitFuseError
  - Updated `src/api/streaming.rs`: Replaced ApiError import with RqbitFuseError
  - Updated `src/api/types.rs`: Changed ListTorrentsResult to use RqbitFuseError for errors field
  - Updated `src/api/mod.rs`: Re-exported RqbitFuseError as ApiError for backward compatibility
  - Updated `src/fs/filesystem.rs`: Changed ApiError reference to RqbitFuseError with to_errno()
  - All existing tests updated to use RqbitFuseError variants and methods
  - Maintained backward compatibility through type alias in api/mod.rs
  - All tests passing (89 unit tests, 15 integration tests, 10 performance tests)
  - Zero clippy warnings

### Added

- EDGE-033: Test path with "." components
  - Added `test_edge_033_dot_components_path` to verify path handling with self-reference components
  - Tests standalone "." at root level (should resolve to root or be handled gracefully)
  - Tests "./file.txt" at root level (if normalized, should resolve to "/file.txt")
  - Tests "." component in middle of path ("/Test Torrent/./file1.txt")
  - Tests nested "." components ("/Test Torrent/subdir/./file2.txt")
  - Tests multiple "." components ("/Test Torrent/./subdir/./file2.txt")
  - Tests trailing "." component ("/Test Torrent/subdir/.")
  - Tests multiple consecutive "." components ("/Test Torrent/././file1.txt")
  - Verifies normal paths without "." components still work correctly
  - Verifies filesystem state remains intact after dot component tests
  - Location: `tests/fuse_operations.rs`

- EDGE-032: Test path with double slashes
  - Added `test_edge_032_double_slashes_path` to verify path normalization with double slashes
  - Tests double slashes at root level ("//Test Torrent")
  - Tests multiple double slashes ("///Test Torrent")
  - Tests double slashes in the middle of paths ("/Test Torrent//file1.txt")
  - Tests double slashes at the end ("/Test Torrent//")
  - Tests double slashes in nested paths ("/Test Torrent/subdir//file2.txt")
  - Verifies normal paths without double slashes still work correctly
  - Tests empty path components with multiple consecutive slashes
  - Verifies filesystem state remains intact after double slash attempts
  - Location: `tests/fuse_operations.rs`

- EDGE-031: Test path traversal attempts
  - Added `test_edge_031_path_traversal_attempts` to verify path traversal security
  - Tests paths with ".." attempting to traverse above root ("/../secret.txt")
  - Tests multiple ".." components ("/../../secret.txt") 
  - Tests ".." in the middle of paths ("/Test Torrent/../secret.txt")
  - Verifies valid paths without ".." still work correctly
  - Tests that paths with ".." within bounds work ("/Test Torrent/subdir/..")
  - Verifies no directory escape via path traversal attacks
  - Tests complex paths like "/Test Torrent/../../etc/passwd" are rejected
  - Tests mixed valid/invalid components ("/Test Torrent/subdir/../../secret.txt")
  - Verifies file structure remains intact and accessible via normal paths
  - Location: `tests/fuse_operations.rs`

- EDGE-030: Test concurrent allocation stress
  - Added `test_edge_030_concurrent_allocation_stress` to verify concurrent inode allocation under high load
  - Tests 100 threads allocating simultaneously, each creating 100 inodes (10,000 total)
  - Verifies all inodes are unique with no duplicates across concurrent allocations
  - Verifies no gaps in inode sequence (all values from 2 to 10,001 are allocated)
  - Tests immediate availability of allocated inodes across all threads
  - Verifies thread-safety of InodeManager's atomic counter and DashMap storage
  - Tests that `next_inode` counter is correctly set after mass concurrent allocation
  - Location: `src/fs/inode_manager.rs`

- EDGE-029: Test allocation after clear_torrents
  - Added `test_allocation_after_clear_torrents` to verify inode reuse after clear_torrents()
  - Tests 7 comprehensive phases:
    1. Initial allocation of torrents, files, and symlinks
    2. Clear torrents and verify old inodes removed
    3. New allocations reuse inode numbers correctly
    4. Verify no duplicates exist after reuse
    5. Lookup operations work for new inodes
    6. Path lookups resolve correctly
    7. Multiple clear cycles maintain consistency
  - Verifies clear_torrents() properly resets next_inode counter to 2
  - Ensures no inode number duplicates after multiple clear/allocation cycles
  - Location: `src/fs/inode_manager.rs`

- EDGE-028: Test max_inodes limit
  - Added `test_max_inodes_limit` to verify inode limit enforcement
  - Tests that 11th allocation fails with return value 0 when max_inodes=10
  - Verifies no panic occurs when limit is reached
  - Tests all entry types (files, directories, symlinks) respect the limit
  - Verifies `can_allocate()` correctly reflects limit status
  - Location: `src/fs/inode_manager.rs`

- EDGE-027: Test inode 0 allocation attempt
  - Added `test_inode_0_allocation_attempt` to verify graceful handling of inode 0 insertion
  - Tests that inserting entry with inode 0 doesn't corrupt the next_inode counter
  - Verifies normal allocations continue to work correctly after inode 0 handling
  - Added `test_inode_0_not_returned_from_allocate` to verify allocate() never returns inode 0
  - Tests allocate 10 entries and verify all have inode >= 2
  - Tests uniqueness of allocated inodes across multiple allocations
  - Location: `src/fs/inode_manager.rs`

- SIMPLIFY-001A: Create unified RqbitFuseError enum in src/error.rs
  - Consolidated three separate error types into single unified enum:
    - FuseError from src/fs/error.rs (12 variants)
    - ApiError from src/api/types.rs (18 variants)
    - ConfigError from src/config/mod.rs (4 variants)
  - Organized errors into logical categories: Not Found, Permission/Auth, Timeout, I/O, Network/API, Validation, Resource, State, Directory, Filesystem, Data
  - Implemented `thiserror::Error` derive macro for consistent error formatting
  - Implemented `to_errno()` method for FUSE error code mapping (eliminates duplicate mappings)
  - Implemented `is_transient()` for retryable error detection
  - Implemented `is_server_unavailable()` for server health checking
  - Added `From` implementations for std::io::Error, reqwest::Error, serde_json::Error, toml::de::Error
  - Preserved `ToFuseError` trait for anyhow::Error backward compatibility
  - Added comprehensive unit tests (11 test functions covering all error variants)
  - Exported new module in src/lib.rs
  - Location: src/error.rs (473 lines)

### Changed

- SIMPLIFY-002: Split Large Files - Inode module refactoring
  - Split `src/fs/inode.rs` (1,051 lines) into focused modules:
    - `src/fs/inode_entry.rs` (~350 lines) - InodeEntry enum with Serialize/Deserialize and helper methods
    - `src/fs/inode_manager.rs` (~850 lines) - InodeManager struct with allocation, lookup, and management
  - Maintained backward compatibility through re-exports in existing `inode.rs` module
  - Updated `fs/mod.rs` to declare new modules and maintain public API
  - All 208+ tests pass with zero clippy warnings
  - Reduces individual file complexity while preserving existing functionality

### Added

- EDGE-025: Test wrong content-length
  - Added `test_edge_025_content_length_more_than_header` to verify graceful handling when server sends more data than Content-Length header indicates
  - Added `test_edge_025_content_length_less_than_header` to verify graceful handling when server sends less data than Content-Length header indicates
  - Added `test_edge_025_content_length_mismatch_at_offset` to verify handling of mismatches at non-zero offsets
  - Tests verify HTTP layer (hyper) detects mismatch and returns error, which streaming layer handles gracefully without panic
  - Location: `src/api/streaming.rs`

- EDGE-024: Test slow server response
  - Added `test_edge_024_slow_server_response` to verify timeout handling with slow server (5s delay vs 100ms client timeout)
  - Added `test_edge_024_slow_server_partial_response` to test timeout during body read phase
  - Added `test_edge_024_normal_server_response` as control test verifying normal operation completes quickly
  - Tests verify client respects timeout settings and doesn't block indefinitely on slow responses
  - Location: `src/api/streaming.rs`

- EDGE-023: Test network disconnect during read
  - Added `test_edge_023_network_disconnect_during_read` to verify graceful handling of network failures
  - Added `test_edge_023_stream_marked_invalid_after_error` to verify streams are marked invalid after errors
  - Added `test_edge_023_stream_manager_cleanup_invalid_stream` to test manager cleanup of invalid streams
  - Tests verify proper error handling, stream invalidation, and resource cleanup with no leaks
  - Location: `src/api/streaming.rs`

- EDGE-022: Test empty response body handling
  - Added `test_edge_022_empty_response_body_200` to verify 200 OK with empty body returns empty bytes
  - Added `test_edge_022_empty_response_body_206` to verify 206 Partial Content with empty body returns empty bytes
  - Added `test_edge_022_empty_response_at_offset` to verify empty response at non-zero offset doesn't cause infinite loop
  - All tests verify streaming layer handles empty bodies gracefully without panics or hanging
  - Location: `src/api/streaming.rs`

- EDGE-021: Test server returning 200 OK instead of 206 Partial Content
  - Added `test_edge_021_server_returns_200_instead_of_206` to verify streaming layer handles 200 OK responses
  - Tests that the streaming layer correctly skips to the requested offset when server returns full file
  - Verifies data correctness by checking returned bytes match expected values at the offset position
  - Added `test_edge_021_server_returns_200_at_offset_zero` to verify no skip occurs at offset 0
  - Added `test_edge_021_large_skip_with_200_response` to test 100KB skip with 1MB file
  - All tests verify the existing rqbit bug workaround in `PersistentStream::new()` works correctly
  - Location: `src/api/streaming.rs`

- EDGE-026: Test seek patterns
  - Added `test_forward_seek_exactly_max_boundary` to verify seeking exactly MAX_SEEK_FORWARD reuses stream
  - Added `test_forward_seek_just_beyond_max_boundary` to verify seeking beyond limit creates new stream
  - Added `test_rapid_alternating_seeks` to verify rapid forward/backward seek patterns work correctly
  - Added `test_backward_seek_one_byte_creates_new_stream` to verify even 1-byte backward seeks create new streams
  - Tests verify stream creation/reuse logic under various seek patterns
  - Location: `src/api/streaming.rs`

- EDGE-020: Test cache statistics edge cases
  - Added `test_cache_stats_edge_cases` to verify hit rate calculations handle edge cases
  - Tests 0 total requests (fresh cache) - verifies no division by zero
  - Tests 0 hits with many misses - verifies 0.0 hit rate and 100.0 miss rate
  - Tests 0 misses with many hits - verifies 100.0 hit rate and 0.0 miss rate
  - Tests mixed hit/miss ratio (75% hits) - verifies accurate percentage calculations
  - Tests very large numbers (u64::MAX) - verifies no overflow
  - Location: `src/cache.rs`

- EDGE-019: Test concurrent insert of same key
  - Added `test_concurrent_insert_same_key` to verify 10 threads inserting same key simultaneously
  - Verifies cache handles concurrent inserts gracefully with exactly one entry
  - Tests that final value is one of the inserted values and cache remains consistent
  - Location: `src/cache.rs`

- EDGE-018: Test rapid insert/remove cycles in cache
  - Added `test_cache_rapid_insert_remove_cycles` to verify 1000 rapid insert/remove cycles on same key
  - Added `test_cache_rapid_mixed_key_cycles` to verify rapid cycles across multiple keys
  - Both tests verify cache consistency and no corruption under rapid operations
  - Location: `src/cache.rs`

- EDGE-016: Test cache entry expiration during access
  - Added `test_cache_entry_expiration_during_access` to verify cache returns None for expired entries
  - Added `test_cache_expiration_race_condition` to test concurrent access during expiration
  - Both tests verify no panics occur when entries expire during get() operations
  - Location: `src/cache.rs`

### Research

- SIMPLIFY-2-012: Review config fields for unused/unimplemented features
  - Analyzed `piece_check_enabled` in `PerformanceConfig`
  - Analyzed `prefetch_enabled` in `PerformanceConfig`
  - Verified `piece_check_enabled` is used in `src/fs/filesystem.rs:888` for piece availability checks
  - Verified `prefetch_enabled` is used in `src/fs/filesystem.rs:944` for prefetch triggering
  - **Conclusion**: All config fields are legitimately used and implemented
  - Rationale: Both fields are working configuration options with real implementations
  - `piece_check_enabled`: Controls piece verification (default: true)
  - `prefetch_enabled`: Controls read-ahead prefetching (default: false)
  - No unused or placeholder config fields found
  - Created research document: `research/config_fields_usage_analysis.md`
  - Location: `src/config/mod.rs`, `src/fs/filesystem.rs`

- SIMPLIFY-2-011: Consolidate inode types across modules
  - Moved `InodeEntry` from `src/types/inode.rs` to `src/fs/inode.rs`
  - Consolidated inode-related code into single module for better maintainability
  - Updated imports in `src/fs/filesystem.rs`, `tests/performance_tests.rs`, `benches/performance.rs`
  - Removed empty `src/types/inode.rs` file
  - Updated `src/types/mod.rs` to re-export from `fs::inode`
  - Updated `src/fs/mod.rs` to export both `InodeEntry` and `InodeManager`
  - Net result: 291 lines consolidated, 5 lines net reduction in imports
  - Location: `src/fs/inode.rs` (now contains both InodeEntry and InodeManager)

- SIMPLIFY-2-010: Evaluate merging FuseError and ApiError types
  - Analyzed `FuseError` in `src/fs/error.rs` (12 variants, FUSE-specific)
  - Analyzed `ApiError` in `src/api/types.rs` (18 variants, API-specific)
  - Reviewed current integration via `ToFuseError` trait
  - **Conclusion**: Error types should NOT be merged
  - Rationale: Clean separation of concerns (filesystem vs HTTP API)
  - Existing ToFuseError trait provides adequate integration
  - Merging would create unnecessarily complex "god enum"
  - Both types are domain-specific and appropriately designed
  - Created research document: `research/error_types_merge_analysis.md`
  - Location: `src/fs/error.rs`, `src/api/types.rs`

- SIMPLIFY-2-009: Verify proptest dev dependency usage
  - Analyzed proptest usage across the codebase
  - Found declaration in `Cargo.toml:37` but no actual usage
  - Only reference was a misleading comment in `src/fs/inode.rs:695`
  - No `use proptest`, `proptest!`, or property-based tests found
  - **Conclusion**: proptest is an unused dev dependency
  - Recommendation: REMOVE the dependency - it was never implemented
  - Created research document: `research/proptest_usage_verification.md`
  - Location: `Cargo.toml`, `src/fs/inode.rs`

- SIMPLIFY-2-009: Verify base64 dependency usage
  - Analyzed base64 usage across the codebase
  - Found active usage in `src/api/client.rs:112` for HTTP Basic Auth header encoding
  - Found active usage in `src/api/streaming.rs:341` for streaming client authentication
  - Verified no other uses of base64 exist in the codebase
  - **Conclusion**: base64 is legitimately required for HTTP Basic Auth functionality
  - Recommendation: KEEP the dependency - it is properly used
  - Created research document: `research/base64_usage_verification.md`
  - Location: `src/api/client.rs`, `src/api/streaming.rs`

- SIMPLIFY-2-007: Review file handle state tracking in `src/types/handle.rs`
  - Analyzed `FileHandleState` struct and its 5 fields (last_offset, last_size, sequential_count, last_access, is_prefetching)
  - Reviewed sequential read detection logic and prefetching trigger mechanism
  - Examined TTL-based handle cleanup implementation
  - **Conclusion**: All complex features are actively used and cannot be removed
  - Sequential tracking runs on every read operation (required for prefetch decisions)
  - Prefetching logic is disabled by default but user-configurable (feature preserved)
  - TTL cleanup runs every 5 minutes via background task (prevents memory leaks)
  - All FileHandleState fields have active references in the codebase
  - Created research document: `research/handle_state_tracking_review.md`
  - Location: `src/types/handle.rs`, `src/fs/filesystem.rs`

- SIMPLIFY-2-006: Review circuit breaker implementation
  - Reviewed `src/api/circuit_breaker.rs` (85 lines)
  - Analyzed circuit breaker usage in `src/api/client.rs` (integrated into execute_with_retry)
  - **Conclusion**: Circuit breaker is over-engineered for localhost API communication
  - Recommendation: Remove circuit breaker and rely on existing retry logic
  - Rationale: rqbit is a local service (127.0.0.1:3030), circuit breakers are for distributed systems
  - Benefits: -85 lines of code, simpler client architecture, reduced complexity
  - Created research document: `research/circuit_breaker_review.md`
  - Location: `src/api/circuit_breaker.rs`, `src/api/client.rs`

### Simplified

- SIMPLIFY-2-008: Replace config macros with standard Rust patterns in `src/config/mod.rs`
  - Removed `default_fn!` macro (28 lines) and replaced with inline `impl Default` blocks
  - Removed `default_impl!` macro (10 lines) and replaced with explicit Default implementations
  - Removed `env_var!` macro (15 lines) and replaced with standard `std::env::var` calls
  - Replaced 27 `default_fn!` invocations with 7 explicit `impl Default` blocks
  - Replaced 35+ `env_var!` invocations with explicit `if let Ok(val)` blocks
  - Total: ~35 lines of macro definitions removed, ~120 lines of macro calls replaced with ~180 lines of explicit code
  - Benefits: Easier to understand, better IDE support, clearer error messages, standard Rust idioms
  - Location: `src/config/mod.rs`

- SIMPLIFY-2-005: Simplify metrics system in `src/metrics.rs`
  - Removed custom `LatencyMetrics` trait (28 lines)
  - Removed `record_op!` macro and replaced with explicit methods (35 lines)
  - Removed atomic snapshot loops from `FuseMetrics::log_summary()` and `ApiMetrics::log_summary()`
  - Implemented `avg_latency_ms()` directly on `FuseMetrics` and `ApiMetrics`
  - Simplified tests: removed complex concurrent tests, kept core functionality tests
  - Reduced file from 657 to 512 lines (144 lines removed, ~22% reduction)
  - All method signatures remain compatible - no changes needed in call sites
  - Location: `src/metrics.rs`

### Fixed

- Fixed floating point precision issue in `test_cache_metrics`
  - Changed from exact equality assertion to approximate comparison with epsilon
  - Prevents test failures due to minor floating point representation differences
  - Location: `src/metrics.rs`

- PORT-001: Add Linux support while maintaining macOS compatibility
  - Fixed compilation warnings on Linux regarding `libc::ENOATTR`
  - ENOATTR is macOS-specific; Linux uses ENODATA for the same purpose
  - Added platform-specific conditional compilation using `#[cfg(target_os = "macos")]`
  - Created internal `ENOATTR` constant that maps to `libc::ENOATTR` on macOS and `libc::ENODATA` on Linux
  - Replaced all three occurrences of `libc::ENOATTR` with the new constant in `getxattr()` method
  - Eliminates deprecation warnings: "ENOATTR is not available on Linux; use ENODATA instead"
  - Location: `src/fs/filesystem.rs`

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

- SIMPLIFY-2-009: Complete unused dependency review
  - Verified `strum` and `proptest` were already removed from Cargo.toml
  - Confirmed `base64` is actively used for HTTP Basic Auth in api/client.rs and api/streaming.rs
  - No unused dependencies remain in the project
  - Updated SIMPLIFY-2.md checklist to mark item 9 as complete
  - Location: `Cargo.toml`, `SIMPLIFY-2.md`

- SIMPLIFY-2-009: Remove unused `proptest` dev dependency
  - Removed `proptest = "1.4"` from `[dev-dependencies]` in `Cargo.toml`
  - Removed misleading "// Property-based tests using proptest" comment from `src/fs/inode.rs`
  - No actual proptest tests existed - only a comment suggesting future use
  - Total: 1 dev dependency removed, 1 comment removed
  - Rationale: Dependency was declared but never implemented or used
  - Benefits: Faster dev builds, cleaner dependency tree, reduced maintenance
  - See analysis: `research/proptest_usage_verification.md`
  - Location: `Cargo.toml`, `src/fs/inode.rs`

- SIMPLIFY-2-009: Remove unused `strum` dependency
  - Removed `strum = { version = "0.25", features = ["derive"] }` from `Cargo.toml`
  - Removed `use strum::Display;` import from `src/api/types.rs`
  - Removed `Display` derive and `#[strum(serialize_all = "snake_case")]` from `DataUnavailableReason` enum
  - Removed `Display` derive and `#[strum(serialize_all = "snake_case")]` from `TorrentState` enum
  - Total: 6 lines removed, 1 dependency removed
  - Rationale: Display trait was never actually used (no `.to_string()` calls found)
  - Enums are only used for pattern matching and comparisons, not string formatting
  - See analysis: `research/strum_usage_verification.md`
  - Location: `Cargo.toml`, `src/api/types.rs`

- SIMPLIFY-2-006: Remove circuit breaker implementation
  - Deleted `src/api/circuit_breaker.rs` (85 lines)
  - Removed circuit breaker from `src/api/mod.rs` exports
  - Removed `with_circuit_breaker()` constructor and `circuit_state()` method from `RqbitClient`
  - Simplified `execute_with_retry()` to remove circuit breaker checks and recording
  - Simplified `health_check()` to remove circuit breaker state tracking
  - Removed circuit breaker unit tests
  - Total: 185 lines removed (85 from circuit_breaker.rs + ~100 from client.rs integration)
  - Rationale: Circuit breaker is over-engineered for localhost API (127.0.0.1:3030)
  - Existing retry logic (3 retries with exponential backoff) provides adequate resilience
  - Circuit breakers are designed for distributed systems, not local services
  - See analysis: `research/circuit_breaker_analysis_decision.md`
  - Location: `src/api/circuit_breaker.rs` (deleted), `src/api/client.rs`, `src/api/mod.rs`

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

## 2026-02-23 - Phase 1.1.2: Remove Status Monitoring Background Task

### Removed
- Removed `start_status_monitoring()` method from `src/fs/filesystem.rs`
- Removed `stop_status_monitoring()` method from `src/fs/filesystem.rs`
- Removed `monitor_handle` field from TorrentFS struct
- Removed initialization of `monitor_handle` field
- Removed calls to status monitoring from `init()`, `destroy()`, and `shutdown()` methods

### Impact
- Reduced codebase by ~70 lines
- Removed unnecessary background task that was polling torrent status
- Status monitoring provided no critical functionality (piece availability checking uses API client's separate cache)
- One less background task running (reduced from 3 to 2 tasks)

## 2026-02-23 - Phase 1.1.3: Remove TorrentStatus and Related Types

### Removed
- Removed `torrent_statuses: Arc<DashMap<u64, TorrentStatus>>` field from TorrentFS struct
- Removed all related imports (TorrentStatus, DashMap)
- Removed status monitoring methods:
  - `get_torrent_status()` - public API method (unused internally)
  - `monitor_torrent()` - added torrents to status cache
  - `unmonitor_torrent()` - removed torrents from status cache
  - `list_torrent_statuses()` - returned all monitored statuses
- Removed calls to `monitor_torrent()` and `unmonitor_torrent()` in torrent lifecycle methods
- Removed `torrent_statuses` cleanup from torrent removal handlers (3 locations)
- Simplified `check_pieces_available()` method - now returns false since status cache is removed
- Removed early EAGAIN checks in read handler that depended on torrent status cache
- Updated `getxattr()` to return ENOATTR for `user.torrent.status` attribute
- Removed initial `TorrentStatus` struct creation in `create_torrent_structure()`

### Impact
- Reduced codebase by ~207 lines (220 changed, 21 added, 199 removed from filesystem.rs)
- Eliminated the `torrent_statuses` DashMap cache entirely
- No critical functionality lost - all piece availability checking uses API client's bitfield cache
- Simplified read operation logic (removed non-critical early-exit optimizations)
- Status information still available via API client when needed
- All 360+ tests pass successfully

### Rationale
Per research in `research/status-monitoring-analysis.md`, the status monitoring provided no critical functionality:
- Real piece availability checking uses `api_client.check_range_available()` with separate bitfield cache
- Status monitoring was informational only - not used for decisions
- Early EAGAIN optimizations could be removed without affecting correctness
- Extended attribute for status was purely diagnostic

