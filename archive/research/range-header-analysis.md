# Range Header Analysis

## HTTP Range Header Format

### Standard Format (RFC 7233)
```
Range: bytes=start-end
```

Where:
- `start`: Byte position to start reading (0-indexed, inclusive)
- `end`: Byte position to end reading (inclusive)
- Both values are REQUIRED for the format we're using

### Examples
```
Range: bytes=0-99      # First 100 bytes
Range: bytes=1000-1999 # Bytes 1000-1999 (1000 bytes total)
Range: bytes=0-4095    # First 4096 bytes
```

### Open-Ended Ranges (NOT RECOMMENDED)
```
Range: bytes=1000-     # From byte 1000 to end of file
```

Some servers don't handle open-ended ranges well. Always specify both start and end.

## Current Implementation

File: `src/api/client.rs` lines 505-512

```rust
let range_header = format!("bytes={}-{}", start, end);
request = request.header("Range", range_header);
```

This is correct per RFC 7233.

## rqbit Behavior

From testing:
```bash
curl -v -H "Range: bytes=0-4095" http://localhost:3030/torrents/1/stream/0
```

Response:
```
< HTTP/1.1 200 OK
< accept-ranges: bytes
< content-type: application/octet-stream
< transfer-encoding: chunked
```

**Problem**: rqbit returns `200 OK` instead of `206 Partial Content`
**Expected**: `206 Partial Content` with `Content-Range: bytes 0-4095/5702520832`

## Root Cause

The rqbit streaming API spec says it supports Range headers, but the actual implementation appears to ignore them and return the full file. This could be:

1. **Bug in rqbit**: Range support not fully implemented
2. **Version mismatch**: Older rqbit version without range support
3. **Configuration**: Range support disabled by default
4. **Torrent state**: Ranges only work when torrent is complete/paused

## Workaround

Since rqbit is returning the full file, we must:
1. Detect when we get `200 OK` for a range request
2. Stream the response instead of loading it all
3. Stop reading after reaching the requested byte count

See `research/streaming-implementation.md` for the implementation.

## Future Investigation

To determine if this is a rqbit bug or expected behavior:

1. Check rqbit version: `curl http://localhost:3030/version` (if endpoint exists)
2. Test with completed torrent vs downloading torrent
3. Test with paused torrent
4. Check rqbit GitHub issues for Range header support
5. Look at rqbit source: `crates/librqbit/src/http_api/handlers/streaming.rs`

## References

- RFC 7233: HTTP Range Requests
- rqbit streaming spec: `rqbit/spec/streaming-api.md`
- rqbit source: `rqbit/crates/librqbit/src/http_api/handlers/streaming.rs`
