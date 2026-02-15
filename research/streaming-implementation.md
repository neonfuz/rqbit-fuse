# Streaming Implementation Details

## Problem
When rqbit ignores HTTP Range headers and returns the full file (200 OK instead of 206 Partial Content), the current implementation downloads the entire file into memory using `response.bytes().await?`. This causes:
- Massive memory usage (loading multi-GB files into RAM)
- Slow performance (2+ seconds for 4KB reads)
- Application hangs on large files

## Solution
Stream the response body and stop reading after reaching the requested byte limit.

## Code Changes

### File: `src/api/client.rs`

#### Step 1: Add Import
Add after line 11 (after existing imports):
```rust
use futures::stream::StreamExt;
```

#### Step 2: Replace Response Handling
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

## Key Points

1. **Three code paths**:
   - `200 OK` + range requested: Server ignored Range, stream with limit
   - `206 Partial Content`: Proper range response, read as-is
   - `200 OK` + no range: Full file requested, read entirely

2. **Efficient streaming**:
   - Pre-allocate buffer to exact size needed
   - Read chunks incrementally
   - Stop immediately when limit reached
   - No wasted memory or bandwidth

3. **Clear logging**:
   - Warning when server ignores Range (helps debugging)
   - Trace for normal operations
   - Shows bytes read vs requested

## Testing

After implementation:
```bash
cargo build --release && umount dl2 && ./target/release/rqbit-fuse mount -m ./dl2
time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```

**Expected**: Completes in <1 second with warning in logs

## References

- rqbit streaming API spec: `rqbit/spec/streaming-api.md`
- Original issue: Server returns 5.7GB for 4KB range request
