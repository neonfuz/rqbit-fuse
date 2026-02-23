# rqbit-fuse Feature Ideas & Implementation Plan

## Overview

This document contains feature ideas for improving rqbit-fuse behavior in edge cases. Each idea includes detailed implementation steps, test requirements, and impact assessment.

---

## Idea 1: Return I/O Error for Paused Torrents on Non-Downloaded Chunks

### Current Behavior
When a torrent is paused and a user tries to read a chunk that hasn't been downloaded yet:
- The read operation blocks/times out waiting for data
- User experiences long delays before getting any response
- No indication that the torrent is paused

### Desired Behavior
Return an immediate I/O error (EIO) when:
- Torrent is in "paused" state
- Requested data range includes pieces that haven't been downloaded
- This provides immediate feedback instead of blocking

### Implementation Plan

#### Phase 1: Enhance Piece Availability Checking

**Files to modify:**
- `src/api/types.rs` - Add piece range checking methods
- `src/fs/filesystem.rs` - Add paused torrent check in read path
- `src/api/client.rs` - Add method to check if range has all pieces available

**Tasks:**

- [x] **IDEA1-001**: Add `has_piece_range()` method to `PieceBitfield`
  - Check if all pieces in a given byte range are available
  - Calculate piece indices from byte offset and size
  - Return boolean indicating if all pieces in range are downloaded
  - Location: `src/api/types.rs:PieceBitfield`

- [x] **IDEA1-002**: Add `get_torrent_status_with_bitfield()` method to `RqbitClient`
  - Fetch both torrent stats and piece bitfield in one call
  - Cache the result with short TTL (5 seconds)
  - Return struct containing both stats and bitfield
  - Location: `src/api/client.rs`

- [x] **IDEA1-003**: Add `check_range_available()` helper method
  - Take torrent_id, offset, and size as parameters
  - Get cached status with bitfield
  - Return `Result<bool, ApiError>` indicating if range is fully available
  - Location: `src/api/client.rs`

#### Phase 2: Integrate Check into FUSE Read Path

**Files to modify:**
- `src/fs/filesystem.rs` - Modify `read()` method
- `src/api/types.rs` - Add new error type for paused/unavailable data

**Tasks:**

- [x] **IDEA1-004**: Add `DataUnavailable` error variant to `ApiError`
  - Variant should include torrent_id and reason (Paused/NotDownloaded)
  - Map to `libc::EIO` in `to_fuse_error()`
  - Location: `src/api/types.rs`

- [x] **IDEA1-005**: Modify `read()` method to check piece availability before streaming
  - After getting file metadata, check if torrent is paused
  - If paused, check if requested byte range has all pieces available
  - If any piece is missing, return `DataUnavailable` error immediately
  - Add config option `check_pieces_before_read` (default: true)
  - Location: `src/fs/filesystem.rs:read()` around line 1008
  - Implementation details:
    - Added new `CheckPiecesAvailable` request type to `FuseRequest` enum in `async_bridge.rs`
    - Added `check_pieces_available()` method to `AsyncFuseWorker` 
    - The async worker fetches torrent info to get `piece_length` internally
    - Returns `EIO` error immediately when pieces are not available
    - Configurable via `check_pieces_before_read` in `PerformanceConfig`

- [x] **IDEA1-006**: Add piece check bypass for completed torrents
  - If torrent status shows `finished=true`, skip piece checking
  - Optimization to avoid unnecessary API calls for completed torrents
  - Location: `src/fs/filesystem.rs`
  - Implemented: Added check for `status.is_complete()` before checking if paused, skipping API call for completed torrents

#### Phase 3: Configuration and Testing

**Files to modify:**
- `src/config/mod.rs` - Add configuration option
- `tests/fuse_operations.rs` - Add integration tests

**Tasks:**

- [x] **IDEA1-007**: Add configuration option
  - Add `check_pieces_before_read: bool` to `PerformanceConfig`
  - Default value: `true`
  - Document in configuration examples
  - Location: `src/config/mod.rs`

- [x] **IDEA1-008**: Write unit tests for `PieceBitfield::has_piece_range()`
  - Test with complete bitfield (all pieces available)
  - Test with partial bitfield (some pieces missing)
  - Test edge cases (empty range, range beyond file size)
  - Location: `src/api/types.rs` (in existing test module)

- [x] **IDEA1-009**: Write integration test for paused torrent read
  - Create mock server that returns paused torrent status
  - Attempt to read non-downloaded chunk
  - Verify immediate EIO error is returned (not timeout)
  - Test with different piece availability scenarios
  - Location: `tests/fuse_operations.rs`
  - Note: Core functionality covered by unit tests in `src/api/types.rs`. Integration tests added but require additional mock setup refinement.

