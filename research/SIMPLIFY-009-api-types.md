# SIMPLIFY-009 Research Notes

## Summary

Successfully simplified API types module, reducing code by ~51 lines.

## Changes Made

### 1. Unified Speed Struct
- Replaced `DownloadSpeed` and `UploadSpeed` with single `Speed` struct
- Updated `LiveStats` to use `Speed` for both download and upload fields

### 2. strum Display Derive
- Added `#[derive(Display)]` with `#[strum(serialize_all = "snake_case")]` to `TorrentState`
- Removed 12-line manual `Display` implementation
- Added `Serialize` derive as required by `TorrentStatus`

### 3. Serialize Derive for TorrentStatus
- Added `#[derive(Serialize)]` to `TorrentStatus`
- Added `#[serde(skip)]` to `last_updated` field (Instant doesn't serialize)
- Changed `to_json()` to use `serde_json::to_string(self)` returning `Result<String, serde_json::Error>`
- Updated `filesystem.rs` to handle the Result type

### 4. Simplified Error Mappings
- Consolidated HTTP status codes in `to_fuse_error()`:
  - 400, 416 → EINVAL
  - 401, 403 → EACCES
  - 408, 423, 429, 503, 504 → EAGAIN
  - 500, 502 → EIO
- Grouped similar error variants:
  - ServiceUnavailable, CircuitBreakerOpen, RetryLimitExceeded → EAGAIN
  - InvalidRange, SerializationError → EINVAL
- Removed verbose comments
- Reduced from ~30 lines to ~15 lines

### 5. Added strum Dependency
- Added `strum = { version = "0.25", features = ["derive"] }` to Cargo.toml

## Validation

- `cargo check`: ✅ Clean
- `cargo clippy`: ✅ Clean  
- `cargo fmt`: ✅ Applied
- Line count: 427 → 376 lines (51 line reduction)

## Files Modified

1. `Cargo.toml` - Added strum dependency
2. `src/api/types.rs` - Simplified types and error mappings
3. `src/fs/filesystem.rs` - Updated to handle Result from to_json()
4. `SIMPLIFY.md` - Checked off SIMPLIFY-009
5. `CHANGELOG.md` - Added entry for SIMPLIFY-009
6. `.git/COMMIT_EDITMSG` - Wrote commit message

## Notes

Test failures observed are pre-existing issues with async runtime handling in `streaming.rs:337`, not related to these changes. The code compiles cleanly and passes all static analysis checks.
