# Migration Guide: SIMPLIFY-011 - Extract Streaming Helpers

**Status**: Ready for Implementation  
**Scope**: `src/api/streaming.rs`  
**Complexity**: Low  
**Estimated Time**: 30 minutes

---

## Overview

Extract duplicate buffer handling logic from `PersistentStream::read()` and `PersistentStream::skip()` methods. Create reusable helper functions to reduce code duplication and improve maintainability.

**Expected Impact**: Remove ~40 lines of duplicate code

---

## Current State

### Problem: Duplicated Buffer Handling (~55 lines)

The `read()` and `skip()` methods in `PersistentStream` contain nearly identical logic for:
1. Consuming data from `pending_buffer` (13 lines duplicated)
2. Reading from stream and buffering leftovers (26 lines duplicated)
3. Duplicate stream read pattern in manager (16 lines duplicated)

### Code to Extract

**1. Pending Buffer Handling (lines 136-149 in read(), 195-207 in skip()):**

```rust
// In read() - lines 136-149
if let Some(ref mut pending) = self.pending_buffer {
    let to_copy = pending.len().min(buf.len());
    buf[..to_copy].copy_from_slice(&pending[..to_copy]);
    bytes_read += to_copy;
    self.current_position += to_copy as u64;

    if to_copy < pending.len() {
        *pending = pending.slice(to_copy..);
    } else {
        self.pending_buffer = None;
    }
}

// In skip() - lines 195-207 (nearly identical)
if let Some(ref mut pending) = self.pending_buffer {
    let to_skip = pending.len().min(bytes_to_skip as usize);
    skipped += to_skip as u64;
    self.current_position += to_skip as u64;

    if to_skip < pending.len() {
        *pending = pending.slice(to_skip..);
    } else {
        self.pending_buffer = None;
    }
}
```

**2. Stream Reading with Leftover Buffering (lines 152-180 in read(), 210-232 in skip()):**

```rust
// In read() - lines 152-180
while bytes_read < buf.len() {
    match self.stream.next().await {
        Some(Ok(chunk)) => {
            let remaining = buf.len() - bytes_read;
            let to_copy = chunk.len().min(remaining);
            buf[bytes_read..bytes_read + to_copy].copy_from_slice(&chunk[..to_copy]);
            bytes_read += to_copy;
            self.current_position += to_copy as u64;

            if to_copy < chunk.len() {
                self.pending_buffer = Some(chunk.slice(to_copy..));
                trace!(bytes_buffered = chunk.len() - to_copy, "Buffered extra bytes");
                break;
            }
        }
        Some(Err(e)) => {
            self.is_valid = false;
            return Err(anyhow::anyhow!("Stream error: {}", e));
        }
        None => break,
    }
}

// In skip() - lines 210-232 (similar structure, discards instead of copies)
while skipped < bytes_to_skip {
    match self.stream.next().await {
        Some(Ok(chunk)) => {
            let remaining = bytes_to_skip - skipped;
            let to_skip = chunk.len().min(remaining as usize);
            skipped += to_skip as u64;
            self.current_position += to_skip as u64;

            if to_skip < chunk.len() {
                self.pending_buffer = Some(chunk.slice(to_skip..));
                break;
            }
        }
        Some(Err(e)) => {
            self.is_valid = false;
            return Err(anyhow::anyhow!("Stream error during skip: {}", e));
        }
        None => break,
    }
}
```

**3. Manager Read Pattern Duplication (lines 394-406 and 422-439):**

```rust
// Lines 394-406 - reading from existing stream
let mut buffer = vec![0u8; size];
let bytes_read = stream.read(&mut buffer).await?;
buffer.truncate(bytes_read);
trace!(stream_op = "read_complete", torrent_id, file_idx, bytes_read, "Completed read");
Ok(Bytes::from(buffer))

// Lines 422-439 - reading from new stream (nearly identical)
let mut buffer = vec![0u8; size];
let bytes_read = new_stream.read(&mut buffer).await?;
buffer.truncate(bytes_read);
let mut streams = self.streams.lock().await;
streams.insert(key, new_stream);
trace!(stream_op = "read_complete", torrent_id, file_idx, bytes_read, "Completed read");
Ok(Bytes::from(buffer))
```

---

## Target State

### New Helper Functions

**1. `consume_pending()` - Extract common pending buffer logic:**