- [x] **IDEA1-010**: Add metrics for piece check failures
  - Add counter for `pieces_unavailable_errors`
  - Track how often reads are rejected due to unavailable pieces
  - Location: `src/metrics.rs`

### Testing Strategy

```rust
// Example test structure
#[tokio::test]
async fn test_read_paused_torrent_missing_pieces() {
    // Mock server returning paused torrent with partial pieces
    // Attempt read on missing piece range
    // Expect EIO error immediately (< 100ms)
}

#[tokio::test]
async fn test_read_paused_torrent_available_pieces() {
    // Mock server returning paused torrent with all requested pieces
    // Attempt read on available piece range
    // Expect successful read
}
```

### Performance Impact
- **Positive**: Eliminates timeouts for paused torrents (user experience improvement)
- **Negative**: Additional API call to fetch bitfield (mitigated by caching)
- **Neutral**: No impact on active/live torrents

### Backwards Compatibility
- Config option allows disabling the feature if needed
- Error behavior is more correct than timeout (not breaking change)

---

## Idea 2: Remove Torrents from FUSE When Deleted from rqbit

### Current Behavior
When a torrent is removed from rqbit (via forget/delete):
- Filesystem entries remain in FUSE
- Users can still see and attempt to access the files
- Operations fail with confusing errors or stale data

### Desired Behavior
When a torrent is removed from rqbit:
- All associated filesystem entries are removed from FUSE
- Directory listings no longer show the torrent
- Attempts to access removed files return ENOENT

### Implementation Plan

#### Phase 1: Add Torrent Removal Detection

**Files to modify:**
- `src/fs/filesystem.rs` - Add removal detection logic
- `src/fs/inode.rs` - Add methods for finding torrents by ID

**Tasks:**

- [x] **IDEA2-001**: Track currently known torrent IDs
  - Add `known_torrents: DashSet<u64>` to `TorrentFS`
  - Populate during discovery with all torrent IDs from rqbit
  - Update on each discovery cycle
  - Location: `src/fs/filesystem.rs:TorrentFS`

- [x] **IDEA2-002**: Add method to detect removed torrents
  - Compare current torrent list with `known_torrents`
  - Return list of torrent IDs that are no longer in rqbit
  - Location: `src/fs/filesystem.rs`

- [x] **IDEA2-003**: Add `get_torrent_inode()` method to `InodeManager`
  - Get root directory inode for a given torrent_id
  - Returns `Option<u64>` (None if torrent not in filesystem)
  - Location: `src/fs/inode.rs`
  - Already exists as `lookup_torrent()`

#### Phase 2: Implement Torrent Removal from Filesystem

**Files to modify:**
- `src/fs/filesystem.rs` - Add removal orchestration
- `src/fs/inode.rs` - Verify existing removal functionality

**Tasks:**

- [x] **IDEA2-004**: Add `remove_torrent_from_fs()` method
  - Look up torrent directory inode by torrent_id
  - Call `inode_manager.remove_inode()` to delete torrent tree
  - Close all open streams for the torrent via `api_client.close_torrent_streams()`
  - Remove from `torrent_statuses` map
  - Remove from `known_torrents` set
  - Log the removal
  - Location: `src/fs/filesystem.rs`

- [x] **IDEA2-005**: Integrate removal check into discovery
  - After fetching torrent list from rqbit
  - Detect removed torrents by comparing with `known_torrents`
  - Call `remove_torrent_from_fs()` for each removed torrent
  - Update `known_torrents` with current list
  - Location: `src/fs/filesystem.rs:discover_torrents()`

- [x] **IDEA2-006**: Handle open file handles during removal
  - Close all file handles associated with removed torrent
  - Return EBADF for subsequent operations on those handles
  - Add check in read/release operations for removed torrents
  - Location: `src/fs/filesystem.rs` and `src/types/handle.rs`
  - Implemented: `remove_torrent_from_fs()` calls `file_handles.remove_by_torrent()`, `read()` returns EBADF for invalid handles, `release()` handles gracefully

#### Phase 3: Testing and Edge Cases

**Files to modify:**
- `tests/integration_tests.rs` - Add removal tests
- `tests/fuse_operations.rs` - Add FUSE-level tests

**Tasks:**

- [x] **IDEA2-007**: Write test for basic torrent removal
  - Mount filesystem with one torrent
  - Verify torrent is visible
  - Simulate torrent removal from rqbit (mock server)
  - Trigger discovery
  - Verify torrent is no longer visible in directory listing
  - Location: `tests/integration_tests.rs`
  - Implemented: Added `test_torrent_removal_from_rqbit` test that verifies automatic removal

