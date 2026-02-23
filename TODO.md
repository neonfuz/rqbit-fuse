# rqbit-fuse Edge Case Testing Checklist

> **Last Updated:** February 22, 2026
> 
> **Recent Changes:**
> - ✅ Marked SIMPLIFY-004 and SIMPLIFY-005 as completed
> - ✅ Removed EDGE-044 (duplicate of EDGE-008)
> - 🔄 Updated priorities: SIMPLIFY-001 and SIMPLIFY-002 are now HIGH priority
> - 📝 Updated file sizes: filesystem.rs is 3,116 lines (was 1,434)
> - ⬇️ Lowered SIMPLIFY-003 priority (minimal benefit for breaking change)

## How to Use This File

Each item is designed to be completed independently. These are edge case tests to improve test coverage beyond the standard scenarios.

**Workflow:**
1. Pick an unchecked item
2. Read the test description and implementation notes
3. Write the test(s)
4. Ensure test passes
5. Check the box
6. Commit your changes

---

## Phase 1: Read/Offset Boundary Edge Cases

### FUSE Read Operations (tests/fuse_operations.rs)

- [x] **EDGE-001**: Test read at EOF boundary
  - Read at offset = `file_size - 1` (only 1 byte remaining)
  - Read at offset exactly equal to file_size (should return 0 bytes/EOF)
  - Verify no panic or error, just empty data
  - Test file sizes: 1 byte, 4096 bytes (block size), 1MB, 1GB
  - Implemented: 9 tests across streaming and fuse_operations modules

- [x] **EDGE-002**: Test zero-byte reads
  - Read with size = 0 at various offsets
  - Read at offset = 0 with size = 0
  - Should return success with empty data, not error
  - Implemented: 3 tests in tests/fuse_operations.rs

- [x] **EDGE-003**: Test negative offset handling
  - Read with offset = -1 (i64::MAX as u64 overflow)
  - Read with offset = i64::MIN
  - Should handle gracefully without panic
  - Implemented: 3 tests in tests/fuse_operations.rs covering negative offsets, i64::MIN, and overflow scenarios

- [x] **EDGE-004**: Test read beyond EOF
  - Request more bytes than remaining in file
  - Read starting at offset > file_size
  - Should return available bytes or empty, not error
  - Implemented: 4 tests in tests/fuse_operations.rs

- [x] **EDGE-005**: Test piece boundary reads
  - Read starting exactly at piece boundary
  - Read ending exactly at piece boundary
  - Read spanning multiple piece boundaries
  - Verify correct data returned across boundaries

---

## Phase 2: File Handle Edge Cases

### File Handle Management (src/types/handle.rs)

- [x] **EDGE-006**: Test double release of handle
  - Allocate handle, release it, release same handle again
  - Should not panic, should handle gracefully
  - Verify no memory corruption
  - **Completed**: Test already exists in `src/types/handle.rs:test_file_handle_removal` - verifies second removal returns `None` without panic

- [x] **EDGE-007**: Test read from released handle
  - Open file, get handle, release handle, try to read
  - Should return EBADF error
  - Verify no panic or crash
  - Implemented: test_read_from_released_handle in src/types/handle.rs

- [x] **EDGE-008**: Test handle exhaustion
  - Open files until handle limit reached (50 streams)
  - Try to open one more file
  - Should return appropriate error (EMFILE or EAGAIN)
  - Verify proper error message
  - Implemented: test_handle_exhaustion in src/types/handle.rs with max_handles=5

- [x] **EDGE-009**: Test handle overflow
  - Simulate handle allocation wrapping past u64::MAX
  - Verify handle uniqueness is maintained
  - Should not allocate handle 0
  - Implemented: `test_handle_overflow` in src/types/handle.rs with overflow protection in allocate() method

- [x] **EDGE-010**: Test TTL expiration of handles
  - Create handle, wait for TTL (1 hour), access handle
  - Should be cleaned up and return error
  - Test with artificially shortened TTL for test speed
  - Implemented: 3 tests covering basic TTL expiration, multiple handles with staggered creation, and direct is_expired() method testing

---

## Phase 3: Directory Operations Edge Cases

### Directory Listing (src/fs/filesystem.rs)

- [x] **EDGE-011**: Test readdir with invalid offsets
  - Offset > number of entries
  - Offset = i64::MAX
  - Negative offset
  - Should handle gracefully
  - Implemented: 4 tests in tests/fuse_operations.rs

- [x] **EDGE-012**: Test readdir on non-directory
  - Try to list contents of a file
  - Should return ENOTDIR error
  - Implemented: `test_error_enotdir_file_as_directory` in `tests/fuse_operations.rs` tests that files have no children and nested lookups inside files fail