```rust
/// Consume bytes from pending buffer, returns bytes consumed
fn consume_pending(&mut self, bytes_needed: usize) -> usize {
    if let Some(ref mut pending) = self.pending_buffer {
        let to_consume = pending.len().min(bytes_needed);
        self.current_position += to_consume as u64;
        
        if to_consume < pending.len() {
            *pending = pending.slice(to_consume..);
        } else {
            self.pending_buffer = None;
        }
        to_consume
    } else {
        0
    }
}
```

**2. `buffer_leftover()` - Extract leftover chunk buffering:**

```rust
/// Buffer remaining chunk data after consuming `consumed` bytes
fn buffer_leftover(&mut self, chunk: Bytes, consumed: usize) {
    if consumed < chunk.len() {
        self.pending_buffer = Some(chunk.slice(consumed..));
        trace!(bytes_buffered = chunk.len() - consumed, "Buffered extra bytes from chunk");
    }
}
```

**3. `read_from_stream()` in manager - Extract duplicate read pattern:**

```rust
/// Read data from a stream into a Bytes buffer
async fn read_from_stream(
    &self,
    stream: &mut PersistentStream,
    size: usize,
    torrent_id: u64,
    file_idx: usize,
) -> Result<Bytes> {
    let mut buffer = vec![0u8; size];
    let bytes_read = stream.read(&mut buffer).await?;
    buffer.truncate(bytes_read);
    
    trace!(
        stream_op = "read_complete",
        torrent_id = torrent_id,
        file_idx = file_idx,
        bytes_read = bytes_read,
        "Completed read from persistent stream"
    );
    
    Ok(Bytes::from(buffer))
}
```

### Refactored Methods

**Simplified `read()` method:**

```rust
async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    if !self.is_valid {
        return Err(anyhow::anyhow!("Stream is no longer valid"));
    }

    let mut bytes_read = 0;

    // First, use any pending buffered data
    let pending_consumed = self.consume_pending(buf.len());
    if pending_consumed > 0 {
        buf[..pending_consumed].copy_from_slice(
            &self.pending_buffer.as_ref().map(|p| p.slice(0..pending_consumed)).unwrap()
        );
        bytes_read += pending_consumed;
    }

    // Read more data from the stream if needed
    while bytes_read < buf.len() {
        match self.stream.next().await {
            Some(Ok(chunk)) => {
                let remaining = buf.len() - bytes_read;
                let to_copy = chunk.len().min(remaining);
                buf[bytes_read..bytes_read + to_copy].copy_from_slice(&chunk[..to_copy]);
                bytes_read += to_copy;
                self.current_position += to_copy as u64;

                self.buffer_leftover(chunk, to_copy);
                if self.pending_buffer.is_some() {
                    break;
                }
            }
            Some(Err(e)) => {
                self.is_valid = false;
                return Err(anyhow::anyhow!("Stream error: {}", e));
            }
            None => break,
        }
    }

    self.last_access = Instant::now();
    Ok(bytes_read)
}
```

**Simplified `skip()` method:**

```rust
async fn skip(&mut self, bytes_to_skip: u64) -> Result<u64> {
    if !self.is_valid {
        return Err(anyhow::anyhow!("Stream is no longer valid"));
    }

    let mut skipped = self.consume_pending(bytes_to_skip as usize) as u64;

    // Skip more data from the stream if needed
    while skipped < bytes_to_skip {
        match self.stream.next().await {
            Some(Ok(chunk)) => {
                let remaining = bytes_to_skip - skipped;
                let to_skip = chunk.len().min(remaining as usize);
                skipped += to_skip as u64;
                self.current_position += to_skip as u64;

                self.buffer_leftover(chunk, to_skip);
                if self.pending_buffer.is_some() {
                    break;
                }
            }
            Some(Err(e)) => {
                self.is_valid = false;
                return Err(anyhow::anyhow!("Stream error during skip: {}", e));
            }
            None => break,
        }
    }

    self.last_access = Instant::now();
    Ok(skipped)
}
```

**Simplified manager `read()` method:**

