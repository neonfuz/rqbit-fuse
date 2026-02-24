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
  
- [ ] **Task 2.2.6**: Remove ResourceLimitsConfig
  - Remove entire `ResourceLimitsConfig` struct
  - Remove all resource limit fields
  - Remove related env var parsing
  - Use hardcoded reasonable defaults instead
  
- [ ] **Task 2.2.7**: Simplify CacheConfig
  - Keep only: `metadata_ttl`, `max_entries`
  - Remove: `torrent_list_ttl`, `piece_ttl` (use metadata_ttl for all)
  - Update all usages to use simplified config

### 2.3 Update CLI Arguments
- [ ] **Task 2.3.1**: Remove CLI args for removed config options
  - Remove `--allow-other` from Mount command
  - Remove `--auto-unmount` from Mount command
  - Remove any other args corresponding to removed config fields
  
- [ ] **Task 2.3.2**: Update env var parsing
  - Remove env var parsing for all removed config fields
  - Keep only: API_URL, MOUNT_POINT, METADATA_TTL, MAX_ENTRIES, READ_TIMEOUT, LOG_LEVEL, AUTH credentials

---

## Phase 3: Error System Simplification

Reduce 28 error types to 8 essential types.

### 3.1 Research Error Usage Patterns
- [ ] **Task 3.1.1**: Research - Document error type usage
  - Search for all usages of each RqbitFuseError variant
  - Group variants by how they're handled (mapped to errno)
  - Write findings to `research/error-usage-analysis.md`
  - Identify which variants can be merged
  - Reference: "See research/error-usage-analysis.md"

### 3.2 Consolidate Error Types
- [ ] **Task 3.2.1**: Create simplified error enum
  - Define new minimal RqbitFuseError with 8 variants
  - Keep: NotFound, PermissionDenied, TimedOut, NetworkError(String), IoError(String), InvalidArgument, NotReady, Other(String)
  - Remove all other 20 variants
  
- [ ] **Task 3.2.2**: Update error mappings
  - Update `to_errno()` method for new variants
  - Ensure all existing error mappings are preserved
  - Update `is_transient()` method
  - Update `is_server_unavailable()` method
  
- [ ] **Task 3.2.3**: Update error conversions
  - Update `From<std::io::Error>` implementation
  - Update `From<reqwest::Error>` implementation
  - Update `From<serde_json::Error>` implementation
  - Update `From<toml::de::Error>` implementation
  
- [ ] **Task 3.2.4**: Update all error usage sites
  - Find and replace all usages of removed error variants
  - Map old variants to appropriate new variants
  - Update error messages to be user-friendly

---

## Phase 4: Metrics System Reduction

Replace 520 lines of metrics with 3 essential metrics.

### 4.1 Research Metrics Usage
- [ ] **Task 4.1.1**: Research - Document metrics usage
  - Search for all calls to metrics recording methods
  - Identify which metrics are actually logged/displayed
  - Write findings to `research/metrics-usage-analysis.md`
  - Determine which 3 metrics are most valuable
  - Reference: "See research/metrics-usage-analysis.md"

### 4.2 Simplify Metrics System
- [ ] **Task 4.2.1**: Create minimal metrics struct
  - Define new `Metrics` struct with only: bytes_read, error_count, cache_hits, cache_misses
  - Remove FuseMetrics, ApiMetrics, CacheMetrics structs
  - Keep only atomic counters, remove all helper methods
  
- [ ] **Task 4.2.2**: Remove metrics recording calls
  - Remove all metrics recording except for the 4 essential counters
  - Delete calls to record_getattr, record_setattr, record_lookup, etc.
  - Keep only: record_read (for bytes_read), record_error, record_cache_hit, record_cache_miss
  
- [ ] **Task 4.2.3**: Remove periodic logging
  - Remove `spawn_periodic_logging()` method
  - Remove `log_periodic()` method
  - Remove `log_full_summary()` method
  - Keep only simple logging on shutdown if needed
  
- [ ] **Task 4.2.4**: Update all metrics usages
  - Update `src/lib.rs` to use simplified metrics
  - Update `src/fs/filesystem.rs` to use simplified metrics
  - Update `src/cache.rs` to use simplified metrics
  - Update `src/api/client.rs` to remove ApiMetrics usage

---

## Phase 5: File Handle Tracking Simplification

Remove prefetching code and state tracking.

### 5.1 Remove Prefetching Infrastructure
- [ ] **Task 5.1.1**: Research - Document prefetching code
  - Read `src/types/handle.rs` and identify prefetching-related code
  - Check if prefetching is ever enabled in practice
  - Write findings to `research/prefetching-analysis.md`
  - Reference: "See research/prefetching-analysis.md"
  
- [ ] **Task 5.1.2**: Remove FileHandleState struct
  - Delete `FileHandleState` struct from `src/types/handle.rs`
  - Remove `state: Option<FileHandleState>` field from FileHandle
  - Remove `init_state()`, `update_state()`, `is_sequential()`, `sequential_count()` methods
  - Remove `set_prefetching()`, `is_prefetching()` methods
  
- [ ] **Task 5.1.3**: Remove FileHandleManager state methods
  - Remove `update_state()` method
  - Remove `set_prefetching()` method
  - Remove any methods that operated on state
  
