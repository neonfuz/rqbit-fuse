# SIMPLIFY-006: Unify Torrent Discovery Logic

## Task ID
**SIMPLIFY-006**

## Scope

- **Primary File**: `src/fs/filesystem.rs`
- **Methods to Modify**:
  - `start_torrent_discovery()` (lines 199-264) - Background task
  - `refresh_torrents()` (lines 284-351) - Public async method  
  - `readdir()` (lines 1320-1427) - FUSE callback

## Current State

Currently, torrent discovery logic is duplicated in 3 places (~120 lines total). Each implementation has slight variations:

### 1. `start_torrent_discovery()` - Background Task (lines 199-264)

```rust
fn start_torrent_discovery(&self) {
    let api_client = Arc::clone(&self.api_client);
    let inode_manager = Arc::clone(&self.inode_manager);
    let last_discovery = Arc::clone(&self.last_discovery);
    let poll_interval = Duration::from_secs(30);

    let handle = tokio::spawn(async move {
        let mut ticker = interval(poll_interval);

        loop {
            ticker.tick().await;

            match api_client.list_torrents().await {
                Ok(torrents) => {
                    let mut new_count = 0;

                    for torrent_info in torrents {
                        if inode_manager.lookup_torrent(torrent_info.id).is_none() {
                            if let Err(e) = Self::create_torrent_structure_static(
                                &inode_manager,
                                &torrent_info,
                            ) {
                                warn!("Failed to create structure for torrent {}: {}", torrent_info.id, e);
                            } else {
                                new_count += 1;
                                info!("Discovered new torrent {}: {}", torrent_info.id, torrent_info.name);
                            }
                        }
                    }

                    if new_count > 0 {
                        info!("Background discovery found {} new torrents", new_count);
                    }

                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    last_discovery.store(now_ms, Ordering::SeqCst);
                }
                Err(e) => {
                    warn!("Failed to discover torrents in background task: {}", e);
                }
            }
        }
    });

    if let Ok(mut h) = self.discovery_handle.lock() {
        *h = Some(handle);
    }

    info!("Started background torrent discovery with {} second interval", 30);
}
```

**Characteristics:**
- Uses `create_torrent_structure_static()` (static method)
- No cooldown check (relies on 30-second interval)
- Updates `last_discovery` timestamp after completion
- Error handling: logs and continues

### 2. `refresh_torrents()` - Public Method (lines 284-351)

```rust
pub async fn refresh_torrents(&self, force: bool) -> bool {
    const COOLDOWN_MS: u64 = 5000;

    if !force {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_ms = self.last_discovery.load(Ordering::SeqCst);

        if last_ms != 0 && now_ms.saturating_sub(last_ms) < COOLDOWN_MS {
            trace!("Skipping torrent discovery - cooldown in effect");
            return false;
        }
    }

    match self.api_client.list_torrents().await {
        Ok(torrents) => {
            let mut new_count = 0;

            for torrent_info in torrents {
                if self.inode_manager.lookup_torrent(torrent_info.id).is_none() {
                    if let Err(e) = self.create_torrent_structure(&torrent_info) {
                        warn!("Failed to create structure for torrent {}: {}", torrent_info.id, e);
                    } else {
                        new_count += 1;
                        info!("Discovered new torrent {}: {}", torrent_info.id, torrent_info.name);
                    }
                }
            }

            if new_count > 0 {
                info!("Discovered {} new torrent(s)", new_count);
            } else {
                trace!("No new torrents found");
            }

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            self.last_discovery.store(now_ms, Ordering::SeqCst);

            true
        }
        Err(e) => {
            warn!("Failed to refresh torrents: {}", e);
            false
        }
    }
}
```

**Characteristics:**
- Uses `create_torrent_structure()` (instance method)
- Has cooldown check with `force` parameter
- Returns `bool` indicating if discovery ran
- Updates `last_discovery` timestamp after completion
- Different log message format