- [x] **EDGE-013**: Test lookup of special entries
  - Lookup "." in root directory
  - Lookup ".." in various directories
  - Should resolve correctly
  - Implemented: Added special handling for "." and ".." in lookup() callback and 5 comprehensive tests

- [x] **EDGE-014**: Test empty directory listing
  - Create empty directory, list contents
  - Should return "." and ".." only  
  - Verify no crash or error
  - Implemented: test_edge_014_empty_directory_listing in tests/fuse_operations.rs

- [x] **EDGE-015**: Test directory with many files
  - Create directory with 1000+ files
  - List contents with various offsets
  - Verify pagination/offset works correctly
  - Implemented: test_edge_015_directory_with_many_files in tests/fuse_operations.rs

---

## Phase 4: Cache Edge Cases

### Cache Operations (src/cache.rs)

- [x] **EDGE-016**: Test cache entry expiration during access
  - Insert entry with short TTL
  - Start get() operation
  - Let entry expire during operation
  - Should return None, not panic
  - **Completed**: Added `test_cache_entry_expiration_during_access` and `test_cache_expiration_race_condition` tests

- [x] **EDGE-017**: Test cache at exact capacity
  - Fill cache to exactly max_entries
  - Insert one more entry (should trigger eviction)
  - Verify eviction count increments
  - Verify oldest entry is evicted
  - **Completed**: Verified by existing `test_cache_lru_eviction` test in src/cache.rs

- [x] **EDGE-018**: Test rapid insert/remove cycles
  - Insert and remove same key 1000 times rapidly
  - Should maintain consistency
  - No memory leaks
  - Implemented: `test_cache_rapid_insert_remove_cycles` and `test_cache_rapid_mixed_key_cycles` in src/cache.rs

- [x] **EDGE-019**: Test concurrent insert of same key
  - 10 threads try to insert same key simultaneously
  - One should succeed, others should handle gracefully
  - Cache should have exactly one entry
  - **Implemented**: `test_concurrent_insert_same_key` in `src/cache.rs`

- [x] **EDGE-020**: Test cache statistics edge cases
  - Hit rate with 0 total requests
  - Hit rate with 0 hits, many misses
  - Hit rate with 0 misses, many hits
  - Should not divide by zero

---

## Phase 5: Streaming Edge Cases

### HTTP Streaming (src/api/streaming.rs)

- [x] **EDGE-021**: Test server returning 200 instead of 206
  - Request range, server returns full file (200 OK)
  - Should handle correctly, skip to offset
  - Verify data correctness
  - Implemented: 3 tests in `src/api/streaming.rs`

- [x] **EDGE-022**: Test empty response body
  - Server returns 200/206 with empty body
  - Should handle gracefully, return empty bytes
  - No panic or infinite loop
  - Implemented: 3 tests in `src/api/streaming.rs`
    - `test_edge_022_empty_response_body_200`: Tests 200 OK with empty body
    - `test_edge_022_empty_response_body_206`: Tests 206 Partial Content with empty body
    - `test_edge_022_empty_response_at_offset`: Tests empty response at non-zero offset

- [x] **EDGE-023**: Test network disconnect during read
  - Start reading stream
  - Simulate network failure mid-read
  - Should return error, clean up properly
  - No resource leaks
  - **Implemented**: 3 tests in `src/api/streaming.rs`
    - `test_edge_023_network_disconnect_during_read`: Tests graceful handling of network issues
    - `test_edge_023_stream_marked_invalid_after_error`: Verifies stream invalidation on error
    - `test_edge_023_stream_manager_cleanup_invalid_stream`: Tests manager cleanup of invalid streams

- [x] **EDGE-024**: Test slow server response
  - Server sends data very slowly
  - Should respect timeout
  - Should not block indefinitely
  - Implemented: 3 tests in `src/api/streaming.rs`
    - `test_edge_024_slow_server_response`: Tests timeout with 5s delay vs 100ms client timeout
    - `test_edge_024_slow_server_partial_response`: Tests timeout during body read
    - `test_edge_024_normal_server_response`: Control test verifying normal operation

- [x] **EDGE-025**: Test wrong content-length
  - Server returns more/less data than Content-Length header
  - Should handle gracefully
  - Return error or available data
  - Implemented: 3 tests in `src/api/streaming.rs`
    - `test_edge_025_content_length_more_than_header`: Tests when server sends more data than header indicates
    - `test_edge_025_content_length_less_than_header`: Tests when server sends less data than header indicates
    - `test_edge_025_content_length_mismatch_at_offset`: Tests mismatch at non-zero offset
    - Note: HTTP layer (hyper) detects mismatch and returns error, which streaming layer handles gracefully

