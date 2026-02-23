# Migration Guide: SIMPLIFY-002 - Unify Torrent Control Methods

## Task ID
SIMPLIFY-002

## Scope

**Files to modify:**
- `src/api/client.rs` - Add helper method and simplify existing methods

## Current State

Four nearly identical torrent control methods (~72 lines total):

```rust
/// Pause a torrent
pub async fn pause_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/pause", self.base_url, id);
    let endpoint = format!("/torrents/{}/pause", id);

    trace!(api_op = "pause_torrent", id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.post(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            self.check_response(response).await?;
            debug!(api_op = "pause_torrent", id = id, "Paused torrent");
            Ok(())
        }
    }
}

/// Resume/start a torrent
pub async fn start_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/start", self.base_url, id);
    let endpoint = format!("/torrents/{}/start", id);

    trace!(api_op = "start_torrent", id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.post(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            self.check_response(response).await?;
            debug!(api_op = "start_torrent", id = id, "Started torrent");
            Ok(())
        }
    }
}

/// Remove torrent from session (keep files)
pub async fn forget_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/forget", self.base_url, id);
    let endpoint = format!("/torrents/{}/forget", id);

    trace!(api_op = "forget_torrent", id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.post(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            self.check_response(response).await?;
            debug!(api_op = "forget_torrent", id = id, "Forgot torrent");
            Ok(())
        }
    }
}

/// Remove torrent from session and delete files
pub async fn delete_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/delete", self.base_url, id);
    let endpoint = format!("/torrents/{}/delete", id);

    trace!(api_op = "delete_torrent", id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.post(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            self.check_response(response).await?;
            debug!(api_op = "delete_torrent", id = id, "Deleted torrent");
            Ok(())
        }
    }
}
```

## Target State

A single helper method and 4 one-liner public methods (~12 lines total):

```rust
/// Execute a torrent action (pause, start, forget, delete)
async fn torrent_action(&self, id: u64, action: &str) -> Result<()> {
    let url = format!("{}/torrents/{}/{}", self.base_url, id, action);
    let endpoint = format!("/torrents/{}/{}", id, action);

    trace!(api_op = action, id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.post(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            self.check_response(response).await?;
            debug!(api_op = action, id = id, "{} torrent", action);
            Ok(())
        }
    }
}

/// Pause a torrent
pub async fn pause_torrent(&self, id: u64) -> Result<()> {
    self.torrent_action(id, "pause").await
}

/// Resume/start a torrent
pub async fn start_torrent(&self, id: u64) -> Result<()> {
    self.torrent_action(id, "start").await
}

/// Remove torrent from session (keep files)
pub async fn forget_torrent(&self, id: u64) -> Result<()> {
    self.torrent_action(id, "forget").await
}

/// Remove torrent from session and delete files
pub async fn delete_torrent(&self, id: u64) -> Result<()> {
    self.torrent_action(id, "delete").await
}
```

## Implementation Steps

1. **Open `src/api/client.rs`**
   - Navigate to the "Torrent Control" section (around line 700)

2. **Add the helper method**
   - Insert `torrent_action()` method right before `pause_torrent()`
   - Make it `async fn torrent_action(&self, id: u64, action: &str) -> Result<()>`
   - Copy the common logic, using `action` parameter for URL construction

3. **Simplify the 4 public methods**
   - Replace each method body with a single line calling `self.torrent_action(id, "action_name").await`
   - Keep the doc comments and public visibility
   - Remove all duplicate code

4. **Update the debug log message**
   - In `torrent_action()`, use `debug!(api_op = action, id = id, "{} torrent", action);`
   - This will produce messages like "pause torrent", "start torrent", etc.

5. **Run clippy and format**
   ```bash
   cargo clippy
   cargo fmt
   ```

## Testing

Verify the refactoring didn't break anything:

```bash
# Run all tests
cargo test

# Specifically run the torrent control tests
cargo test pause_torrent
cargo test start_torrent
cargo test forget_torrent  
cargo test delete_torrent

# Run all API client tests
cargo test client::tests
```

**Expected test results:**
- All existing tests should pass
- No new tests needed (behavior is unchanged)
- Mock server tests will verify correct endpoints are still called

## Expected Reduction

- **Lines removed:** ~60 lines (72 → 12)
- **Code duplication:** Eliminated
- **Maintainability:** Improved - changes to error handling only needed in one place
- **API compatibility:** 100% preserved - all public methods have same signature

## Notes

- The `action` parameter uses `&str` for flexibility (accepts string literals)
- Logging uses the action name directly, so logs will show "pause", "start", etc.
- Error handling remains identical - `TorrentNotFound` is still returned for 404s
- This pattern can be extended if more torrent actions are added in the future
