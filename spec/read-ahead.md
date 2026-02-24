# Read-Ahead/Prefetching Specification

## Overview

**Note: Prefetching has been removed from the current implementation.**

The read-ahead/prefetching feature was previously implemented to optimize sequential file reads by detecting access patterns and prefetching data ahead of time. However, this feature has been removed to simplify the codebase and reduce complexity.

## Current Implementation

The filesystem now uses a simplified approach:

1. **Direct Streaming**: Files are read directly via HTTP streaming without prefetching
2. **Persistent Connections**: HTTP connections are reused for sequential reads via `PersistentStreamManager`
3. **No Read-Ahead Buffer**: The `readahead_size` configuration option remains but only affects rqbit's internal behavior
4. **Simplified FileHandleManager**: Tracks only basic file handle information without sequential access tracking

### FileHandleManager (src/types/handle.rs)

```rust
/// Manager for file handles.
/// Allocates unique file handles and tracks open file state.
pub struct FileHandleManager {
    /// Counter for generating unique handle IDs
    next_handle: AtomicU64,
    /// Map of handle IDs to handle information
    handles: Arc<Mutex<HashMap<u64, FileHandle>>>,
    /// Maximum number of file handles allowed (0 = unlimited)
    max_handles: usize,
}

impl FileHandleManager {
    /// Create a new file handle manager with unlimited handles
    pub fn new() -> Self;
    
    /// Create a new file handle manager with a maximum handle limit
    pub fn with_max_handles(max_handles: usize) -> Self;
    
    /// Allocate a new file handle for an open file
    pub fn allocate(&self, inode: u64, torrent_id: u64, flags: i32) -> u64;
    
    /// Get file handle information by handle ID
    pub fn get(&self, fh: u64) -> Option<FileHandle>;
    
    /// Remove a file handle (called on release)
    pub fn remove(&self, fh: u64) -> Option<FileHandle>;
}
```

## Rationale for Removal

The prefetching feature was removed because:

1. **Complexity**: The implementation required significant complexity for pattern detection, cache coordination, and prefetch management
2. **Limited Benefit**: Modern media players and applications already implement their own buffering strategies
3. **rqbit's Read-Ahead**: The underlying rqbit torrent client already implements 32MB read-ahead, making additional prefetching redundant
4. **Maintenance Burden**: The feature required ongoing maintenance for race conditions, cache coherence, and memory management
5. **Simplicity**: Removing prefetching allows the filesystem to focus on reliable, direct streaming

## Future Considerations

If prefetching is re-implemented in the future, it should:

1. Be opt-in via configuration
2. Use a simpler heuristic-based approach
3. Integrate tightly with the cache layer
4. Include comprehensive metrics for monitoring efficiency

## References

- File handle implementation: `src/types/handle.rs`
- Streaming implementation: `src/api/streaming.rs`
- Configuration: `src/config/mod.rs`
- rqbit read-ahead: Uses rqbit's internal 32MB buffer

---

*Document version: 2.0 (simplified)*
*Last updated: 2026-02-23*
*Note: Prefetching infrastructure removed in SIMPLIFY-034*