- [x] **EDGE-026**: Test seek patterns
  - Seek backward by 1 byte (should create new stream)
  - Seek forward exactly MAX_SEEK_FORWARD bytes
  - Rapid alternating forward/backward seeks
  - Verify stream creation/reuse logic
  - Implemented: 4 comprehensive tests in `src/api/streaming.rs`:
    - `test_forward_seek_exactly_max_boundary`: Tests boundary at MAX_SEEK_FORWARD
    - `test_forward_seek_just_beyond_max_boundary`: Tests gap > MAX_SEEK_FORWARD creates new stream
    - `test_rapid_alternating_seeks`: Tests rapid forward/backward seek patterns
    - `test_backward_seek_one_byte_creates_new_stream`: Tests 1-byte backward seek creates new stream

---

## Phase 6: Inode Management Edge Cases

### Inode Allocation (src/fs/inode.rs)

- [x] **EDGE-027**: Test inode 0 allocation attempt
  - Try to allocate inode 0
  - Should fail gracefully, return 0 or error
  - Should not corrupt inode counter
  - Implemented: 2 tests in `src/fs/inode_manager.rs`

- [ ] **EDGE-028**: Test max_inodes limit
  - Set max_inodes = 10
  - Allocate 11 inodes
  - 11th allocation should fail (return 0)
  - Verify no panic

- [ ] **EDGE-029**: Test allocation after clear_torrents
  - Allocate some inodes
  - Call clear_torrents()
  - Allocate more inodes
  - Should reuse inode numbers correctly
  - No duplicates

- [ ] **EDGE-030**: Test concurrent allocation stress
  - 100 threads allocating simultaneously
  - Each thread allocates 100 inodes
  - Verify all inodes are unique
  - No duplicates, no gaps

---

## Phase 7: Path Resolution Edge Cases

### Path Handling (src/fs/inode.rs)

- [ ] **EDGE-031**: Test path traversal attempts
  - Path with ".." traversing above root ("/../secret")
  - Should resolve to root or return error
  - No directory escape

- [ ] **EDGE-032**: Test path with double slashes
  - Path with "//" double slashes
  - Should normalize correctly

- [ ] **EDGE-033**: Test path with "." components
  - Path with "." self-reference
  - Should resolve correctly
  - "./file.txt" should work

- [ ] **EDGE-034**: Test symlink edge cases
  - Circular symlink (a -> b, b -> a)
  - Symlink pointing outside torrent directory
  - Symlink with absolute path
  - Should handle gracefully

- [ ] **EDGE-035**: Test case sensitivity
  - On case-insensitive filesystems (macOS)
  - Look up "FILE.txt" when file is "file.txt"
  - Behavior should be consistent

---

## Phase 8: Error Handling Edge Cases

### API Error Scenarios (src/api/client.rs)

- [ ] **EDGE-036**: Test HTTP 429 Too Many Requests
  - Server returns 429 with Retry-After header
  - Should respect rate limit
  - Should retry appropriately

- [ ] **EDGE-037**: Test malformed JSON response
  - Server returns invalid JSON
  - Should return parse error
  - Should not panic

- [ ] **EDGE-038**: Test timeout at different stages
  - DNS resolution timeout
  - Connection timeout
  - Read timeout
  - Each should return appropriate error

- [ ] **EDGE-039**: Test connection reset
  - Server resets connection mid-request
  - Should handle gracefully
  - Should retry if configured

---

## Phase 9: Concurrency Edge Cases

### Race Conditions (tests/concurrent_tests.rs)

- [ ] **EDGE-040**: Test read while torrent being removed
  - Start reading file
  - Remove torrent mid-read
  - Should handle gracefully
  - No crash or data corruption

- [ ] **EDGE-041**: Test concurrent discovery
  - Trigger discovery from readdir
  - Simultaneously trigger from background task
  - Should not create duplicate torrents
  - Atomic check-and-set should work

- [ ] **EDGE-042**: Test mount/unmount race
  - Start mount operation
  - Immediately unmount
  - Should not panic
  - Resources should be cleaned up

- [ ] **EDGE-043**: Test cache eviction during get
  - Start get() operation
  - Trigger cache eviction simultaneously
  - Should handle gracefully
  - Return valid data or None

---

## Phase 10: Resource Limit Edge Cases

### Resource Exhaustion (tests/resource_tests.rs)

