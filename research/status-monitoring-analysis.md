# Status Monitoring Analysis

**Date:** 2026-02-23  
**Source:** `src/fs/filesystem.rs` lines 177-244  
**Author:** Code Review Task 1.1.1

## Overview

The status monitoring system is a background task that periodically polls the rqbit API for torrent statistics and maintains a cache of torrent statuses in memory.

## What It Does

### Core Functionality

1. **Periodic Polling** (lines 183-234)
   - Runs in a background tokio task spawned during filesystem initialization
   - Polls every `status_poll_interval` seconds (configurable, default: 60)
   - Iterates through all torrent IDs stored in `torrent_statuses` DashMap

2. **Status Collection** (lines 193-211)
   - For each torrent:
     - Fetches torrent statistics via `api_client.get_torrent_stats(torrent_id)`
     - Fetches piece bitfield via `api_client.get_piece_bitfield(torrent_id)` (optional)
     - Creates a `TorrentStatus` struct with aggregated data
     - Detects stalled torrents by comparing last update time to `stalled_timeout`
     - Updates the `torrent_statuses` cache with new status

3. **Stalled Detection** (lines 203-208)
   - Compares time since last update against `stalled_timeout` (default: 300 seconds)
   - If exceeded and torrent not complete, marks state as `TorrentState::Stalled`

4. **Error Handling** (lines 214-220)
   - On API failure, logs warning and marks torrent status as `TorrentState::Error`

### Data Structure

**`torrent_statuses: Arc<DashMap<u64, TorrentStatus>>`** (line 69)
- Key: Torrent ID (u64)
- Value: `TorrentStatus` struct containing:
  - `torrent_id`: u64
  - `state`: TorrentState (Live, Paused, Complete, Error, Stalled, Unknown)
  - `progress_pct`: f64 (0.0 - 100.0)
  - `progress_bytes`: u64
  - `total_bytes`: u64
  - `downloaded_pieces`: usize
  - `total_pieces`: usize
  - `last_updated`: Instant (when status was last refreshed)

### Task Lifecycle

- **Started:** `start_status_monitoring()` called in `init()` (line 2102)
- **Stopped:** `stop_status_monitoring()` called in `destroy()` (line 2122)
- **Handle Storage:** `monitor_handle: Arc<Mutex<Option<JoinHandle<()>>>>` (line 73)

## Where It's Used

### Direct Usage in Filesystem

1. **Lines 772-789**: Public API methods (unused by core filesystem):
   - `get_torrent_status()` - Returns status for single torrent
   - `monitor_torrent()` - Adds torrent to monitoring
   - `unmonitor_torrent()` - Removes torrent from monitoring  
   - `list_torrent_statuses()` - Returns all monitored statuses

2. **Line 889**: `check_pieces_available()` method
   - Uses `torrent_statuses.get(&torrent_id)` to check if torrent is complete
   - Falls back to progress percentage approximation if not complete
   - **Note:** This is a simplified check that doesn't use the actual bitfield

3. **Lines 1203-1216**: Read operation (read handler)
   - Checks if torrent has started (progress_bytes > 0)
   - Returns `EAGAIN` if torrent not ready or not monitored
   - **Note:** This provides early-exit optimization only

4. **Lines 1220-1227**: Piece checking for paused torrents
   - Skips piece check if torrent is complete (using `status.is_complete()`)
   - **Note:** Actual piece availability check uses API client directly, not this cache

5. **Lines 2005-2025**: Extended attribute handler (getxattr)
   - Returns torrent status as JSON when reading `user.torrent.status` xattr
   - **Note:** This is a diagnostic/debugging feature only

6. **Lines 306, 470, 721**: Cleanup operations
   - Removes entries from `torrent_statuses` during torrent removal/unmount

### What Would Break If Removed

#### Critical Functionality (Would Break)

**NONE** - The status monitoring task is not used for any critical filesystem operations.

All actual piece availability checking is done via:
- `api_client.check_range_available()` which uses its own cached bitfield (`status_bitfield_cache` in `RqbitClient`)
- Direct API calls in the async worker (lines 256-258 in async_bridge.rs)

#### Non-Critical Functionality (Would Be Lost)

1. **Stalled Torrent Detection** (lines 203-208)
   - Torrents that haven't made progress in `stalled_timeout` seconds won't be marked as "Stalled"
   - **Impact:** Minor - informational status only, doesn't affect reads

2. **Early EAGAIN in Read Operations** (lines 1203-1216)
   - Quick check to return EAGAIN if torrent not started
   - Without this, read would proceed to API and fail there instead
   - **Impact:** Minimal - changes error timing, not error outcome

3. **Torrent Completion Optimization** (lines 1220-1227)
   - Skip piece check for completed torrents (using `status.is_complete()`)
   - Without this, all reads would check pieces via API
   - **Impact:** Minor performance impact for completed torrents

4. **Extended Attribute Status** (lines 2005-2025)
   - `user.torrent.status` xattr would return ENOATTR or error
   - **Impact:** None - this is purely diagnostic

5. **Public Status API Methods** (lines 772-789)
   - External code calling these would get empty results
   - **Impact:** Unknown - not called internally

### Dependencies to Remove

If removing status monitoring, also remove:

1. **Field:** `torrent_statuses: Arc<DashMap<u64, TorrentStatus>>` (line 69)
2. **Field:** `monitor_handle: Arc<Mutex<Option<JoinHandle<()>>>>` (line 73)
3. **Methods:** 
   - `start_status_monitoring()` (lines 177-234)
   - `stop_status_monitoring()` (lines 237-244)
   - `get_torrent_status()` (line 772)
   - `monitor_torrent()` (line 777)
   - `unmonitor_torrent()` (line 783)
   - `list_torrent_statuses()` (line 789)
4. **Type:** `TorrentStatus` struct (in `src/api/types.rs` line 247) - if unused elsewhere
5. **Related imports:** DashMap, TorrentStatus, TorrentState

### Code Simplification

Removing status monitoring would:
- Eliminate 1 background task (out of 3 total)
- Remove ~70 lines of code from filesystem.rs
- Remove need for `TorrentStatus` and `TorrentState` types (if not used elsewhere)
- Simplify read operation logic (remove early EAGAIN checks)
- Reduce memory usage (no status cache)
- Remove dependency on `config.monitoring.status_poll_interval` and `stalled_timeout`

### Recommendation

**SAFE TO REMOVE** - The status monitoring task provides no critical functionality. All piece availability checking is done through the API client's separate bitfield cache. The monitoring task only provides:
- Informational status that isn't used for decisions
- Early-exit optimizations that could be handled elsewhere
- Diagnostic xattr that could be removed

Removing this feature would reduce complexity without impacting core read functionality.

## Related Configuration

- `monitoring.status_poll_interval` - How often to poll (default: 60s)
- `monitoring.stalled_timeout` - Threshold for marking stalled (default: 300s)

Both config options would become unnecessary if monitoring is removed.

## Testing Considerations

If removing status monitoring:
1. Test that read operations still work for incomplete torrents
2. Test that piece availability checking still functions via API client
3. Test that completed torrents still allow reads
4. Verify no performance degradation for common read patterns