### 3. Inline Discovery in `readdir()` - FUSE Callback (lines 1335-1399)

```rust
// Inside readdir() method
if ino == 1 {
    let api_client = Arc::clone(&self.api_client);
    let inode_manager = Arc::clone(&self.inode_manager);
    let last_discovery = Arc::clone(&self.last_discovery);

    tokio::spawn(async move {
        const COOLDOWN_MS: u64 = 5000;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_ms = last_discovery.load(Ordering::SeqCst);

        let should_run = last_ms == 0 || now_ms.saturating_sub(last_ms) >= COOLDOWN_MS;

        if should_run {
            let claim_result = last_discovery.compare_exchange(
                last_ms,
                now_ms,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );

            if claim_result.is_ok() {
                if let Ok(torrents) = api_client.list_torrents().await {
                    let mut new_count = 0;

                    for torrent_info in torrents {
                        if inode_manager.lookup_torrent(torrent_info.id).is_none() {
                            if let Err(e) = Self::create_torrent_structure_static(
                                &inode_manager,
                                &torrent_info,
                            ) {
                                warn!("Failed to create structure for torrent {}: {}", torrent_info.id, e);
                            } else {
                                new_count += 1;
                                info!("Discovered new torrent {}: {}", torrent_info.id, torrent_info.name);
                            }
                        }
                    }

                    if new_count > 0 {
                        info!("Found {} new torrent(s) during directory listing", new_count);
                    }
                }
            } else {
                trace!("Lost race for discovery slot");
            }
        }
    });
}
```

**Characteristics:**
- Uses `create_torrent_structure_static()` (static method)
- Has cooldown check with atomic compare-and-exchange (race protection)
- Spawns task to avoid blocking (FUSE callback context)
- Different log message format
- No return value (fire-and-forget)

### Problems with Current Approach

1. **Code Duplication**: ~120 lines of nearly identical logic
2. **Inconsistent Error Handling**: Some log, some return Result
3. **Inconsistent Method Usage**: Mix of static vs instance `create_torrent_structure`
4. **Different Log Messages**: Same operation, different descriptions
5. **Maintenance Burden**: Changes must be applied in 3 places
6. **Behavioral Drift**: Risk of implementations diverging over time

## Target State

### New Unified Method: `discover_torrents()`

```rust
impl TorrentFS {
    /// Discover new torrents from rqbit and create filesystem structures.
    /// 
    /// This is the core discovery logic used by:
    /// - `start_torrent_discovery()` - background polling
    /// - `refresh_torrents()` - explicit refresh
    /// - `readdir()` - on-demand discovery when listing root
    ///
    /// # Arguments
    /// * `api_client` - Reference to the API client for listing torrents
    /// * `inode_manager` - Reference to the inode manager for structure creation
    ///
    /// # Returns
    /// * `Result<u64, anyhow::Error>` - Number of new torrents discovered, or error
    async fn discover_torrents(
        api_client: &Arc<RqbitClient>,
        inode_manager: &Arc<InodeManager>,
    ) -> Result<u64> {
        let torrents = api_client.list_torrents().await?;
        let mut new_count: u64 = 0;

        for torrent_info in torrents {
            // Check if we already have this torrent
            if inode_manager.lookup_torrent(torrent_info.id).is_none() {
                // New torrent found - create filesystem structure
                if let Err(e) = Self::create_torrent_structure_static(
                    inode_manager,
                    &torrent_info,
                ) {
                    warn!("Failed to create structure for torrent {}: {}", torrent_info.id, e);
                } else {
                    new_count += 1;
                    info!("Discovered new torrent {}: {}", torrent_info.id, torrent_info.name);
                }
            }
        }

        if new_count > 0 {
            info!("Discovered {} new torrent(s)", new_count);
        } else {
            trace!("No new torrents found");
        }

        Ok(new_count)
    }
}
```

### Refactored Callers

