# Inode Design Research

**Date:** February 14, 2026  
**Task:** INODE-002 - Make inode table operations atomic

## Summary

This research documents the implementation of atomic inode table operations in the rqbit-fuse filesystem. The work focused on ensuring consistency across the three DashMap structures (`entries`, `path_to_inode`, `torrent_to_inode`) during concurrent access.

## Problem Statement

The original `InodeManager` updated three separate DashMap structures independently:

1. `entries: DashMap<u64, InodeEntry>` - Primary storage
2. `path_to_inode: DashMap<String, u64>` - Path-to-inode index
3. `torrent_to_inode: DashMap<u64, u64>` - Torrent-to-inode index

This created potential for inconsistent state if:
- A panic occurred between map updates
- Concurrent operations raced on the same data
- A crash left partial updates

## Solution Implemented

### Atomic Allocation

Refactored `allocate_entry()` to use DashMap's entry API:

```rust
match self.entries.entry(inode) {
    dashmap::mapref::entry::Entry::Vacant(e) => {
        e.insert(entry);
    }
    dashmap::mapref::entry::Entry::Occupied(_) => {
        panic!("Inode {} already exists (counter corrupted)", inode);
    }
}
```

Key improvements:
1. **Primary-first ordering**: Insert into `entries` before updating indices
2. **Entry API**: Uses atomic check-and-insert via DashMap's entry API
3. **Panic protection**: Detects corrupted inode counter (duplicate inodes)
4. **Recoverable indices**: If index updates fail, entry still exists and can be rebuilt

### Atomic Removal

Rewrote `remove_inode()` with consistent 4-step order:

1. **Recursively remove children** (bottom-up traversal)
2. **Remove from parent's children list** (maintain hierarchy consistency)
3. **Remove from indices** using stored path (cleanup lookups)
4. **Remove from primary entries** (authoritative removal)

This ensures no dangling references remain after removal.

### Clear Torrents

Updated `clear_torrents()` to use atomic `remove_inode()` for each entry rather than direct map clearing, ensuring proper cleanup of all references.

## Testing

Added 4 comprehensive concurrent tests:

### test_concurrent_allocation_atomicity
- 50 threads × 20 allocations each (1000 total)
- Immediate verification after each allocation
- Verifies: allocated inode exists, unique inodes, correct count

### test_concurrent_removal_atomicity
- 20 torrents with 5 files each (120 total inodes)
- 4 threads removing 5 torrents each concurrently
- Verifies: proper cleanup, no orphans, torrent mappings cleared

### test_mixed_concurrent_operations
- 10 allocator threads + 5 remover threads
- Tests interleaved operations
- Verifies: no orphans, parent consistency, count invariants

### test_atomic_allocation_no_duplicates
- 100 threads allocating simultaneously
- Records all allocated inodes in synchronized HashSet
- Verifies: no duplicate inodes under extreme contention

## Results

- All 20 inode tests pass
- No clippy warnings
- Code formatted
- No behavioral changes to existing functionality

## Files Modified

- `src/fs/inode.rs` - Core atomic operation implementation
- `TODO.md` - Marked INODE-002 as complete
- `CHANGELOG.md` - Added entry for the fix
- `.git/COMMIT_EDITMSG` - Commit message

## Related

- Specification: `spec/inode-design.md`
- Next tasks: INODE-003 (Fix torrent directory mapping), INODE-004 (Make entries private)
