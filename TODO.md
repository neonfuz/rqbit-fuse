# rqbit-fuse Edge Case Testing Checklist

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

- [ ] **EDGE-014**: Test empty directory listing
  - Create empty directory, list contents
  - Should return "." and ".." only
  - Verify no crash or error

- [ ] **EDGE-015**: Test directory with many files
  - Create directory with 1000+ files
  - List contents with various offsets
  - Verify pagination/offset works correctly

---

## Phase 4: Cache Edge Cases

### Cache Operations (src/cache.rs)

- [ ] **EDGE-016**: Test cache entry expiration during access
  - Insert entry with short TTL
  - Start get() operation
  - Let entry expire during operation
  - Should return None, not panic

- [ ] **EDGE-017**: Test cache at exact capacity
  - Fill cache to exactly max_entries
  - Insert one more entry (should trigger eviction)
  - Verify eviction count increments
  - Verify oldest entry is evicted

- [ ] **EDGE-018**: Test rapid insert/remove cycles
  - Insert and remove same key 1000 times rapidly
  - Should maintain consistency
  - No memory leaks

- [ ] **EDGE-019**: Test concurrent insert of same key
  - 10 threads try to insert same key simultaneously
  - One should succeed, others should handle gracefully
  - Cache should have exactly one entry

- [ ] **EDGE-020**: Test cache statistics edge cases
  - Hit rate with 0 total requests
  - Hit rate with 0 hits, many misses
  - Hit rate with 0 misses, many hits
  - Should not divide by zero

---

## Phase 5: Streaming Edge Cases

### HTTP Streaming (src/api/streaming.rs)

- [ ] **EDGE-021**: Test server returning 200 instead of 206
  - Request range, server returns full file (200 OK)
  - Should handle correctly, skip to offset
  - Verify data correctness

- [ ] **EDGE-022**: Test empty response body
  - Server returns 200/206 with empty body
  - Should handle gracefully, return empty bytes
  - No panic or infinite loop

- [ ] **EDGE-023**: Test network disconnect during read
  - Start reading stream
  - Simulate network failure mid-read
  - Should return error, clean up properly
  - No resource leaks

- [ ] **EDGE-024**: Test slow server response
  - Server sends data very slowly
  - Should respect timeout
  - Should not block indefinitely

- [ ] **EDGE-025**: Test wrong content-length
  - Server returns more/less data than Content-Length header
  - Should handle gracefully
  - Return error or available data

- [ ] **EDGE-026**: Test seek patterns
  - Seek backward by 1 byte (should create new stream)
  - Seek forward exactly MAX_SEEK_FORWARD bytes
  - Rapid alternating forward/backward seeks
  - Verify stream creation/reuse logic

---

## Phase 6: Inode Management Edge Cases

### Inode Allocation (src/fs/inode.rs)

- [ ] **EDGE-027**: Test inode 0 allocation attempt
  - Try to allocate inode 0
  - Should fail gracefully, return 0 or error
  - Should not corrupt inode counter

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

- [ ] **EDGE-044**: Test stream limit exhaustion
  - Open 50 files (max streams)
  - Try to open 51st file
  - Should return error
  - Error should be clear

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

## Quick Reference

### Test Categories

1. **Boundary Tests**: EOF, offsets, capacity limits
2. **Resource Tests**: Handle limits, stream limits, inode limits
3. **Error Tests**: Network failures, invalid inputs, edge responses
4. **Concurrency Tests**: Race conditions, simultaneous operations
5. **Unicode Tests**: Special characters, normalization, encoding
6. **Configuration Tests**: Invalid configs, edge values

### Running Tests

```bash
# Run all edge case tests
cargo test edge_

# Run specific category
cargo test edge_001
cargo test edge_006  # File handle tests
cargo test edge_021  # Streaming tests

# Run with output
cargo test edge_ -- --nocapture
```

### Completion Criteria

Each test should:
- Be isolated (no dependencies on other tests)
- Run quickly (< 1 second per test)
- Cover both success and failure paths
- Include assertions for all outcomes
- Pass `cargo test`
- Pass `cargo clippy`
- Be formatted with `cargo fmt`
- Have checkbox marked as complete

---

*Generated from edge case analysis - February 16, 2026*