#### 1. `start_torrent_discovery()` - Now ~40 lines

```rust
fn start_torrent_discovery(&self) {
    let api_client = Arc::clone(&self.api_client);
    let inode_manager = Arc::clone(&self.inode_manager);
    let last_discovery = Arc::clone(&self.last_discovery);
    let poll_interval = Duration::from_secs(30);

    let handle = tokio::spawn(async move {
        let mut ticker = interval(poll_interval);

        loop {
            ticker.tick().await;

            match Self::discover_torrents(&api_client, &inode_manager).await {
                Ok(_) => {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    last_discovery.store(now_ms, Ordering::SeqCst);
                }
                Err(e) => {
                    warn!("Background torrent discovery failed: {}", e);
                }
            }
        }
    });

    if let Ok(mut h) = self.discovery_handle.lock() {
        *h = Some(handle);
    }

    info!("Started background torrent discovery with 30 second interval");
}
```

#### 2. `refresh_torrents()` - Now ~25 lines

```rust
pub async fn refresh_torrents(&self, force: bool) -> bool {
    const COOLDOWN_MS: u64 = 5000;

    // Check cooldown unless forced
    if !force {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_ms = self.last_discovery.load(Ordering::SeqCst);

        if last_ms != 0 && now_ms.saturating_sub(last_ms) < COOLDOWN_MS {
            let remaining_secs = (COOLDOWN_MS - (now_ms - last_ms)) / 1000;
            trace!("Skipping torrent discovery - cooldown in effect ({}s remaining)", remaining_secs);
            return false;
        }
    }

    // Perform discovery
    match Self::discover_torrents(&self.api_client, &self.inode_manager).await {
        Ok(_) => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            self.last_discovery.store(now_ms, Ordering::SeqCst);
            true
        }
        Err(e) => {
            warn!("Failed to refresh torrents: {}", e);
            false
        }
    }
}
```

#### 3. `readdir()` - Now ~15 lines in discovery section

```rust
// Inside readdir() method, when ino == 1
if ino == 1 {
    let api_client = Arc::clone(&self.api_client);
    let inode_manager = Arc::clone(&self.inode_manager);
    let last_discovery = Arc::clone(&self.last_discovery);

    tokio::spawn(async move {
        const COOLDOWN_MS: u64 = 5000;

        // Atomically check and claim discovery slot
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_ms = last_discovery.load(Ordering::SeqCst);

        let should_run = last_ms == 0 || now_ms.saturating_sub(last_ms) >= COOLDOWN_MS;

        if should_run {
            let claim_result = last_discovery.compare_exchange(
                last_ms,
                now_ms,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );

            if claim_result.is_ok() {
                if let Err(e) = Self::discover_torrents(&api_client, &inode_manager).await {
                    warn!("On-demand torrent discovery failed: {}", e);
                }
            }
        }
    });
}
```

## Implementation Steps

1. **Create the unified method** (lines 198-220)
   - Add `discover_torrents()` as private async method
   - Move core discovery logic (list + iterate + create)
   - Return `Result<u64>` (new torrent count)
   - Use consistent log messages

2. **Refactor `start_torrent_discovery()`** (lines 221-250)
   - Remove inline discovery logic
   - Call `Self::discover_torrents()` in the loop
   - Keep ticker interval and handle management
   - Update timestamp after successful discovery

3. **Refactor `refresh_torrents()`** (lines 251-280)
   - Remove inline discovery logic  
   - Keep cooldown check (specific to this caller)
   - Call `Self::discover_torrents()` after cooldown passes
   - Update timestamp and return bool

4. **Refactor `readdir()` inline discovery** (lines ~1335-1399)
   - Remove inline discovery logic
   - Keep atomic compare-exchange (race protection)
   - Call `Self::discover_torrents()` when slot claimed
   - Spawn task remains (FUSE callback constraint)

