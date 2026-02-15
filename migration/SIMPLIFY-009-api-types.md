# SIMPLIFY-009: API Types Cleanup

## Scope

**Files to modify:**
- `src/api/types.rs` - Main types module
- `Cargo.toml` - Add strum dependency
- `src/api/client.rs` - Update field references (if any)
- `src/api/streaming.rs` - Update field references (if any)

## Current State

### Problem 1: Duplicate Speed Structs

Two nearly identical structs with duplicate code:

```rust
/// Download speed information from stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadSpeed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}

/// Upload speed information from stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UploadSpeed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}
```

### Problem 2: Manual Display Implementation

Manual `Display` impl for `TorrentState` (lines 338-349):

```rust
impl std::fmt::Display for TorrentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TorrentState::Downloading => write!(f, "downloading"),
            TorrentState::Seeding => write!(f, "seeding"),
            TorrentState::Paused => write!(f, "paused"),
            TorrentState::Stalled => write!(f, "stalled"),
            TorrentState::Error => write!(f, "error"),
            TorrentState::Unknown => write!(f, "unknown"),
        }
    }
}
```

### Problem 3: Manual JSON Serialization

Manual `to_json()` method in `TorrentStatus` (lines 413-425):

```rust
/// Get status as a JSON string for xattr
pub fn to_json(&self) -> String {
    format!(
        r#"{{"torrent_id":{},"state":"{}","progress_pct":{:.2},"progress_bytes":{},"total_bytes":{},"downloaded_pieces":{},"total_pieces":{}}}"#,
        self.torrent_id,
        self.state,
        self.progress_pct,
        self.progress_bytes,
        self.total_bytes,
        self.downloaded_pieces,
        self.total_pieces
    )
}
```

### Problem 4: Verbose Error Mappings

Overly verbose error mappings in `to_fuse_error()` (lines 73-118).

## Target State

### Solution 1: Unified Speed Struct

```rust
/// Speed information from stats endpoint (used for both download and upload)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Speed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}

// In LiveStats, update field types:
pub struct LiveStats {
    pub snapshot: TorrentSnapshot,
    #[serde(rename = "average_piece_download_time")]
    pub average_piece_download_time: Option<serde_json::Value>,
    #[serde(rename = "download_speed")]
    pub download_speed: Speed,
    #[serde(rename = "upload_speed")]
    pub upload_speed: Speed,
    #[serde(rename = "time_remaining")]
    pub time_remaining: Option<serde_json::Value>,
}
```

### Solution 2: strum Display Derive

```rust
use strum::Display;

/// Status of a torrent for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "snake_case")]
pub enum TorrentState {
    /// Torrent is downloading
    Downloading,
    /// Torrent is seeding (complete)
    Seeding,
    /// Torrent is paused
    Paused,
    /// Torrent appears stalled (no progress)
    Stalled,
    /// Torrent has encountered an error
    Error,
    /// Unknown state
    Unknown,
}
```

### Solution 3: Derive Serialize for TorrentStatus

```rust
use serde::Serialize;
use std::time::{Duration, SystemTime};

/// Comprehensive torrent status information
#[derive(Debug, Clone, Serialize)]
pub struct TorrentStatus {
    pub torrent_id: u64,
    pub state: TorrentState,
    pub progress_pct: f64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub downloaded_pieces: usize,
    pub total_pieces: usize,
    #[serde(with = "serde_millis")]
    pub last_updated: SystemTime,
}

impl TorrentStatus {
    /// Get status as a JSON string for xattr
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
```

Note: Requires adding `serde_millis` or custom serialization for `SystemTime`. Alternative: Keep `Instant` internally, convert to `SystemTime` for serialization.

### Solution 4: Simplified Error Mappings

```rust
impl ApiError {
    /// Map API errors to FUSE error codes
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            ApiError::TorrentNotFound(_) | ApiError::FileNotFound { .. } => libc::ENOENT,
            ApiError::ApiError { status, .. } => match status {
                400 | 416 => libc::EINVAL,
                401 | 403 => libc::EACCES,
                404 => libc::ENOENT,
                408 | 423 | 429 | 503 | 504 => libc::EAGAIN,
                409 => libc::EEXIST,
                413 => libc::EFBIG,
                500 | 502 => libc::EIO,
                _ => libc::EIO,
            },
            ApiError::InvalidRange(_) | ApiError::SerializationError(_) => libc::EINVAL,
            ApiError::ConnectionTimeout | ApiError::ReadTimeout => libc::EAGAIN,
            ApiError::ServerDisconnected => libc::ENOTCONN,
            ApiError::NetworkError(_) => libc::ENETUNREACH,
            ApiError::ServiceUnavailable(_) | ApiError::CircuitBreakerOpen | ApiError::RetryLimitExceeded => libc::EAGAIN,
            ApiError::HttpError(_) => libc::EIO,
        }
    }
}
```

