# FIXES

A prioritized list of bugs and issues discovered during development, in Ralph style (focused, actionable, no ceremony).

## Active Issues

### [P0] Multi-file torrent directories appear empty

**Status:** 🔍 Investigating  
**Discovered:** 2026-02-14  
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

**Suspected Cause:**
Runtime visibility issue with DashMap concurrent hash map. The code logic is correct:
1. `create_file_entry()` allocates file inode
2. `add_child()` adds file to directory's children vector
3. `get_children()` reads children from directory entry

But the children may not be visible when `readdir()` is called due to DashMap's lock-free nature.

**Next Steps:**
- [ ] Add debug logging to verify inode numbers during creation
- [ ] Run integration test `test_multi_file_torrent_structure` to confirm test passes
- [ ] Check if `get_children()` fallback (filter by parent) finds files when children vector doesn't
- [ ] Consider using blocking synchronization during torrent structure creation

**Files to Modify:**
- `src/fs/filesystem.rs` - Add debug logging
- `src/fs/inode.rs` - Verify `get_children()` behavior

---

### [P1] Stats API mismatch causing warnings

**Status:** 🔧 Identified  
**Discovered:** 2026-02-14  
**Reporter:** User (neonfuz)

**Problem:**
- Repeated warnings: `missing field 'file_count' at line 1 column 597`
- Stats endpoint returns different structure than expected

**Actual API Response:**
```json
{
  "snapshot": { "downloaded_and_checked_bytes": 0, ... },
  "download_speed": { "mbps": 0.0, "human_readable": "0.00 MiB/s" }
}
```

**Expected by Code:**
```rust
pub struct TorrentStats {
    pub file_count: usize,      // Missing from API
    pub progress_pct: f64,      // Missing from API
    pub total_bytes: u64,       // Missing from API
    ...
}
```

**Impact:** 
- Warning spam every 5 seconds (monitoring interval)
- No actual functionality broken - monitoring just logs warnings

**Fix Options:**
1. **Option A:** Update `TorrentStats` struct to match actual API (preferred)
2. **Option B:** Make all fields optional with `#[serde(default)]`
3. **Option C:** Silently ignore parse errors in monitoring loop

**Next Steps:**
- [ ] Verify stats endpoint behavior across different rqbit versions
- [ ] Decide on fix approach (likely Option A or B)
- [ ] Update `src/api/types.rs`

---

### [P2] file_count field inconsistent across API endpoints

**Status:** ✅ Fixed  
**Discovered:** 2026-02-14  
**Fixed:** 2026-02-14

**Problem:**
- `TorrentInfo.file_count` was required but API doesn't always return it
- Caused deserialization errors when listing torrents

**Fix:**
Made `file_count: Option<usize>` in `TorrentInfo` struct.

**Files Modified:**
- `src/api/types.rs` - Added `TorrentSummary`, made `file_count` optional
- `src/api/client.rs` - Updated `list_torrents()` to fetch full details per torrent

**Note:** Test files need updating for `Some(...)` wrappers:
- `src/api/client.rs:886`
- `src/fs/filesystem.rs:2075, 2114`
- `tests/integration_tests.rs` (multiple locations)

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
