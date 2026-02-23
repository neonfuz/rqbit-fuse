# Config Fields Usage Analysis

**Date:** 2026-02-22
**Task:** Review config fields for unused/unimplemented features

## Fields Analyzed

### 1. `piece_check_enabled` (PerformanceConfig)

**Location**: `src/config/mod.rs:309`

**Usage Found**:
- **src/fs/filesystem.rs:888**: Used in `check_pieces_available()` method
- When `false`: Skips piece availability checks, assumes all pieces available
- When `true`: Performs actual piece availability checks via torrent status

**Implementation Status**: ✅ FULLY IMPLEMENTED

**Rationale**: This is a working configuration option that allows users to disable piece checking if needed (e.g., for completed torrents where all pieces are guaranteed available).

### 2. `prefetch_enabled` (PerformanceConfig)

**Location**: `src/config/mod.rs:311`

**Usage Found**:
- **src/fs/filesystem.rs:944**: Used in read path to trigger prefetch
- When `false` (default): Prefetch disabled - PersistentStream handles buffering
- When `true`: Calls `do_prefetch()` method to prefetch additional data

**Implementation Status**: ✅ FULLY IMPLEMENTED

**Rationale**: Although disabled by default (as documented in code comments), the prefetch functionality is fully implemented. It's disabled because PersistentStream already provides adequate buffering via:
1. HTTP Keep-Alive connections
2. Internal pending_buffer for chunk caching

The config option allows power users to enable prefetch if they have specific use cases.

## Other Config Fields

All config fields in `src/config/mod.rs` are:
- Properly defined with documentation
- Have Default implementations
- Have environment variable mappings
- Have validation logic
- Used in the codebase

## Conclusion

**No unused config fields found.**

All configuration options are legitimate and serve a purpose:
- `piece_check_enabled`: Controls piece availability verification
- `prefetch_enabled`: Controls read-ahead prefetching behavior

**No action needed** - all config fields are appropriately designed and utilized.

## References

- `src/config/mod.rs` - Config definitions
- `src/fs/filesystem.rs:888` - piece_check_enabled usage
- `src/fs/filesystem.rs:944` - prefetch_enabled usage
