# FIXES

A prioritized list of bugs and issues discovered during development, in Ralph style (focused, actionable, no ceremony).

## Active Issues

### [P0] Multi-file torrent directories appear empty

**Status:** ✅ Fixed  
**Discovered:** 2026-02-14  
**Fixed:** 2026-02-14  
**Reporter:** User (neonfuz)

**Problem:**
- Multi-file torrents create directories correctly (e.g., "Cosmos Laundromat/")
- Directory appears in listing, but is empty
- Single-file torrents work correctly (e.g., "ubuntu-25.10-desktop-amd64.iso")

**Evidence:**
```
$ tree dl2
dl2
├── Cosmos Laundromat  # Empty directory - should have 6 files
└── ubuntu-25.10-desktop-amd64.iso  # Works
```

**Logs show files ARE being created:**
```
INFO Created filesystem structure for torrent 0 with 6 files
INFO Created filesystem structure for torrent 1 with 1 files
```

**Root Cause:**
In `inode.rs:get_children()`, the code first checked if the parent was a directory with a children list, and if so, returned those children. However, due to DashMap's lock-free concurrent nature, writes to the children vector in `add_child()` might not be immediately visible to readers on different threads. The fallback mechanism (filtering all entries by parent) only triggered when the parent wasn't a directory, not when the children list was empty.

**Fix:**
Modified `get_children()` in `src/fs/inode.rs` to check if the children vector is empty and use the fallback filtering method in that case:

```rust
// If children list is populated, use it
if !children.is_empty() {
    return children
        .iter()
        .filter_map(|&child_ino| {
            self.entries.get(&child_ino).map(|e| (child_ino, e.clone()))
        })
        .collect();
}
```

This ensures that even when the children vector hasn't been populated yet (due to eventual consistency), the fallback correctly finds children by filtering the entries map by parent inode.

**Files Modified:**
- `src/fs/inode.rs` - Fixed `get_children()` to use fallback when children vector is empty

---

### [P1] Stats API mismatch causing warnings

**Status:** ✅ Fixed  
**Discovered:** 2026-02-14  
**Fixed:** 2026-02-14  
**Reporter:** User (neonfuz)

**Problem:**
- Repeated warnings: `missing field 'file_count' at line 1 column 597`
- Stats endpoint returns different structure than expected

**Actual API Response:**
```json
{
  "snapshot": { "downloaded_and_checked_bytes": 0, "total_bytes": 0, ... },
  "download_speed": { "mbps": 0.0, "human_readable": "0.00 MiB/s" }
}
```

**Expected by Code (OLD):**
```rust
pub struct TorrentStats {
    pub file_count: usize,      // Missing from API
    pub progress_pct: f64,      // Missing from API
    pub total_bytes: u64,       // Missing from API
    ...
}
```

**Fix:**
Updated `TorrentStats` struct to match actual API v1 response structure:
- Added `TorrentSnapshot` struct with `downloaded_and_checked_bytes`, `total_bytes`, and optional fields
- Added `DownloadSpeed` struct with `mbps` and `human_readable`
- Updated `TorrentStats` to contain `snapshot` and `download_speed` fields
- Updated `TorrentStatus::new()` to calculate `progress_pct` from bytes
- Updated trace logging in client to use new fields
- Updated tests to mock correct response structure

**Files Modified:**
- `src/api/types.rs` - Replaced TorrentStats with new structure matching API
- `src/api/client.rs` - Updated trace logging and test mocks

**Result:**
- All 98 tests passing
- No clippy warnings
- Warning spam eliminated
- API responses deserialize correctly

---

### [P2] file_count field inconsistent across API endpoints

**Status:** ✅ Fixed  
**Discovered:** 2026-02-14  
**Fixed:** 2026-02-14

**Problem:**
- `TorrentInfo.file_count` was required but API doesn't always return it
- Caused deserialization errors when listing torrents
- Also caused test compilation failures after struct change

**Fix:**
Made `file_count: Option<usize>` in `TorrentInfo` struct.
Updated all test files to wrap file_count values in `Some()`.
Fixed `test_list_torrents_success` to mock individual torrent endpoint.

**Files Modified:**
- `src/api/types.rs` - Added `TorrentSummary`, made `file_count` optional
- `src/api/client.rs` - Updated `list_torrents()` to fetch full details per torrent
- `src/api/client.rs` - Fixed test to mock `/torrents/1` endpoint
- `src/fs/filesystem.rs` - Updated test file_count values to `Some(...)`
- `tests/integration_tests.rs` - Updated all file_count values to `Some(...)`

---

## Completed Issues

### ✅ FUSE panic on large reads - "Too much data: TryFromIntError"

**Status:** ✅ Fixed  
**Discovered:** 2026-02-14  
**Fixed:** 2026-02-14

**Problem:**
- Filesystem panicked when reading files with `cat`
- Error: `Too much data: TryFromIntError(())` at `fuser-0.14.0/src/ll/reply.rs:47`
- `cat` requests large reads (>128KB) that exceed FUSE protocol limits

**Root Cause:**
The FUSE protocol's `fuse_out_header.len` field is `u32`, limiting response size. When the kernel requested large reads, fuser panicked during the size conversion.

**Fix:**
Added `FUSE_MAX_READ` constant (128KB) and clamp read size in the `read()` callback before making HTTP requests.

**Code Change:**
```rust
impl TorrentFS {
    const FUSE_MAX_READ: u32 = 128 * 1024; // 128KB
}

// In read() callback:
let size = std::cmp::min(size, Self::FUSE_MAX_READ);
```

**Files Modified:**
- `src/fs/filesystem.rs` - Added size clamping in `read()` callback

---

### ✅ Torrent discovery on mount

**Status:** Complete  
**Fixed:** 2026-02-14

**Problem:**
- Filesystem mounted empty even when rqbit had torrents
- Only torrents added through torrent-fuse appeared

**Root Cause:**
`init()` function didn't call `list_torrents()` to discover existing torrents.

**Fix:**
- Added `discover_existing_torrents()` function
- Called before mounting in `run()`

**Files Modified:**
- `src/fs/filesystem.rs` - Added discovery function
- `src/lib.rs` - Integrated discovery into startup flow

---

## Ralph's Rules for This File

1. **No fluff** - Just the facts, stack traces, and file paths
2. **P0/P1/P2** - Priority levels: broken/unusable, annoying/workaroundable, polish
3. **Status emojis** - 🔴 P0, 🟡 P1, 🟢 P2, 🔍 investigating, 🔧 identified, ✅ fixed
4. **Next steps are checkboxes** - Actionable items you can actually do
5. **Evidence over theory** - Logs, API responses, actual behavior
6. **Suspected ≠ confirmed** - Distinguish between "might be" and "is"
7. **Files at the bottom** - Always list files that need modification

## How to Use

- Add issues as you find them
- Move items to Completed when fixed
- Update Status as investigation progresses
- Check off Next Steps as you go
- Delete items older than 30 days from Completed

---

*Last updated: 2026-02-14*
