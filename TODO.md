# Ralph-Loop Fix Checklist: Range Request Streaming

## Pre-Flight Checklist
- [ ] torrent-fuse is currently mounted at `./dl2`
- [ ] rqbit server is running at localhost:3030
- [ ] Test file exists: `dl2/ubuntu-25.10-desktop-amd64.iso` (or similar large file)

## Iteration 1: Add Streaming Support to API Client
**Goal**: Modify `read_file` to stream response and limit bytes when server ignores Range headers

**Files to modify**:
- `src/api/client.rs`

**Changes**:
1. Add import: `use futures::stream::StreamExt;` (after line 11)
2. Replace lines 538-551 (the `_ =>` match arm) with streaming implementation

**Implementation details**: See `research/streaming-implementation.md`

**Testing**:
```bash
cargo build --release && umount dl2 && ./target/release/torrent-fuse mount -m ./dl2
time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Expected**: Should complete in <1 second instead of 2+ seconds

---

## Iteration 2: Verify Range Header Format
**Goal**: Ensure Range header format matches rqbit expectations exactly

**Files to modify**:
- `src/api/client.rs`

**Research**: See `research/range-header-analysis.md`

**Changes**:
1. Check current Range header format at line 505
2. Verify it's `bytes={start}-{end}` (inclusive, both values required)
3. If using open-ended range (`bytes={start}-`), change to explicit end

**Testing**:
```bash
curl -v -H "Range: bytes=0-4095" http://localhost:3030/torrents/1/stream/0 2>&1 | grep -E "(Range|HTTP|Content)"
```
**Expected**: Should see "Range: bytes=0-4095" in request, "206 Partial Content" in response

**If still getting 200 OK**: Proceed to Iteration 3 (rqbit bug workaround)

---

## Iteration 3: Add Response Status Validation
**Goal**: Detect when server returns 200 instead of 206 and apply byte limiting

**Files to modify**:
- `src/api/client.rs`

**Changes**:
1. Check response status BEFORE consuming body
2. If status == 200 AND range was requested, apply byte limit
3. If status == 206, read normally (server is working correctly)

**Implementation**:
```rust
let status = response.status();
let is_range_request = range.is_some();
let is_full_response = status == StatusCode::OK && is_range_request;

if is_full_response {
    warn!("Server returned 200 OK for range request, limiting to {} bytes", requested_size);
}
```

**Testing**:
```bash
cargo build --release && umount dl2 && ./target/release/torrent-fuse mount -m ./dl2
# In another terminal:
time head -c 16384 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Expected**: Should complete quickly, log should show warning about 200 OK

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
