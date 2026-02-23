# SIMPLIFY-013: Add tracing::instrument to API Client Methods

**Status**: Ready to implement  
**Priority**: Low (Code simplification)  
**Estimated Time**: 30 minutes  
**Expected Reduction**: ~30 lines

---

## Overview

Add `#[tracing::instrument]` attribute to public API methods in `src/api/client.rs` to eliminate manual `trace!` and `debug!` logging calls. This simplifies the code, reduces boilerplate, and provides automatic structured logging with function arguments.

---

## Scope

**Files to modify:**
- `src/api/client.rs` - Add `#[tracing::instrument]` to ~12 public methods

**No changes required to:**
- Tests (already verify functionality)
- Other modules (isolated to API client)

---

## Current State

Methods contain manual logging with `trace!` at entry points and `debug!` at exit points:

```rust
// Lines 308-346: list_torrents
pub async fn list_torrents(&self) -> Result<Vec<TorrentInfo>> {
    let url = format!("{}/torrents", self.base_url);
    
    trace!(api_op = "list_torrents", url = %url);  // Entry logging
    
    let response = self
        .execute_with_retry("/torrents", || self.client.get(&url).send())
        .await?;
    
    // ... logic ...
    
    debug!(  // Exit logging
        api_op = "list_torrents",
        count = full_torrents.len(),
        "Listed torrents with full details"
    );
    Ok(full_torrents)
}

// Lines 349-368: get_torrent
pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
    trace!(api_op = "get_torrent", id = id);
    
    // ... logic ...
    
    debug!(api_op = "get_torrent", id = id, name = %torrent.name);
    Ok(torrent)
}

// Similar pattern in:
// - add_torrent_magnet (lines 371-388)
// - add_torrent_url (lines 391-408)
// - get_torrent_stats (lines 411-448)
// - get_piece_bitfield (lines 451-491)
// - read_file (lines 501-641)
// - read_file_streaming (lines 657-675)
// - pause_torrent (lines 704-722)
// - start_torrent (lines 725-743)
// - forget_torrent (lines 746-764)
// - delete_torrent (lines 767-785)
```

**Total manual logging lines to remove**: ~40 lines across 12 methods

---

## Target State

Methods use `#[tracing::instrument]` for automatic entry/exit tracing:

```rust
use tracing::instrument;

#[instrument(skip(self), fields(api_op = "list_torrents"))]
pub async fn list_torrents(&self) -> Result<Vec<TorrentInfo>> {
    let url = format!("{}/torrents", self.base_url);
    
    // Automatic trace on entry with:
    // - function name
    // - arguments (url not logged due to skip)
    // - api_op field
    
    let response = self
        .execute_with_retry("/torrents", || self.client.get(&url).send())
        .await?;
    
    // ... logic ...
    
    // trace! on success with return value via instrument
    Ok(full_torrents)
}

#[instrument(skip(self), fields(api_op = "get_torrent", id))]
pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
    // Automatic trace includes 'id' parameter
    
    // ... logic ...
    
    Ok(torrent)
}
```

---

## Implementation Steps

1. **Add import at top of file** (line 14):
   ```rust
   use tracing::{debug, error, instrument, trace, warn};
   ```

2. **Instrument each public API method** with `#[instrument]`:
   - Use `skip(self)` to avoid logging the entire client struct
   - Add `fields(api_op = "...")` to maintain consistent operation naming
   - Include relevant parameters (id, torrent_id, etc.) in fields

3. **Methods to instrument**:
   - [ ] `list_torrents` (line 308) - skip self, add api_op field
   - [ ] `get_torrent` (line 349) - skip self, add api_op + id fields
   - [ ] `add_torrent_magnet` (line 371) - skip self, add api_op field
   - [ ] `add_torrent_url` (line 391) - skip self, add api_op + url fields
   - [ ] `get_torrent_stats` (line 411) - skip self, add api_op + id fields
   - [ ] `get_piece_bitfield` (line 451) - skip self, add api_op + id fields
   - [ ] `read_file` (line 501) - skip self, add api_op + torrent_id + file_idx fields
   - [ ] `read_file_streaming` (line 657) - skip self, add all relevant fields
   - [ ] `pause_torrent` (line 704) - skip self, add api_op + id fields
   - [ ] `start_torrent` (line 725) - skip self, add api_op + id fields
   - [ ] `forget_torrent` (line 746) - skip self, add api_op + id fields
   - [ ] `delete_torrent` (line 767) - skip self, add api_op + id fields

4. **Remove manual trace!/debug! calls** from instrumented methods

5. **Keep warn! and error! calls** - These should remain for actual error conditions

6. **Run tests to verify**:
   ```bash
   cargo test --lib api::client::tests
   ```

---

## Testing

**Verify functionality unchanged:**
```bash
# Run all API client tests
cargo test --lib api::client::tests

# Verify no compilation errors
cargo check

# Verify formatting
cargo fmt -- --check

# Run clippy
cargo clippy -- -D warnings
```

**Visual verification:**
1. Ensure all public API methods have `#[instrument]` attribute
2. Confirm ~30-40 lines of manual logging removed
3. Check that `warn!` and `error!` calls are preserved
4. Verify import statement includes `instrument`

---

## Expected Line Reduction

**Before**: ~40 lines of manual `trace!` and `debug!` calls  
**After**: ~12 `#[instrument]` attributes (~1 line each)  
**Net reduction**: ~28-30 lines

---

## Notes

- `#[tracing::instrument]` automatically creates spans at the TRACE level
- Span includes function name, arguments (unless skipped), and custom fields
- On function exit, the span is closed automatically
- Use `skip(self)` to avoid serializing large structs in logs
- Keep `warn!` and `error!` for actual warnings and errors
- Consider adding `level = "trace"` explicitly for clarity

---

## Related Tasks

- None - This is a standalone simplification
