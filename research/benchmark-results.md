# Benchmark Results Template

## Test Environment
- **File**: ubuntu-25.10-desktop-amd64.iso (5.7GB)
- **Chunk sizes tested**: 4KB, 16KB, 64KB, 256KB, 1MB
- **Metric**: Time to read N bytes using `time head -c N`

## Results

### 4KB Reads (Current: 4KB chunk size)
```bash
time head -c 4096 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Results**:
- Before fix: ~2-5 seconds
- After fix: TBD

### 16KB Reads
```bash
time head -c 16384 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Results**: TBD

### 64KB Reads
```bash
time head -c 65536 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Results**: TBD

### 256KB Reads
```bash
time head -c 262144 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Results**: TBD

### 1MB Reads
```bash
time head -c 1048576 dl2/ubuntu-25.10-desktop-amd64.iso > /dev/null
```
**Results**: TBD

## Chunk Size Tuning

File: `src/fs/filesystem.rs` line 1481

```rust
const FUSE_MAX_READ: u32 = 4 * 1024; // 4KB
```

### Recommended Values

| Chunk Size | Pros | Cons | Use Case |
|------------|------|------|----------|
| 4KB | Safe, works everywhere | Slower for large reads | Default, compatibility |
| 16KB | Good balance | Moderate overhead | General use |
| 64KB | Optimal, matches rqbit buffer | Higher memory usage | Recommended |
| 256KB | Fastest sequential reads | Risk of "Too much data" | Large file copies |
| 1MB+ | Very fast | High memory, may crash | Not recommended |

### rqbit Buffer Size

From spec (`rqbit/spec/streaming-api.md` line 128):
```rust
let s = tokio_util::io::ReaderStream::with_capacity(stream, 65536);
```

rqbit uses a 64KB (65536 bytes) buffer internally. Matching this should be optimal.

## Recommended Setting

After benchmarking, set:
```rust
const FUSE_MAX_READ: u32 = 64 * 1024; // 64KB
```

This matches:
- rqbit's internal buffer size
- Common OS page size (4KB) × 16
- Good balance between throughput and memory

## Full File Copy Test

Final validation:
```bash
time cat dl2/ubuntu-25.10-desktop-amd64.iso > /tmp/test_output.iso
cmp /tmp/test_output.iso dl2/ubuntu-25.10-desktop-amd64.iso && echo "Data matches!"
```

**Expected**: Copy completes in reasonable time, data integrity verified

## Notes

- "Too much data" FUSE error occurs when response exceeds FUSE buffer
- Larger chunks = fewer API calls = better performance
- But larger chunks = more memory = higher latency
- 64KB is the sweet spot for this use case
