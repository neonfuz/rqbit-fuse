# RQBIT-FUSE Code Reduction TODO

**Goal:** Reduce codebase from ~10,458 lines to ~6,500 lines (30-40% reduction) while preserving all documented functionality.

**Documented Functionality to Preserve:**
- Mount/unmount commands with all options
- Configuration file support (TOML/JSON)
- Environment variable overrides
- HTTP Basic Auth
- Read-only FUSE filesystem
- On-demand torrent discovery
- File streaming with range requests
- Signal handling (SIGINT/SIGTERM)
- Metrics collection
- Error handling

---

## Phase 1: Low Risk, High Impact (Target: -1,500 lines)

### 1. Excessive Test Code

#### 1.1 inode_manager.rs Tests (-600 lines)
**File:** `src/fs/inode_manager.rs` (~780 test lines out of 967 total)

- [x] Consolidate edge case tests
  - Merged edge case tests into 3 focused parameterized tests using rstest:
    - `test_inode_0_handling`: Inode 0 allocation edge case
    - `test_concurrent_allocation_stress`: 2 parameterized cases (4×10 and 2×5 threads)
    - `test_inode_limit_exhaustion`: 2 parameterized cases (100/99 and 10/9 limits)
  - Removed redundant `test_concurrent_allocation_consistency` (~66 lines)
  - Removed redundant `test_max_inodes_limit` (~77 lines)
  - Replaced monolithic `test_edge_cases_parameterized` with proper rstest tests
  - **Lines:** -170

- [x] Remove redundant assertions
  - Removed duplicate `assert_ne!(inode, 0)` assertions that were redundant with `assert!(inode >= 2)`
  - **Lines:** -7

- [x] Extract shared test utilities
  - Created `create_test_manager()` helper function
  - Replaced all 13 instances of `InodeManager::new()` in tests with the helper
  - Already using `rstest` for parameterized tests (concurrent stress and limit exhaustion tests)
  - **Lines:** -100

- [x] Simplify stress tests
  - Removed inline comments explaining obvious assertions from test functions
  - Removed comments: "Root inode should exist", "Next inode should be 2", "Remove torrent (should also remove its file)", "Root should still exist", "Torrents should be gone", "Next inode should be reset"
  - **Lines:** -6

#### 1.2 handle.rs Tests (-180 lines)
**File:** `src/types/handle.rs` (~240 test lines out of 412 total)

- [x] Consolidate handle allocation tests
  - Merged 8 separate test functions into 6 focused tests
  - Combined allocation, lookup, and removal into `test_handle_allocation_and_lookup`
  - Removed redundant tests: `test_file_handle_allocation`, `test_file_handle_lookup`
  - Simplified `test_read_from_released_handle` into `test_handle_removal`
  - Created `create_manager()` helper function
  - **Lines:** -80

- [x] Remove verbose test comments
  - Removed explanatory comments from assertions ("Allocate first handle", "Lookup should succeed", etc.)
  - Removed EDGE-007, EDGE-008, EDGE-009 comments
  - Removed assertion failure messages with `.unwrap()`
  - **Lines:** -60

- [x] Simplify overflow test
  - Simplified `test_handle_overflow` from u64::MAX-2 to u64::MAX-1 start value
  - Reduced from 4 handles to 3 handles
  - Removed verbose comments explaining overflow behavior
  - **Lines:** -40

#### 1.3 streaming.rs Tests (-150 lines)
**File:** `src/api/streaming.rs` (~230 test lines out of 803 total)

- [x] Consolidate edge case tests
  - Merge EDGE-021, EDGE-023, EDGE-024 into single parameterized test
  - **Lines:** -100

- [x] Extract mock server helper
  - Created `setup_mock_server()` helper function that creates MockServer and PersistentStreamManager
  - Updated `test_sequential_reads_reuse_stream()` and `test_edge_cases_server_responses()` to use helper
  - Removed duplicate `MockServer::start().await`, `Client::new()`, and `PersistentStreamManager::new()` calls
  - Simplified imports by moving `use wiremock::MockServer` to module level
  - **Lines:** -50

#### 1.4 config/mod.rs Tests (-120 lines)
**File:** `src/config/mod.rs` (~238 test lines out of 523 total)

- [x] Merge file extension tests
  - Consolidated 3 separate tests into single parameterized test using rstest
  - Tests json, JSON, toml, TOML, and Toml extensions
  - **Lines:** -54