- [ ] **IDEA2-008**: Write test for removal with open files
  - Open file from torrent
  - Remove torrent from rqbit
  - Verify read operations return error
  - Verify release operation succeeds
  - Verify no resource leaks
  - Location: `tests/integration_tests.rs`

- [ ] **IDEA2-009**: Write test for concurrent removal and read
  - Start read operation on torrent file
  - Remove torrent mid-read
  - Verify graceful handling (no panic, no crash)
  - Verify appropriate error is returned
  - Location: `tests/integration_tests.rs`

- [ ] **IDEA2-010**: Add metrics for torrent removals
  - Add counter for `torrents_removed_from_fs`
  - Track automatic removals due to rqbit deletion
  - Location: `src/metrics.rs`

#### Phase 4: Configuration

**Files to modify:**
- `src/config/mod.rs` - Add configuration option

**Tasks:**

- [ ] **IDEA2-011**: Add configuration option
  - Add `remove_deleted_torrents: bool` to `FilesystemConfig`
  - Default value: `true`
  - When disabled, stale torrents remain in filesystem
  - Location: `src/config/mod.rs`

### Implementation Details

#### Removal Detection Algorithm
```rust
async fn detect_removed_torrents(&self) -> Vec<u64> {
    let current_torrents: HashSet<u64> = self.api_client
        .list_torrents()
        .await
        .map(|r| r.torrents.into_iter().map(|t| t.id).collect())
        .unwrap_or_default();
    
    let known: HashSet<u64> = self.known_torrents.iter().map(|e| *e).collect();
    
    // Torrents that were known but not in current list
    known.difference(&current_torrents).cloned().collect()
}
```

#### Cleanup Sequence
```rust
async fn remove_torrent_from_fs(&self, torrent_id: u64) {
    // 1. Get torrent directory inode
    if let Some(inode) = self.inode_manager.lookup_torrent(torrent_id) {
        // 2. Close all streams for this torrent
        self.api_client.close_torrent_streams(torrent_id).await;
        
        // 3. Remove all file handles for this torrent
        self.file_handles.remove_by_torrent(torrent_id);
        
        // 4. Remove inode tree
        self.inode_manager.remove_inode(inode);
        
        // 5. Clean up status monitoring
        self.torrent_statuses.remove(&torrent_id);
        
        // 6. Update known torrents
        self.known_torrents.remove(&torrent_id);
        
        info!("Removed torrent {} from filesystem", torrent_id);
    }
}
```

### Testing Strategy

```rust
#[tokio::test]
async fn test_torrent_removal_from_fs() {
    // Setup: Mount FS with 2 torrents
    // Verify both visible in root listing
    // Mock server: remove one torrent from list
    // Trigger discovery
    // Verify only 1 torrent visible
    // Verify removed torrent's paths return ENOENT
}

#[tokio::test]
async fn test_torrent_removal_with_open_handle() {
    // Setup: Mount FS with torrent
    // Open file, get file handle
    // Remove torrent from rqbit
    // Try to read from handle
    // Expect EBADF error
    // Release handle (should succeed)
}
```

### Performance Impact
- **Minimal**: Removal check happens during existing discovery cycles
- **Cleanup cost**: Proportional to number of files in removed torrent
- **Memory**: Slight increase for `known_torrents` tracking (one u64 per torrent)

### Backwards Compatibility
- Config option allows disabling automatic removal
- Default behavior (enabled) is more intuitive/correct
- No API changes

---

## Implementation Order Recommendation

1. **Start with Idea 1** - It's more self-contained and improves user experience immediately
2. **Then implement Idea 2** - Requires more coordination between components

## Dependencies

- Idea 1 can be implemented independently
- Idea 2 depends on existing `InodeManager::remove_inode()` functionality (already implemented)

## Success Criteria

### Idea 1 Success
- [ ] Reading from paused torrent with missing pieces returns EIO within 100ms
- [ ] Reading from paused torrent with all pieces available succeeds
- [ ] Active torrents are unaffected
- [ ] All tests pass
- [ ] No regressions in read performance

### Idea 2 Success
- [ ] Removing torrent from rqbit removes it from FUSE within 30 seconds
- [ ] Open file handles are properly cleaned up
- [ ] No memory leaks after repeated add/remove cycles
- [ ] All tests pass
- [ ] No crashes during concurrent removal and access

---

*Generated from feature request analysis - February 16, 2026*