5. **Remove unused method** (lines 352-414)
   - `create_torrent_structure()` instance method is now unused
   - Only `create_torrent_structure_static()` remains (used by unified method)
   - Or keep both if needed elsewhere

6. **Update imports and verify compilation**
   - Ensure no unused imports
   - Run `cargo check`
   - Run `cargo clippy`

## Testing

### Verification Steps

1. **Build verification**
   ```bash
   cargo build
   cargo clippy -- -D warnings
   ```

2. **Unit tests**
   ```bash
   cargo test filesystem::tests
   ```

3. **Integration test - discovery scenarios**
   
   Create test in `tests/torrent_discovery.rs`:
   ```rust
   #[tokio::test]
   async fn test_refresh_torrents_respects_cooldown() {
       // Setup with mock API
       // First call should discover
       // Second call within cooldown should return false
   }

   #[tokio::test]
   async fn test_refresh_torrents_force_bypasses_cooldown() {
       // Setup with mock API
       // First call discovers
       // Second call with force=true should also discover
   }

   #[tokio::test]
   async fn test_discover_torrents_returns_count() {
       // Setup with mock returning 3 torrents
       // Verify discover_torrents returns 3
   }

   #[tokio::test]
   async fn test_discover_torrents_skips_existing() {
       // Setup with 2 existing, 1 new torrent
       // Verify discover_torrents returns 1
   }
   ```

4. **Manual testing checklist**
   - [ ] Mount filesystem
   - [ ] Add torrent via rqbit CLI
   - [ ] List root directory (`ls /mount/point`) - should trigger discovery
   - [ ] Verify torrent appears in listing
   - [ ] Call refresh endpoint - should respect cooldown
   - [ ] Force refresh - should bypass cooldown
   - [ ] Background task continues polling every 30s

5. **Log verification**
   - Check logs show consistent message format:
     - "Discovered {} new torrent(s)"
     - "No new torrents found"
     - "Failed to create structure for torrent {}"

## Expected Reduction

**Line Count Impact:**

| Component | Before | After | Savings |
|-----------|--------|-------|---------|
| `start_torrent_discovery()` | ~65 lines | ~40 lines | -25 |
| `refresh_torrents()` | ~65 lines | ~25 lines | -40 |
| `readdir()` discovery section | ~65 lines | ~15 lines | -50 |
| `discover_torrents()` (new) | 0 lines | ~35 lines | +35 |
| **Total** | ~195 lines | ~115 lines | **~80 lines** |

**Code Quality Improvements:**

1. **Single source of truth** for discovery logic
2. **Consistent error handling** via `Result<>` propagation
3. **Consistent logging** across all discovery paths
4. **Easier testing** - one method to unit test
5. **Easier maintenance** - changes in one place
6. **Clearer separation** of concerns:
   - `discover_torrents()` - WHAT to discover
   - Callers - WHEN and HOW to trigger

## Migration Checklist

- [ ] Implement `discover_torrents()` method
- [ ] Refactor `start_torrent_discovery()` to use unified method
- [ ] Refactor `refresh_torrents()` to use unified method
- [ ] Refactor `readdir()` inline discovery to use unified method
- [ ] Remove or deprecate `create_torrent_structure()` instance method
- [ ] Add unit tests for `discover_torrents()`
- [ ] Verify all existing tests pass
- [ ] Run manual discovery tests
- [ ] Update inline documentation
- [ ] Mark task complete in TODO.md

## Notes

- **FS-008** already fixed race condition in readdir() with compare_exchange
- The unified method should use `create_torrent_structure_static()` since all callers either:
  - Run in spawned task (background discovery, readdir)
  - Are async (refresh_torrents) and can call static method
- Consider if `create_torrent_structure()` instance method can be removed entirely
- All callers handle timestamp updates separately (appropriate since timing differs by context)
- Background task has no cooldown (30s interval is sufficient)
- refresh_torrents has 5s cooldown with force option
- readdir has 5s cooldown with atomic race protection