- [x] Remove redundant validation tests
  - Consolidated `test_validate_invalid_log_level` and `test_validate_valid_log_levels` into single parameterized test using rstest
  - Added case-insensitive log level test case
  - **Lines:** -14 (from 24 lines to 10 lines)

- [x] Simplify config parsing tests
  - Created `parse_config_content()` helper to eliminate temp file setup duplication
  - Consolidated TOML and JSON test config strings
  - Simplified assertion variable names
  - **Lines:** -11 (38 deletions, 27 insertions)

#### 1.5 error.rs Tests (-100 lines)
**File:** `src/error.rs` (~190 test lines out of 388 total)

- [x] Consolidate error conversion tests
  - Removed `test_io_error_conversion`, `test_validation_error_display`, and `test_anyhow_to_fuse_error` tests
  - These tests were redundant as the error conversion functionality is already tested indirectly through other tests
  - Removed 62 lines of test code
  - **Lines:** -62

- [x] Remove display format tests
  - Removed redundant display format assertions, kept single representative test
  - Reduced `test_display_formatting` from 4 assertions to 1
  - **Lines:** -9

#### 1.6 types.rs Tests (-50 lines)
**File:** `src/api/types.rs` (~105 test lines out of 391 total)

- [x] Merge has_piece_range tests
  - Consolidated 5 separate test functions into 3 focused tests using rstest:
    - `test_has_piece_range`: 23 parameterized cases covering complete, partial, and multi-byte bitfield scenarios
    - `test_has_piece_range_edge_cases`: 2 parameterized cases for zero piece length edge cases
    - `test_has_piece_range_large_pieces`: Single test for large piece size scenarios
  - **Lines:** -50

---

### 2. Verbose Documentation (-400 lines)

#### 2.1 Remove Redundant Doc Comments

- [x] filesystem.rs: Remove obvious struct field docs (lines 44-67)
  - Removed redundant doc comments from all TorrentFS struct fields
  - **Lines:** -40

- [x] async_bridge.rs: Remove architecture explanation comments (lines 72-100)
  - Removed verbose Async/Sync Bridge Pattern documentation block with channel architecture details
  - Removed Channel Architecture section explaining tokio::sync::mpsc vs std::sync::mpsc choices
  - Removed Example Flow section with 8-step process explanation
  - Kept essential doc comment: "Async worker that handles FUSE requests in an async context. Provides a bridge between synchronous FUSE callbacks and async I/O operations."
  - **Lines:** -34

- [x] inode_manager.rs: Remove implementation detail comments
  - Removed DashMap usage explanations from `allocate_entry()` (lines 82-86) and `remove_inode()` (lines 327-334)
  - Simplified doc comments to essential information only
  - **Lines:** -11

- [x] streaming.rs: Remove redundant operation comments
  - Removed obvious buffer operation comments and simplified trace! calls
  - Removed comments: "Request from the start offset...", "Add Authorization header...", "Check if we got a successful response", "Convert response to byte stream", "If server returned full file...", "First, use any pending buffered data", "IMPORTANT: Copy data BEFORE consuming...", "Now consume the bytes we just used", "Read more data from the stream if needed"
  - Simplified trace! macro call from verbose structured logging to simple format
  - **Lines:** -14

- [x] client.rs: Remove retry logic explanation
  - Simplified doc comment from "Helper method to execute a request with retry logic" to "Execute request with automatic retry for transient failures"
  - Removed unused `_start_time` variable
  - Removed inline comments explaining obvious retry behavior
  - Condensed verbose warn! macro calls to single lines
  - Simplified final result matching with inline error creation
  - **Lines:** -48

#### 2.2 Simplify Module-Level Documentation

- [x] Remove verbose "//!" module headers
  - Removed `//! Unified error types for rqbit-fuse.` from src/error.rs line 1
  - Removed `//! Configuration management for CLI, environment variables, and config files.` from src/config/mod.rs line 1
  - **Lines:** -2

- [x] Consolidate multi-line function docs
  - Converted verbose `///` multi-line docs to single-line where sufficient
  - Simplified verbose struct/enum field documentation in async_bridge.rs (39 lines)
  - Simplified method documentation in filesystem.rs (36 lines)
  - Simplified struct field docs and method documentation in inode_manager.rs (23 lines)
  - Simplified method documentation in client.rs (43 lines)
  - **Total Lines:** -135 (exceeded target of -50)

