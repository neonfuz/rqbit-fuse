# Fix: Handle rqbit ignoring Range headers

## Problem
The rqbit API is ignoring HTTP Range headers and returning the entire file (5.7GB) instead of the requested range (4KB). The current code downloads the entire response using `response.bytes().await?`, causing:
- Massive memory usage (loading 5.7GB into RAM)
- Slow read performance (2+ seconds per 4KB read)
- File copy operations failing or timing out

## Root Cause
In `src/api/client.rs`, the `read_file` function calls `response.bytes().await?` which downloads the complete response body. When rqbit ignores the Range header and returns 200 OK with the full file, this loads the entire file into memory.

## Streaming API Spec Analysis

According to the rqbit streaming API spec:
- **Expected**: Range requests should return `206 Partial Content` with `Content-Range` header
- **Current behavior**: Returns `200 OK` with full file (5.7GB) despite `Range: bytes=0-4095` header
- **Spec allows**: `200 OK` for full content, `206 Partial Content` for ranges

The fix must handle both cases defensively.

## Solution
Modify `read_file` to:
1. Detect when we get 200 OK instead of 206 Partial Content for range requests
2. Stream the response body instead of loading it all at once
3. Stop reading after reaching the requested byte limit
4. Use `response.bytes_stream()` to read chunks incrementally

## Changes Required

### File: `src/api/client.rs`

Add import for `StreamExt` at the top of the file (after existing imports):
```rust
use futures::stream::StreamExt;
```

Note: `futures = "0.3"` is already in Cargo.toml, so this import should work.

Replace the `_ =>` match arm in `read_file` function (lines 538-551) with:

```rust
_ => {
    let response = self.check_response(response).await?;
    let status = response.status();
    
    // Handle range response properly
    // Per spec: 206 Partial Content = range honored, 200 OK = full file
    let is_range_response = status == StatusCode::PARTIAL_CONTENT;
    let is_full_file = status == StatusCode::OK && range.is_some();
    
    if is_full_file {
        // Server ignored Range header - stream with limit to avoid downloading entire file
        let (start, end) = range.unwrap();
        let requested_size = end - start + 1;
        warn!(
            api_op = "read_file",
            torrent_id = torrent_id,
            file_idx = file_idx,
            requested_bytes = requested_size,
            "Server returned 200 OK instead of 206 Partial Content, streaming with byte limit"
        );
        
        // Stream and limit to requested bytes
        let mut stream = response.bytes_stream();
        let mut result = Vec::with_capacity(requested_size as usize);
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let remaining = requested_size as usize - result.len();
            
            if chunk.len() >= remaining {
                // We've got enough bytes
                result.extend_from_slice(&chunk[..remaining]);
                break;
            } else {
                result.extend_from_slice(&chunk);
            }
        }
        
        let bytes = Bytes::from(result);
        
        trace!(
            api_op = "read_file",
            torrent_id = torrent_id,
            file_idx = file_idx,
            bytes_read = bytes.len(),
            requested_bytes = requested_size,
            "Range request completed with byte limiting"
        );
        
        Ok(bytes)
    } else if is_range_response {
        // Server properly returned partial content - read exactly what was returned
        // The response should contain exactly the requested range
        let bytes = response.bytes().await?;
        
        trace!(
            api_op = "read_file",
            torrent_id = torrent_id,
            file_idx = file_idx,
            bytes_read = bytes.len(),
            "Range request completed with 206 Partial Content"
        );
        
        Ok(bytes)
    } else {
        // Full file request (no range) - read entire response
        let bytes = response.bytes().await?;
        
        trace!(
            api_op = "read_file",
            torrent_id = torrent_id,
            file_idx = file_idx,
            bytes_read = bytes.len(),
            "Full file read completed"
        );
        
        Ok(bytes)
    }
}
```

## Key Improvements

1. **Three distinct code paths**:
   - `206 Partial Content`: Proper range response, read as-is
   - `200 OK` with range requested: Server ignored range, stream with limit
   - `200 OK` no range: Full file requested, read entirely

2. **Efficient streaming for broken case**:
   - Pre-allocate buffer to requested size
   - Read chunks until limit reached
   - Stop immediately when we have enough bytes

3. **Clear logging**:
   - Warning when server ignores Range header
   - Trace for normal operations
   - Shows bytes read vs requested

## Verification

After applying this fix:
- API reads should complete in milliseconds instead of seconds
- Memory usage should remain low (only requested bytes loaded)
- File copy operations should work correctly
- Log should show warning when server ignores Range header

## Test

Test with:
```bash
# Copy a small portion of the file
cat dl2/ubuntu-25.10-desktop-amd64.iso > a
# Should complete quickly instead of downloading entire 5.7GB
```

## Future Investigation

The spec says rqbit should support Range headers. The fact that it's returning `200 OK` instead of `206 Partial Content` suggests either:
1. The rqbit version being used doesn't have range support implemented
2. There's a bug in how the Range header is being sent
3. The torrent state affects range support

Once this fix is applied, investigate further by:
- Checking rqbit version and comparing to streaming spec implementation date
- Verifying the exact Range header format being sent
- Testing with different torrent states (live vs paused)