- [ ] **Task 5.1.4**: Remove prefetching from filesystem
  - Remove `track_and_prefetch()` method from `src/fs/filesystem.rs`
  - Remove `do_prefetch()` method
  - Remove call to `track_and_prefetch()` in read handler
  - Remove `prefetch_enabled` config check

### 5.2 Simplify File Handle Cleanup
- [ ] **Task 5.2.1**: Remove TTL-based cleanup
  - Remove `created_at` field from FileHandle
  - Remove `is_expired()` method
  - Remove `remove_expired_handles()` from FileHandleManager
  - Remove `count_expired()` method
  - Remove `start_handle_cleanup()` from filesystem
  - Remove `stop_handle_cleanup()` from filesystem
  - Remove cleanup_handle field from TorrentFS
  
- [ ] **Task 5.2.2**: Remove memory tracking
  - Remove `memory_usage()` method from FileHandleManager
  - Remove calls to memory usage tracking
  
- [ ] **Task 5.2.3**: Simplify FileHandle struct
  - Keep only: fh, inode, torrent_id, flags
  - Remove all other fields and methods

---

## Phase 6: FUSE Logging Simplification

Replace macros with direct tracing calls.

### 6.1 Replace FUSE Macros
- [ ] **Task 6.1.1**: Research - Document macro usage
  - Find all usages of fuse_log!, fuse_error!, fuse_ok! macros
  - Find all usages of reply_* macros
  - Write findings to `research/fuse-macro-usage.md`
  - Reference: "See research/fuse-macro-usage.md"
  
- [ ] **Task 6.1.2**: Replace fuse_log! macro
  - Replace all `fuse_log!` calls with `tracing::debug!`
  - Remove conditional check (tracing handles filtering)
  - Update `src/fs/filesystem.rs`
  
- [ ] **Task 6.1.3**: Replace fuse_error! macro
  - Replace all `fuse_error!` calls with `tracing::debug!` or `tracing::error!`
  - Include error code in message
  - Update all error logging sites
  
- [ ] **Task 6.1.4**: Replace fuse_ok! macro
  - Replace all `fuse_ok!` calls with `tracing::debug!`
  - Include success info in message
  - Update all success logging sites
  
- [ ] **Task 6.1.5**: Replace reply_* macros
  - Replace `reply_ino_not_found!` with direct error recording and reply
  - Replace `reply_not_directory!` with direct error recording and reply
  - Replace `reply_not_file!` with direct error recording and reply
  - Replace `reply_no_permission!` with direct error recording and reply
  
- [ ] **Task 6.1.6**: Remove macro definitions
  - Delete `src/fs/macros.rs` file
  - Remove module declaration from `src/fs/mod.rs`
  - Remove macro imports from `src/fs/filesystem.rs`

---

## Phase 7: Cache Layer Simplification

Remove redundant caching and simplify cache implementation.

### 7.1 Remove Bitfield Cache
- [ ] **Task 7.1.1**: Research - Document bitfield cache usage
  - Analyze `status_bitfield_cache` in `src/api/client.rs`
  - Check if it's actually providing value vs fetching fresh
  - Write findings to `research/bitfield-cache-analysis.md`
  - Reference: "See research/bitfield-cache-analysis.md"
  
- [ ] **Task 7.1.2**: Remove bitfield cache fields
  - Remove `status_bitfield_cache` field from RqbitClient
  - Remove `status_bitfield_cache_ttl` field
  - Remove `TorrentStatusWithBitfield` struct if unused
  
- [ ] **Task 7.1.3**: Update get_torrent_status_with_bitfield
  - Change to fetch fresh data without caching
  - Or remove method if only used for caching
  - Update all call sites
  
- [ ] **Task 7.1.4**: Update check_range_available
  - Ensure it works without cached bitfield
  - May need to fetch bitfield synchronously

### 7.2 Simplify Cache Implementation
- [ ] **Task 7.2.1**: Research - Document cache wrapper value
  - Analyze if Cache struct in `src/cache.rs` adds value over direct moka usage
  - Check if stats tracking is worth the overhead
  - Write findings to `research/cache-wrapper-analysis.md`
  - Reference: "See research/cache-wrapper-analysis.md"
  
- [ ] **Task 7.2.2**: Consider direct moka usage
  - Evaluate replacing Cache wrapper with direct MokaCache usage
  - If keeping wrapper, remove stats tracking
  - Simplify to minimal wrapper

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
- [ ] **Task 8.2.1**: Identify tests to remove
  - Review all tests in `src/api/streaming.rs` test section
  - Mark tests that verify reqwest/wiremock behavior
  - Keep only: basic stream test, one 200-vs-206 test, one concurrent test
  
- [ ] **Task 8.2.2**: Remove redundant streaming tests
  - Remove forward seek within/beyond limit tests (behavioral, not correctness)
  - Remove backward seek tests
  - Remove EOF boundary tests (checked at FUSE layer)
  - Remove rapid alternating seek tests
  - Keep ~150 lines of tests

### 8.3 Review Other Test Files
- [ ] **Task 8.3.1**: Review `tests/performance_tests.rs`
  - Keep if valuable for CI
  - Consider moving to benchmarks only
  
- [ ] **Task 8.3.2**: Review `benches/performance.rs`
  - Keep benchmarks for performance tracking
  - Remove if not used in CI

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
