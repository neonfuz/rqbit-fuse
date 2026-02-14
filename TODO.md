# Ralph-Loop Fix Checklist: Range Request Streaming

## Pre-Flight Checklist
- [x] torrent-fuse is currently mounted at `./dl2`
- [x] rqbit server is running at localhost:3030 (2 torrents available)
- [x] Test file exists: ubuntu-25.10-desktop-amd64.iso via API (returns 200, not 206 - confirms range issue)

## Iteration 1: Add Streaming Support to API Client ✅
**Goal**: Modify `read_file` to stream response and limit bytes when server ignores Range headers

**Files modified**:
- `src/api/client.rs`
- `Cargo.toml`

**Changes made**:
1. Added `futures = "0.3"` dependency to Cargo.toml
2. Enabled reqwest "stream" feature in Cargo.toml
3. Added import: `use futures::stream::StreamExt;` after line 11
4. Replaced lines 538-551 with streaming implementation that:
   - Detects when server returns 200 OK for range requests
   - Streams response and limits bytes to requested size
   - Logs warnings when server ignores Range headers

**Test Results**:
```bash
$ time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
0.04s total (was 2+ seconds)
```
✅ 4KB read: 40ms (was 2+ seconds) - 50x faster
✅ 64KB read: 5ms
✅ Data integrity verified against API
✅ Warning logs confirm server returns 200 OK instead of 206

---

## Iteration 2: Verify Range Header Format ✅
**Goal**: Ensure Range header format matches rqbit expectations exactly

**Files checked**:
- `src/api/client.rs` (line 505)

**Findings**:
- Range header format is correct: `bytes={start}-{end}` (inclusive, both values)
- Example: `bytes=0-4095` for 4096 bytes
- Format follows HTTP/1.1 specification exactly

**Test Results**:
```bash
$ curl -sI -H "Range: bytes=0-4095" http://localhost:3030/torrents/1/stream/0
HTTP/1.1 200 OK
accept-ranges: bytes
```

**Conclusion**: Range header format is correct. Server returns 200 OK instead of 206 Partial Content. This is a rqbit bug - server advertises `accept-ranges: bytes` but doesn't honor Range requests. Proceeded to Iteration 3 workaround.

---

## Iteration 3: Add Response Status Validation ✅
**Goal**: Detect when server returns 200 instead of 206 and apply byte limiting

**Status**: COMPLETED - Implemented as part of Iteration 1 streaming changes

**Files modified**:
- `src/api/client.rs` (lines 552-604)

**Implementation details**:
- Check response status before consuming body: `let status = response.status()`
- Detect full response: `is_full_response = status == 200 && range.is_some()`
- Log warning when server returns 200 OK for range requests
- Stream response with byte limiting when full response detected

**Log output confirms working**:
```
WARN ... Server returned 200 OK for range request, will limit to 4096 bytes
```

**Test Results**:
- 16KB read completes in <100ms (was 2+ seconds)
- Warning logs confirm detection of 200 OK responses
- Byte limiting ensures only requested data is returned

---

## Iteration 4: Optimize Chunk Size
**Goal**: Tune read size for best performance

**Files to modify**:
- `src/fs/filesystem.rs`

**Current value**: Line 1481 `const FUSE_MAX_READ: u32 = 4 * 1024; // 4KB`

**Benchmarking**:
```bash
# Test different chunk sizes
cargo build --release && umount dl2 && ./target/release/torrent-fuse mount -m ./dl2

# Test 4KB (current)
time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Test 16KB
time head -c 16384 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Test 64KB
time head -c 65536 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Test 256KB
time head -c 262144 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```

**Recommended values**:
- 4KB: Safe, works everywhere
- 16KB: Good balance
- 64KB: Optimal for most cases (matches rqbit buffer size from spec)
- 256KB+: May cause "Too much data" FUSE errors

**Changes**:
1. Update `FUSE_MAX_READ` constant based on benchmark results
2. Recommended: `64 * 1024` (64KB) - matches rqbit's internal buffer

**Final test**:
```bash
cargo build --release && umount dl2 && ./target/release/torrent-fuse mount -m ./dl2
time cat dl2/ubuntu-25.10-desktop-amd64.iso > /tmp/test_output.iso
cmp <(head -c 1000000 /tmp/test_output.iso) <(head -c 1000000 dl2/ubuntu-25.10-desktop-amd64.iso) && echo "Data matches!"
```
**Expected**: Copy completes in reasonable time, data integrity verified

---

## Research Files

### `research/streaming-implementation.md`
Complete implementation details for streaming response handling.

### `research/range-header-analysis.md`
Analysis of HTTP Range header formats and rqbit behavior.

### `research/benchmark-results.md`
Performance benchmarks for different chunk sizes.

---

## Quick Commands Reference

```bash
# Full rebuild and remount
cargo build --release && umount dl2 && ./target/release/torrent-fuse mount -m ./dl2

# Quick test (4KB read)
time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Medium test (64KB read)
time head -c 65536 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Large test (1MB read)
time head -c 1048576 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null

# Full file copy test
time cat dl2/ubuntu-25.10-desktop-amd64.iso > /tmp/full_copy.iso

# Verify data integrity
cmp /tmp/full_copy.iso dl2/ubuntu-25.10-desktop-amd64.iso

# Check logs for warnings
tail -f /tmp/torrent-fuse.log | grep -E "(WARN|ERROR|streaming|range)"
```

---

## Success Criteria

- [ ] 4KB read completes in <500ms
- [ ] 64KB read completes in <1s
- [ ] 1MB read completes in <5s
- [ ] No "Too much data" FUSE errors
- [ ] File copy produces identical output
- [ ] Log shows warning when server ignores Range headers (if applicable)
