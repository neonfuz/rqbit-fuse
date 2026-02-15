# Migration Guide: SIMPLIFY-014 - Generic Request Helpers

## Task ID
**SIMPLIFY-014**: Create generic request helpers in `src/api/client.rs`

## Scope

**Files to Modify:**
- `src/api/client.rs`: Add generic helper methods and refactor existing methods

## Current State

The API client has ~8 methods that each repeat the same request-response pattern (~5 lines each = ~40 lines of duplicated code):

### Example: `get_torrent()` (lines 348-368)
```rust
pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
    let url = format!("{}/torrents/{}", self.base_url, id);
    let endpoint = format!("/torrents/{}", id);

    trace!(api_op = "get_torrent", id = id);

    let response = self
        .execute_with_retry(&endpoint, || self.client.get(&url).send())
        .await?;

    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound(id).into()),
        _ => {
            let response = self.check_response(response).await?;
            let torrent: TorrentInfo = response.json().await?;  // ← Repeated
            debug!(api_op = "get_torrent", id = id, name = %torrent.name);
            Ok(torrent)
        }
    }
}
```

### Example: `add_torrent_magnet()` (lines 371-388)
```rust
pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<AddTorrentResponse> {
    let url = format!("{}/torrents", self.base_url);
    let request = AddMagnetRequest {
        magnet_link: magnet_link.to_string(),
    };

    trace!(api_op = "add_torrent_magnet");

    let response = self
        .execute_with_retry("/torrents", || self.client.post(&url).json(&request).send())
        .await?;

    let response = self.check_response(response).await?;
    let result: AddTorrentResponse = response.json().await?;  // ← Same pattern

    debug!(api_op = "add_torrent_magnet", id = result.id, info_hash = %result.info_hash);
    Ok(result)
}
```

### Similar Patterns Found In:
1. `get_torrent()` - GET + 404 handling
2. `add_torrent_magnet()` - POST + JSON body
3. `add_torrent_url()` - POST + JSON body
4. `get_torrent_stats()` - GET + 404 handling
5. `pause_torrent()` - POST + 404 handling
6. `start_torrent()` - POST + 404 handling
7. `forget_torrent()` - POST + 404 handling
8. `delete_torrent()` - POST + 404 handling

## Target State

### New Generic Helper Methods (~15 lines)

```rust
impl RqbitClient {
    /// Generic GET request that returns JSON
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        url: &str,
    ) -> Result<T> {
        let response = self
            .execute_with_retry(endpoint, || self.client.get(url).send())
            .await?;
        let response = self.check_response(response).await?;
        Ok(response.json().await?)
    }

    /// Generic POST request with JSON body that returns JSON
    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .execute_with_retry(endpoint, || self.client.post(url).json(body).send())
            .await?;
        let response = self.check_response(response).await?;
        Ok(response.json().await?)
    }
}
```

### Refactored Methods Using Helpers (~25 lines total)

```rust
pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
    let url = format!("{}/torrents/{}", self.base_url, id);
    let endpoint = format!("/torrents/{}", id);

    trace!(api_op = "get_torrent", id = id);

    match self.get_json::<TorrentInfo>(&endpoint, &url).await {
        Ok(torrent) => {
            debug!(api_op = "get_torrent", id = id, name = %torrent.name);
            Ok(torrent)
        }
        Err(e) if e.downcast_ref::<ApiError>() == Some(&ApiError::TorrentNotFound(id)) => {
            Err(ApiError::TorrentNotFound(id).into())
        }
        Err(e) => Err(e),
    }
}

pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<AddTorrentResponse> {
    let url = format!("{}/torrents", self.base_url);
    let request = AddMagnetRequest {
        magnet_link: magnet_link.to_string(),
    };

    trace!(api_op = "add_torrent_magnet");

    let result = self.post_json::<_, AddTorrentResponse>("/torrents", &url, &request).await?;
    debug!(api_op = "add_torrent_magnet", id = result.id, info_hash = %result.info_hash);
    Ok(result)
}

pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<AddTorrentResponse> {
    let url = format!("{}/torrents", self.base_url);
    let request = AddTorrentUrlRequest {
        torrent_link: torrent_url.to_string(),
    };

    trace!(api_op = "add_torrent_url", url = %torrent_url);

    let result = self.post_json::<_, AddTorrentResponse>("/torrents", &url, &request).await?;
    debug!(api_op = "add_torrent_url", id = result.id, info_hash = %result.info_hash);
    Ok(result)
}

pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats> {
    let url = format!("{}/torrents/{}/stats/v1", self.base_url, id);
    let endpoint = format!("/torrents/{}/stats", id);

    trace!(api_op = "get_torrent_stats", id = id);

    let stats = self.get_json::<TorrentStats>(&endpoint, &url).await?;
    let progress_pct = if stats.total_bytes > 0 {
        (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0
    } else {
        0.0
    };
    trace!(
        api_op = "get_torrent_stats",
        id = id,
        state = %stats.state,
        progress_pct = progress_pct,
        finished = stats.finished,
    );
    Ok(stats)
}

pub async fn pause_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/pause", self.base_url, id);
    let endpoint = format!("/torrents/{}/pause", id);

    trace!(api_op = "pause_torrent", id = id);

    self.get_json::<serde_json::Value>(&endpoint, &url).await?;
    debug!(api_op = "pause_torrent", id = id, "Paused torrent");
    Ok(())
}

pub async fn start_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/start", self.base_url, id);
    let endpoint = format!("/torrents/{}/start", id);

    trace!(api_op = "start_torrent", id = id);

    self.get_json::<serde_json::Value>(&endpoint, &url).await?;
    debug!(api_op = "start_torrent", id = id, "Started torrent");
    Ok(())
}

pub async fn forget_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/forget", self.base_url, id);
    let endpoint = format!("/torrents/{}/forget", id);

    trace!(api_op = "forget_torrent", id = id);

    self.get_json::<serde_json::Value>(&endpoint, &url).await?;
    debug!(api_op = "forget_torrent", id = id, "Forgot torrent");
    Ok(())
}

pub async fn delete_torrent(&self, id: u64) -> Result<()> {
    let url = format!("{}/torrents/{}/delete", self.base_url, id);
    let endpoint = format!("/torrents/{}/delete", id);

    trace!(api_op = "delete_torrent", id = id);

    self.get_json::<serde_json::Value>(&endpoint, &url).await?;
    debug!(api_op = "delete_torrent", id = id, "Deleted torrent");
    Ok(())
}
```