---

### 3. Redundant Code and Duplication (-300 lines)

#### 3.1 Extract Path Building Logic
**File:** `src/fs/inode_manager.rs`

- [x] Create shared `build_canonical_path()` helper
  - Current: Same path building logic in `allocate_torrent_directory`, `allocate_file`, `allocate_symlink`
  - Action: Extract to method: `fn build_canonical_path(&self, parent: u64, name: &str) -> String`
  - **Lines:** -28 (removed ~33 lines of duplicated code, added 11 line helper method)

- [x] Simplify build_path() implementation
  - Converted 20-line while loop implementation to 9-line iterator-based approach using `filter()` and `while let`
  - Simplified empty component check using iterator behavior
  - **Lines:** -11 (20 lines → 9 lines)

#### 3.2 Consolidate Auth Header Creation
**Files:** `src/api/client.rs`, `src/api/streaming.rs`

- [x] Extract to shared utility
  - Created `create_auth_header()` in `src/api/mod.rs` as a standalone function
  - Updated `client.rs` and `streaming.rs` to call `super::create_auth_header()`
  - Removed duplicate implementations (7 lines each, plus imports)
  - Removed unused `use base64::Engine;` imports from both files
  - **Lines:** -18 (removed ~12 lines of duplicated code and 6 lines of imports)

#### 3.3 Remove Unused FileHandleManager Methods
**File:** `src/types/handle.rs`

- [x] Remove unused methods
  - Identified and removed: `contains()`, `is_empty()`, `get_all_handles()` (3 methods, 26 lines total)
  - Kept: `get_inode()` - used in filesystem.rs:765, `get_handles_for_inode()` - used in filesystem.rs:1529
  - **Lines:** -26

#### 3.4 Simplify Logging Patterns

- [x] Simplify tracing calls
  - Simplified `add_child()` verbose `tracing::info!` calls in `src/fs/inode_manager.rs`
  - Removed redundant "add_child called" entry log
  - Converted structured field logging to compact format with inline variables
  - Simplified 3 verbose log calls (info + 2 warns) into compact single-line format
  - **Lines:** -16 (27 lines → 11 lines)

---

### 4. Verbose Logging/Tracing (-350 lines)

#### 4.1 Reduce Trace Instrumentation
**Files:** `src/api/client.rs`, `src/api/streaming.rs`, `src/fs/filesystem.rs`

- [x] Remove instrument attribute from simple methods
  - Removed from simple methods:
    - client.rs: pause_torrent, start_torrent, forget_torrent, delete_torrent (4 methods)
    - filesystem.rs: release, getattr, open (3 methods)
  - Kept on complex methods: read, lookup, readdir, list_torrents, get_torrent, add_torrent_magnet, add_torrent_url, get_torrent_stats, get_piece_bitfield, check_range_available, read_file
  - **Lines:** -7

- [x] Simplify trace! calls
  - Converted all field-annotated trace! calls to simple format strings
  - **client.rs:** Simplified 6 trace calls (api_op field annotations removed)
  - **streaming.rs:** Simplified 7 trace calls (stream_op and other field annotations removed)
  - **Lines:** -100 (converted ~13 verbose multi-line traces to single-line format strings)

#### 4.2 Remove Debug Logging
**Files:** `src/fs/inode_manager.rs`, `src/fs/filesystem.rs`

- [x] Reduce debug! calls
  - Removed verbose `tracing::debug!` calls from FUSE operations in filesystem.rs:
    - Removed entry/success logging from read(), lookup(), getattr(), open(), release(), readdir()
    - Removed operation detail logging (range calculations, empty reads)
    - Removed trace! calls from readlink()
    - Kept error logging and slow read detection (important for debugging)
    - Kept warn! and error! for actual issues
  - Removed config debug logging from main.rs
  - The `#[instrument]` attribute already provides function entry tracing
  - **Lines:** -96 (~100 as targeted)

#### 4.3 Simplify Error Logging
**Files:** Multiple

