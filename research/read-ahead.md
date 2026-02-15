# Read-Ahead Strategies Research - PERF-001

## Current State

Looking at the codebase:
1. In `streaming.rs`, there's a `PersistentStreamManager` that manages HTTP connections
2. The `readahead_size` config option exists (default: 32MB)
3. Current behavior: Data is fetched via HTTP Range requests, but there's no explicit read-ahead implementation

## Current Prefetch Behavior

Looking at the code:
- HTTP Range requests are used for fetching data
- `readahead_size` is configured but not actively used for prefetching
- The comment mentions "prefetching" but implementation is minimal

## Implementation Approaches

### Approach 1: Simple Sequential Access Detection

Track if reads are sequential (offset progresses monotonically). If so, issue prefetch requests ahead of the current read position.

```rust
// Pseudocode
struct ReadState {
    last_offset: u64,
    is_sequential: bool,
}

fn on_read(inode, offset) {
    if offset == state.last_offset + state.last_size {
        state.is_sequential = true;
        // Issue prefetch for next chunk
        prefetch_ahead(inode, offset + state.last_size, readahead_size);
    } else {
        state.is_sequential = false;
    }
    state.last_offset = offset;
}
```

### Approach 2: Sliding Window Prefetch

Maintain a sliding window of prefetched data:
- Keep N bytes ahead of current read position cached
- Use background tasks to prefetch
- Evict prefetched data that's no longer needed

### Approach 3: rqbit Integration

rqbit already has internal prefetching. The current implementation relies on rqbit's behavior when Range requests are made. We could:
1. Make larger Range requests to trigger rqbit's internal prefetch
2. Adjust the Range request size based on `readahead_size` config

## Recommendations

### Recommended: Approach 3 (rqbit Integration)

The simplest approach - leverage rqbit's existing prefetch:
1. When reading at offset, request `readahead_size` bytes instead of just what's asked
2. The extra data stays in rqbit's buffer for subsequent reads
3. Minimal code changes needed

### Alternative: Approach 1 (Simple Sequential Detection)

If we want explicit prefetch in rqbit-fuse:
1. Track sequential access patterns per file handle
2. Issue prefetch requests in background
3. More complex but gives more control

## Implementation Plan

### Simple Implementation (Recommended):
1. Modify `read_stream_range()` in streaming.rs
2. When making HTTP Range request, extend range to include `readahead_size` extra bytes
3. Don't return the extra data to caller, just let rqbit buffer it
4. Make this configurable (can be disabled for testing)

### Performance Considerations:
- Larger Range requests = more memory in rqbit
- Trade-off: Better prefetch vs more memory
- Consider: Make readahead_size configurable per-file-type (video gets more)

## Configuration

```toml
[performance]
# Current:
readahead_size = 33554432  # 32MB

# New (optional):
prefetch_enabled = true    # Enable prefetch
prefetch_multiplier = 2.0  # Prefetch N times the read size
```

## Risk Assessment

- Low risk: rqbit already handles this
- Testing: Need to verify rqbit buffers the extra data
- Fallback: If rqbit doesn't buffer, implement Approach 1

## References

- rqbit HTTP API: Range request handling
- fuser: How to implement read-ahead