## Implementation Steps

1. **Add generic helper methods** (after `check_response` method, around line 299):
   - Add `get_json<T>()` method
   - Add `post_json<B, T>()` method

2. **Refactor GET methods** (lines 348-448):
   - Update `get_torrent()` to use `get_json()`
   - Update `get_torrent_stats()` to use `get_json()`

3. **Refactor POST methods** (lines 371-408):
   - Update `add_torrent_magnet()` to use `post_json()`
   - Update `add_torrent_url()` to use `post_json()`

4. **Refactor control methods** (lines 704-785):
   - Update `pause_torrent()` to use `get_json::<serde_json::Value>()`
   - Update `start_torrent()` to use `get_json::<serde_json::Value>()`
   - Update `forget_torrent()` to use `get_json::<serde_json::Value>()`
   - Update `delete_torrent()` to use `get_json::<serde_json::Value>()`

5. **Handle 404 errors in helpers** (optional but recommended):
   - Option A: Keep 404 handling in each method (current approach)
   - Option B: Add an `expected_not_found` parameter to helpers
   - Option C: Use a custom result type that distinguishes 404

6. **Run verification**:
   ```bash
   cargo test
   cargo clippy
   cargo fmt
   ```

## Testing

### 1. Unit Tests (already exist in client.rs)
```bash
cargo test api::client::tests
```

Tests cover:
- `test_get_torrent_success`
- `test_get_torrent_not_found`
- `test_add_torrent_magnet_success`
- `test_add_torrent_url_success`
- `test_get_torrent_stats_success`
- `test_pause_torrent_success`
- `test_start_torrent_success`
- All error cases

### 2. Compile Check
```bash
cargo check
```

### 3. Lint Check
```bash
cargo clippy -- -D warnings
```

### 4. Format Check
```bash
cargo fmt -- --check
```

## Expected Reduction

| Metric | Before | After | Reduction |
|--------|--------|-------|-----------|
| Lines in 8 methods | ~40 lines | ~25 lines | **~15 lines** (38%) |
| Repeated `response.json().await?` | 8 times | 0 times | **8 instances** |
| Unique request patterns | 8 | 2 | **6 fewer patterns** |
| Cognitive complexity | High | Low | **Easier to maintain** |

### Benefits
1. **Reduced duplication**: Single point of change for JSON parsing
2. **Type safety**: Generic bounds enforce correct usage
3. **Readability**: Intent is clearer (GET JSON vs POST JSON)
4. **Maintainability**: Adding new endpoints requires less code

### Trade-offs
1. **Less explicit**: Debugging may require looking at helper
2. **404 handling**: May need adjustment if different 404 behaviors needed

## Notes

- The `get_piece_bitfield()` method has special handling (octet-stream, not JSON) and should **NOT** use these helpers
- The `read_file()` method has complex streaming logic and should **NOT** use these helpers
- The `list_torrents()` method makes N+1 calls and should **NOT** use these helpers for now

## Completion Criteria

- [ ] `get_json<T>()` helper added
- [ ] `post_json<B, T>()` helper added
- [ ] All 8 methods refactored to use helpers
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code formatted
- [ ] Line reduction verified (~15 lines)

---
*Created for SIMPLIFY-014 migration*