- [x] **EDGE-044**: ~~Test stream limit exhaustion~~
  - ✅ **DUPLICATE**: Already implemented as EDGE-008 in handle.rs
  - Tests handle exhaustion with max_handles=5 (configurable)
  - See `test_handle_exhaustion` in src/types/handle.rs

- [ ] **EDGE-045**: Test inode limit exhaustion
  - Set max_inodes = 100
  - Create 101 torrents
  - 101st should fail gracefully
  - No memory corruption

- [ ] **EDGE-046**: Test cache memory limit
  - Set max_cache_bytes = 1MB
  - Insert data exceeding limit
  - Should trigger eviction
  - Should not crash

- [ ] **EDGE-047**: Test semaphore exhaustion
  - Trigger max_concurrent_reads simultaneously
  - 11th read should wait or fail
  - Should not deadlock

---

## Phase 11: Unicode/Path Edge Cases

### Filename Handling (tests/unicode_tests.rs)

- [ ] **EDGE-048**: Test maximum filename length
  - Filename with 255 characters (max)
  - Should work
  - 256 characters should fail

- [ ] **EDGE-049**: Test null byte in filename
  - Filename containing \0
  - Should sanitize or reject
  - No panic

- [ ] **EDGE-050**: Test control characters
  - Filename with \n, \t, \r, etc.
  - Should sanitize or reject

- [ ] **EDGE-051**: Test UTF-8 edge cases
  - Emoji (multi-byte sequences)
  - CJK characters (Chinese, Japanese, Korean)
  - Right-to-left text (Arabic, Hebrew)
  - Zero-width joiners (emoji sequences)
  - Should handle all correctly

- [ ] **EDGE-052**: Test path normalization
  - NFD vs NFC unicode normalization
  - File created with one form, looked up with other
  - Behavior should be consistent

- [ ] **EDGE-053**: Test maximum path length
  - Path with 4096 characters
  - Should work up to limit
  - 4097 should fail gracefully

---

## Phase 12: Configuration Edge Cases

### Config Validation (tests/config_tests.rs)

- [ ] **EDGE-054**: Test invalid URLs
  - URL without scheme ("localhost:3030")
  - URL with invalid scheme ("ftp://...")
  - Empty URL
  - Should fail validation

- [ ] **EDGE-055**: Test invalid mount points
  - Mount point as file (not directory)
  - Relative path ("./mount")
  - Non-existent path
  - Should fail validation

- [ ] **EDGE-056**: Test timeout edge cases
  - Timeout = 0
  - Timeout = u64::MAX
  - Negative timeout (if parsed from string)
  - Should validate and reject invalid values

- [ ] **EDGE-057**: Test environment variable edge cases
  - Missing required env vars
  - Empty string env var value
  - Very long env var value (>4096 chars)
  - Should handle gracefully

---

*Generated from edge case analysis - February 16, 2026*

---

## Phase 13: Code Simplification Tasks

### Architecture Simplifications

- [ ] **SIMPLIFY-001**: Consolidate Error Types **[HIGH PRIORITY]**
  - Create single `RqbitFuseError` enum replacing:
    - `FuseError` in `fs/error.rs` (178 lines)
    - `ApiError` in `api/types.rs` (lines 14-69)
    - `ConfigError` in `config/mod.rs` (lines 466-476)
  - Currently have duplicate error mappings (e.g., ENOENT in both FuseError and ApiError)
  - Implement `std::error::Error` for all error variants
  - Use `thiserror` derive macros for consistency
  - Update all `anyhow::Result` usages in library code
    - **Sub-tasks:**
    - [x] **SIMPLIFY-001A**: Create unified RqbitFuseError enum in src/error.rs
    - [ ] **SIMPLIFY-001B**: Implement From traits for backward compatibility
    - [ ] **SIMPLIFY-001C**: Migrate api/ module from ApiError to RqbitFuseError
    - [ ] **SIMPLIFY-001D**: Migrate fs/ module from FuseError to RqbitFuseError
    - [ ] **SIMPLIFY-001E**: Migrate config/ module from ConfigError to RqbitFuseError
    - [ ] **SIMPLIFY-001F**: Remove old error types and clean up exports

- [x] **SIMPLIFY-002**: Split Large Files **[HIGH PRIORITY]**
  - Split `src/fs/inode.rs` (1,051 lines) into smaller modules:
    - `src/fs/inode_entry.rs` (~350 lines) - InodeEntry enum and methods
    - `src/fs/inode_manager.rs` (~850 lines) - InodeManager struct and methods
  - Maintained backward compatibility through re-exports in `inode.rs`
  - Updated `fs/mod.rs` to include new module declarations
  - All tests pass, zero clippy warnings
  - **See:** [research/file-splitting-strategy.md](research/file-splitting-strategy.md) for detailed plan
  - **Note:** filesystem.rs split deferred - requires careful coordination with SIMPLIFY-001 (Consolidate Error Types)