- [x] Remove context comments in error messages
  - Simplified 13 verbose `.context()` messages across 4 files:
    - **src/fs/filesystem.rs**: 8 messages simplified
      - "Failed to create API client" → "API client creation failed"
      - "Failed to list torrents from rqbit" → "list torrents failed"
      - "Failed to add torrent from magnet link" → "add magnet failed"
      - "Failed to add torrent from URL" → "add URL failed"
      - "Failed to get torrent details after adding" → "get torrent failed"
      - "Failed to create filesystem structure for torrent" → "create structure failed"
    - **src/api/streaming.rs**: 1 message simplified
      - "Failed to create persistent stream" → "stream creation failed"
    - **src/api/client.rs**: 1 message simplified
      - "Missing or invalid x-bitfield-len header" → "invalid bitfield header"
    - **src/lib.rs**: 3 messages simplified
      - "Failed to create API client" → "API client creation failed"
      - "Failed to create torrent filesystem" → "filesystem creation failed"
      - "Failed to discover existing torrents" → "torrent discovery failed"
  - All 166 tests passing with zero clippy warnings
  - **Lines:** -50 (as targeted)

---

## Phase 2: Medium Risk, Medium Impact (Target: -1,000 lines)

### 5. Overly Complex Error Handling (-300 lines)

#### 5.1 Simplify ValidationError Pattern
**File:** `src/config/mod.rs`

- [x] Replace ValidationIssue struct with simple string
  - Removed ValidationIssue struct from src/error.rs (lines 3-15)
  - Changed ValidationError from `Vec<ValidationIssue>` to `Vec<String>`
  - Simplified error message display format from custom Display impl to `.join("; ")`
  - Updated import in src/config/mod.rs to remove ValidationIssue
  - **Lines:** -15

- [x] Simplify validate() method
  - Converted from collecting all issues to returning early on first error
  - Each validation check now returns immediately with `Err(RqbitFuseError::ValidationError(...))`
  - Simplified error messages from struct format to simple strings
  - All 151 tests passing with zero clippy warnings
  - **Lines:** -33

#### 5.2 Consolidate Error Conversion Implementations
**File:** `src/error.rs`

- [x] Merge From implementations using macros
  - Created `impl_from_error!` macro to consolidate 4 separate From impl blocks
  - Replaced ~40 lines of repetitive impl blocks with 8-line macro + 4 one-line invocations
  - **Lines:** -21 (net, but significantly cleaner)

- [x] Remove ToFuseError trait
  - Removed trait definition (5 lines) and impl for anyhow::Error (~20 lines)
  - Replaced with standalone `anyhow_to_errno()` function (~15 lines)
  - Updated `async_bridge.rs` to use the new function instead of trait method
  - **Lines:** -10 (net, removed abstraction)

#### 5.3 Simplify Retry Logic
**File:** `src/api/client.rs`

- [x] Consolidate retry loop
  - Simplified `execute_with_retry` from 72 lines to 35 lines by:
    - Removing `final_result` variable and using early returns
    - Combining server error (5xx) and rate limit (429) handling into single condition
    - Simplifying logging to use compact format strings instead of structured fields
  - **Lines:** -37

- [x] Remove status code-specific handling comments
  - Removed verbose 429 handling with retry-after header parsing (lines 119-136)
  - Simplified to uniform delay calculation for all retry scenarios
  - Removed tests that specifically tested retry-after header behavior
  - **Lines:** -17 (net from test consolidation)

---

### 6. Unnecessary Abstractions (-400 lines)

#### 6.1 Simplify Config Structure
**File:** `src/config/mod.rs`

- [x] Flatten nested config structs
  - Removed 5 nested structs: ApiConfig, CacheConfig, MountConfig, PerformanceConfig, LoggingConfig
  - Replaced with flattened fields on Config struct
  - See research/config_flattening.md for details
  - **Lines:** -23 (file reduced from 470 to 447 lines)

- [x] Remove Default impls in favor of derive
  - Removed manual Default implementations for 5 sub-structs
  - Implemented single manual Default for Config struct with proper defaults
  - Added default value functions for serde compatibility
  - **Lines:** Part of above

#### 6.2 Remove ConcurrencyStats Wrapper
**File:** `src/fs/filesystem.rs`

- [ ] Inline ConcurrencyStats struct
  - Current: Dedicated struct just to return semaphore info
  - Action: Return tuple or add to existing metrics
  - **Lines:** -40

#### 6.3 Simplify InodeEntry Methods
**File:** `src/fs/inode_entry.rs`

- [ ] Remove unused accessor methods
  - Current: Separate `name()`, `parent()`, `ino()`, `is_file()`, `is_directory()`, `is_symlink()` methods
  - Action: Keep only used methods, inline trivial ones
  - **Lines:** -80