```rust
pub async fn read(&self, torrent_id: u64, file_idx: usize, offset: u64, size: usize) -> Result<Bytes> {
    let key = StreamKey { torrent_id, file_idx };

    // Check if we have a usable stream
    let can_use_existing = {
        let streams = self.streams.lock().await;
        streams.get(&key).map(|s| s.can_read_at(offset)).unwrap_or(false)
    };

    if can_use_existing {
        let mut streams = self.streams.lock().await;
        let stream = streams.get_mut(&key).unwrap();

        // If we need to seek forward a bit, do it
        if offset > stream.current_position {
            let gap = offset - stream.current_position;
            trace!(bytes_to_skip = gap, "Skipping forward in existing stream");
            stream.skip(gap).await?;
        }

        self.read_from_stream(stream, size, torrent_id, file_idx).await
    } else {
        let mut new_stream = PersistentStream::new(
            &self.client, &self.base_url, torrent_id, file_idx, offset
        ).await?;

        let result = self.read_from_stream(&mut new_stream, size, torrent_id, file_idx).await?;
        
        let mut streams = self.streams.lock().await;
        streams.insert(key, new_stream);
        
        Ok(result)
    }
}
```

---

## Implementation Steps

1. **Add `consume_pending()` helper to `PersistentStream`**
   - Add after line 46 (after struct definition)
   - Extract logic from lines 136-149 and 195-207
   - Test: Verify both read() and skip() still work

2. **Add `buffer_leftover()` helper to `PersistentStream`**
   - Add after `consume_pending()`
   - Extract logic for buffering remaining chunk data
   - Test: Verify partial chunk buffering works

3. **Refactor `read()` to use helpers**
   - Replace lines 136-149 with `consume_pending()` call
   - Replace lines 162-169 with `buffer_leftover()` call
   - Test: `cargo test streaming` (if tests exist)

4. **Refactor `skip()` to use helpers**
   - Replace lines 195-207 with `consume_pending()` call
   - Replace lines 219-221 with `buffer_leftover()` call
   - Test: Verify skip operations still accurate

5. **Add `read_from_stream()` to `PersistentStreamManager`**
   - Add after line 291 (after `Drop` impl)
   - Extract duplicate buffer allocation/read pattern
   - Test: Verify stream reads return correct data

6. **Refactor manager `read()` to use helper**
   - Replace lines 394-406 with `read_from_stream()` call
   - Replace lines 422-439 with `read_from_stream()` call
   - Test: Full read cycle works correctly

7. **Run verification**
   ```bash
   cargo test
   cargo clippy
   cargo fmt
   ```

---

## Testing

### Unit Tests (if existing)

```bash
# Run any existing streaming tests
cargo test streaming

# Run all tests to ensure no regressions
cargo test
```

### Manual Verification

1. **Test sequential reads:**
   - Mount filesystem
   - Read a file from start to finish
   - Verify data integrity

2. **Test seeking behavior:**
   - Seek forward within MAX_SEEK_FORWARD
   - Verify it uses skip() rather than creating new stream
   - Check position tracking is accurate

3. **Test pending buffer handling:**
   - Read partial chunks to trigger buffering
   - Read again to verify buffered data is used
   - Check no data loss or duplication

### Edge Cases to Verify

- [ ] Empty pending buffer (first read)
- [ ] Exact chunk boundary read (no leftover)
- [ ] Partial chunk leftover buffering
- [ ] Skip larger than pending buffer
- [ ] Skip exactly matches pending buffer size
- [ ] Stream error during read/skip
- [ ] End of stream during read/skip

---

## Expected Reduction

| Section | Current Lines | After Refactor | Savings |
|---------|---------------|----------------|---------|
| `read()` pending handling | 13 | 4 | 9 |
| `read()` stream loop | 26 | 19 | 7 |
| `skip()` pending handling | 13 | 2 | 11 |
| `skip()` stream loop | 22 | 17 | 5 |
| Manager read() duplicate | 16 | 4 | 12 |
| **New helper functions** | 0 | +22 | -22 |
| **Total** | **~90** | **~47** | **~40** |

**Net reduction: ~40 lines** (from ~505 to ~465 lines)

---

## Success Criteria

- [ ] All existing tests pass
- [ ] No functional changes to behavior
- [ ] Code coverage maintained or improved
- [ ] `cargo clippy` shows no new warnings
- [ ] Code is more readable and maintainable

---

## Related Tasks

- **STREAM-001**: Fix unwrap panic in stream access (line 384)
- **STREAM-002**: Fix check-then-act race condition (lines 372-407)
- **STREAM-003**: Add yielding in large skip operations

**Note**: Complete SIMPLIFY-011 before STREAM-001 and STREAM-002 for cleaner diffs.

---

*Created from code review - February 14, 2026*