- [ ] **SIMPLIFY-003**: Simplify Configuration System **[LOW PRIORITY]**
  - ~~Remove JSON config file support~~ - Keep both, works seamlessly
  - Remove niche options:
    - `piece_check_enabled` (always check pieces)
    - `return_eagain` (always use consistent behavior)
  - Consider using `config` crate instead of custom loading
  - Document breaking changes in migration guide
  - **Note:** Removing JSON provides minimal benefit for breaking change cost

- [x] **SIMPLIFY-004**: Remove Unused Types
  - ✅ **COMPLETED**: Analysis shows:
    - `types/torrent.rs` doesn't exist (no separate Torrent struct to remove)
    - `TorrentSummary` is actively used in `api/types.rs` (line 159)
    - `FileStats` doesn't exist (already removed or never existed)
  - No action needed - types are either already removed or actively used

### Performance Simplifications

- [x] **SIMPLIFY-005**: Simplify Metrics Collection
  - ✅ **COMPLETED**: Already using simple `AtomicU64` counters in `src/metrics.rs`
  - No sharded counters exist in the codebase
  - Metrics are working efficiently with atomic operations

- [ ] **SIMPLIFY-006**: Consolidate Test Helpers
  - Create `tests/common/test_helpers.rs` module
  - Extract shared mock setup code from integration tests
  - Create helper functions for:
    - Mock server setup
    - Test torrent creation
    - File handle allocation patterns
  - Reduce duplication across test files

### API Simplifications

- [ ] **SIMPLIFY-007**: Simplify AsyncFuseWorker
  - Review if channel-based approach can be simplified
  - Consider if `tokio::sync::mpsc` can be replaced with `std::sync::mpsc`
  - Document the async/sync bridge pattern more clearly
  - Add sequence diagram to architecture docs

- [ ] **SIMPLIFY-008**: Consolidate Type Definitions
  - Merge duplicate torrent representations:
    - `types/torrent.rs::Torrent`
    - `api/types.rs::TorrentInfo`
    - `api/types.rs::TorrentDetails`
  - Choose canonical representation for each entity
  - Use `From`/`Into` traits for conversions
  - Remove redundant fields

### Documentation Simplifications

- [ ] **SIMPLIFY-009**: Consolidate Documentation
  - Remove redundant architecture diagrams (keep one in lib.rs)
  - Move implementation details from README to code docs
  - Create single source of truth for configuration options
  - Update outdated references in comments

### Testing Simplifications

- [ ] **SIMPLIFY-010**: Simplify Test Structure
  - Consolidate test modules if too fragmented
  - Use parameterized tests where appropriate
  - Extract common test fixtures into shared module
  - Remove redundant test cases that don't add coverage

---

## Quick Reference for Simplifications

### Priority Order (Updated Feb 22, 2026)

**HIGH PRIORITY:**
1. **SIMPLIFY-002** (Split Large Files) - `filesystem.rs` is 3,116 lines (not 1,434 as documented)
2. **SIMPLIFY-001** (Consolidate errors) - 3 error types cause confusion and duplicate mappings

**MEDIUM PRIORITY:**
3. **SIMPLIFY-006** (Test helpers) - Reduce test duplication
4. **Phase 4-6 Edge Cases** - Cache, streaming, and inode tests

**LOW PRIORITY:**
5. **SIMPLIFY-003** (Remove JSON config) - Breaking change with minimal benefit
6. **Phase 7+ Edge Cases** - Path resolution, error handling, concurrency, resources, unicode, config

**COMPLETED:**
- ✅ SIMPLIFY-004 (Remove dead code)
- ✅ SIMPLIFY-005 (Simplify metrics)

### Before Simplifying

1. Run full test suite: `cargo test`
2. Run clippy: `cargo clippy -- -D warnings`
3. Check for dead code: `cargo deadlinks` (if available)
4. Review usage with: `cargo check --all-features`

### After Simplifying

1. Verify all tests pass
2. Update documentation
3. Check for API breakage
4. Update CHANGELOG if applicable

### Tools to Help

```bash
# Find unused code
cargo deadlinks

# Check for complexity
cargo clippy -- -W clippy::complexity

# Find duplicate code
cargo bloat --crates

# Check module sizes
cargo modules structure
```

---

*Simplification tasks added - February 22, 2026*