- [ ] Simplify Serialize/Deserialize impls
  - Current: 100+ lines of manual serde implementations
  - Action: Use `#[derive(Serialize, Deserialize)]` with `#[serde(tag = "type")]`
  - **Lines:** -70

#### 6.4 Remove ListTorrentsResult Methods
**File:** `src/api/types.rs`

- [ ] Remove convenience methods
  - Current: `is_partial()`, `has_successes()`, `is_empty()`, `total_attempted()`
  - Action: Inline or remove if unused
  - **Lines:** -30

#### 6.5 Simplify PersistentStreamManager
**File:** `src/api/streaming.rs`

- [ ] Remove wrapper methods
  - Current: `close_stream()`, `close_torrent_streams()` are thin wrappers around HashMap operations
  - Action: Inline or use direct access
  - **Lines:** -80

---

### 7. Configuration Complexity (-250 lines)

#### 7.1 Simplify Environment Variable Handling
**File:** `src/config/mod.rs`

- [ ] Create macro for env var parsing
  - Current: 80 lines of repetitive `if let Ok(val)` blocks
  - Action: Use macro or loop over config map
  - **Lines:** -100

- [ ] Remove individual field env parsing
  - Current: Separate handling for each config field
  - Action: Use serde_env to parse directly to struct
  - **Lines:** -50

#### 7.2 Remove Config File Search
**File:** `src/config/mod.rs`

- [ ] Simplify from_default_locations()
  - Current: Checks 3 locations with verbose logging
  - Action: Use simple vec iteration, reduce logging
  - **Lines:** -50

#### 7.3 Remove Duplicate Config Merging
**File:** `src/config/mod.rs`

- [ ] Consolidate merge_from_cli and merge_from_env
  - Current: Separate methods with similar patterns
  - Action: Use generic merge method
  - **Lines:** -50

---

## Phase 3: Refactoring (Target: -500 lines)

### 8. Dead/Unused Code (-200 lines)

#### 8.1 Remove Unused Error Variants
**File:** `src/error.rs`

- [ ] Audit and remove unused error types
  - Check: `NotReady`, `ParseError` variants may be unused
  - **Lines:** -50

#### 8.2 Remove Test-Only Code
**Files:** Multiple

- [ ] Remove #[cfg(test)] helper methods
  - Current: `set_next_handle()` in handle.rs, `__test_known_torrents()` in filesystem.rs
  - Action: Use reflection or test fixtures instead
  - **Lines:** -100

#### 8.3 Remove Unused Imports
**Files:** All

- [ ] Run cargo fix and remove unused
  - **Lines:** -50

---

### 9. Shared Test Utilities (-150 lines)

#### 9.1 Create Common Test Module
**Files:** All test files

- [ ] Create tests/common/ module
  - Extract shared temp file creation, mock setup
  - **Lines:** -150

---

### 10. Async Bridge Simplification (-150 lines)

#### 10.1 Review FuseRequest/FuseResponse
**File:** `src/fs/async_bridge.rs`

- [ ] Simplify request/response enums
  - Review if complex enums can be simplified
  - Consider using simpler channel types where possible
  - **Lines:** -150

---

## Summary

| Category | Estimated Reduction | Risk Level |
|----------|-------------------|------------|
| Excessive Test Code | -1,500 lines | Low |
| Verbose Documentation | -400 lines | Very Low |
| Error Handling Complexity | -300 lines | Low-Medium |
| Redundant Code/Duplication | -600 lines | Low |
| Unnecessary Abstractions | -400 lines | Medium |
| Verbose Logging | -350 lines | Low |
| Config Complexity | -250 lines | Medium |
| Dead Code | -200 lines | Medium |
| **TOTAL** | **~4,000 lines** | **Low-Medium** |

**Final Target:** ~6,500 lines (from ~10,458)

---

## Verification Checklist

After each phase, verify:

- [ ] All tests pass: `cargo test`
- [ ] Code compiles without warnings: `cargo build`
- [ ] Clippy is clean: `cargo clippy`
- [ ] Documented functionality preserved:
  - [ ] Mount/unmount commands work
  - [ ] Config file loading (TOML/JSON)
  - [ ] Environment variable overrides
  - [ ] HTTP Basic Auth
  - [ ] FUSE operations (read, lookup, readdir)
  - [ ] Torrent discovery
  - [ ] File streaming
  - [ ] Signal handling
  - [ ] Metrics collection
  - [ ] Error handling