## Implementation Steps

1. **Add strum to Cargo.toml**
   ```toml
   [dependencies]
   strum = { version = "0.25", features = ["derive"] }
   ```

2. **Merge speed structs** in `src/api/types.rs`:
   - Replace `DownloadSpeed` and `UploadSpeed` with single `Speed` struct
   - Update `LiveStats` to use `Speed` type for both fields
   - Keep serde rename attributes unchanged

3. **Add strum Display derive** to `TorrentState`:
   - Add `#[derive(Display)]` attribute
   - Add `#[strum(serialize_all = "snake_case")]` for automatic snake_case conversion
   - Remove the manual `impl std::fmt::Display for TorrentState` block

4. **Derive Serialize for TorrentStatus**:
   - Add `#[derive(Serialize)]` to `TorrentStatus`
   - Change `last_updated` from `Instant` to `SystemTime` (or add custom serialization)
   - Replace manual `to_json()` implementation with `serde_json::to_string(self)`
   - Update return type to `Result<String, serde_json::Error>`

5. **Simplify error mappings** in `to_fuse_error()`:
   - Group similar HTTP status codes
   - Remove redundant comments
   - Consolidate error variants with same FUSE code

6. **Update field references** (if any):
   - Search for usages of `DownloadSpeed` and `UploadSpeed` in codebase
   - Update to use `Speed` instead
   - Check `src/api/client.rs` and `src/api/streaming.rs`

7. **Run tests to verify**:
   ```bash
   cargo test
   cargo clippy
   cargo fmt
   ```

## Testing

### Unit Tests
1. Verify `Speed` struct serializes/deserializes correctly:
   ```bash
   cargo test types::tests::test_speed_serialization --lib
   ```

2. Verify `TorrentState` Display formatting:
   ```bash
   cargo test types::tests::test_torrent_state_display --lib
   ```

3. Verify `TorrentStatus::to_json()` produces valid JSON:
   ```bash
   cargo test types::tests::test_status_json --lib
   ```

### Integration Tests
1. Test full API response parsing with updated types
2. Verify streaming operations still work (check for field references)
3. Test error mapping with various API error responses

### Build Verification
```bash
cargo check
cargo clippy -- -D warnings
cargo fmt --check
```

## Expected Reduction

**Estimated line reduction: ~70 lines**

| Component | Current Lines | Target Lines | Reduction |
|-----------|---------------|--------------|-----------|
| DownloadSpeed struct | 6 | - | -6 |
| UploadSpeed struct | 6 | - | -6 |
| Speed struct (new) | - | 5 | +5 |
| Display impl for TorrentState | 12 | 2 (derive) | -10 |
| to_json() method | 13 | 3 | -10 |
| to_fuse_error() verbose comments | ~30 | ~10 | -20 |
| HTTP status match arms | ~25 | ~10 | -15 |
| Import statements | 2 | 3 | +1 |
| **Total** | **~94** | **~33** | **~61** |

Plus removal of duplicate doc comments and consolidation of similar code patterns.

## Verification Checklist

- [ ] `cargo build` succeeds without errors
- [ ] `cargo test` passes all tests
- [ ] `cargo clippy` shows no warnings
- [ ] `cargo fmt` produces clean formatting
- [ ] All references to `DownloadSpeed`/`UploadSpeed` updated to `Speed`
- [ ] `TorrentState` display format unchanged (e.g., "downloading" not "Downloading")
- [ ] `TorrentStatus::to_json()` output format preserved
- [ ] Error mappings produce same FUSE codes as before

## References

- [strum crate documentation](https://docs.rs/strum/latest/strum/)
- [serde serialization](https://serde.rs/derive.html)
- TODO.md - Task tracking file
- src/api/types.rs - Main file being refactored
