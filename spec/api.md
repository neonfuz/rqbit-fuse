# API Documentation

## Overview

This document describes the rqbit HTTP API endpoints used by torrent-fuse and how we interact with them.

## Base URL

Default: `http://127.0.0.1:3030`

## Endpoints

### List All Torrents

**Endpoint:** `GET /torrents`

**Description:** Returns list of all torrents in the session.

**Response:**
```json
{
  "torrents": [
    {
      "id": 1,
      "info_hash": "aabbccdd...",
      "name": "Ubuntu 24.04 ISO",
      "output_folder": "/home/user/Downloads",
      "file_count": 1,
      "files": [
        {
          "name": "ubuntu-24.04.iso",
          "length": 5120000000,
          "components": ["ubuntu-24.04.iso"]
        }
      ]
    }
  ]
}
```

**Usage:** Build directory structure, get torrent IDs for operations.

**Cache TTL:** 30 seconds

---

### Get Torrent Details

**Endpoint:** `GET /torrents/{id_or_infohash}`

**Description:** Get detailed information about a specific torrent.

**Parameters:**
- `id_or_infohash` - Torrent ID (integer) or info hash (40-character hex string)

**Response:**
```json
{
  "id": 1,
  "info_hash": "aabbccdd...",
  "name": "Ubuntu 24.04 ISO",
  "output_folder": "/home/user/Downloads",
  "file_count": 1,
  "piece_length": 262144,
  "files": [
    {
      "name": "ubuntu-24.04.iso",
      "length": 5120000000,
      "components": ["ubuntu-24.04.iso"]
    }
  ]
}
```

**Usage:** Get file list, piece length, verify torrent exists.

**Cache TTL:** 60 seconds

---

### Get Piece Availability (Bitfield)

**Endpoint:** `GET /torrents/{id_or_infohash}/haves`

**Headers:**
- `Accept: application/octet-stream` - Binary bitfield (default)
- `Accept: image/svg+xml` - SVG visualization

**Description:** Returns which pieces have been downloaded.

**Binary Response:**
- Body: Bitfield where bit i = 1 if piece i is downloaded
- Header: `x-bitfield-len` - Total number of pieces

**Example:**
```rust
// Piece 0, 1, 3 downloaded (binary: 1011)
// Bytes: [0b00001011, ...]
```

**Usage:** 
- Check if file is fully downloaded
- Show download progress
- For debugging/statistics only (not required for reads)

**Cache TTL:** 5 seconds (updates frequently during download)

---

### Stream File (Read Data)

**Endpoint:** `GET /torrents/{id_or_infohash}/stream/{file_idx}`

**Parameters:**
- `id_or_infohash` - Torrent ID (integer) or info hash (40-character hex string)
- `file_idx` - Zero-based index of the file within the torrent

**Request Headers:**

| Header | Description |
|--------|-------------|
| `Range` | Optional. Standard HTTP Range header for seeking. Format: `bytes=start-[end]` |
| `transferMode.dlna.org` | Optional. Set to "Streaming" for DLNA streaming mode |
| `getcontentFeatures.dlna.org` | Optional. Set to "1" to request content features |

**Range Examples:**
```
# Request first 1000 bytes
Range: bytes=0-999

# Request from byte 1000 to end
Range: bytes=1000-

# Request specific range (recommended for FUSE reads)
Range: bytes=1048576-1179647
```

**Response:**
- Without Range header: Full file (200 OK)
- With Range header: Partial content (206 Partial Content)

**Response Headers:**

| Header | Description |
|--------|-------------|
| `Accept-Ranges` | Always set to "bytes" |
| `Content-Type` | MIME type of the file (determined from filename extension) |
| `Content-Length` | Size of the response body |
| `Content-Range` | Present when Range header is used (format: `bytes start-end/total`) |
| `transferMode.dlna.org` | Set to "Streaming" if requested |
| `contentFeatures.dlna.org` | DLNA content features if requested |

**Response Status Codes:**

| Status | Description |
|--------|-------------|
| `200 OK` | Full content being streamed |
| `206 Partial Content` | Partial content (when Range header used) |
| `404 Not Found` | Torrent or file not found |
| `416 Range Not Satisfiable` | Invalid range requested |

**Behavior:**
1. Maps byte range to torrent pieces
2. Prioritizes downloading those pieces
3. Uses 32MB readahead buffer
4. Blocks until pieces are available
5. Returns data as it becomes available

**Usage:** Primary endpoint for FUSE read operations.

**No caching** - Always fresh data

**Example URLs:**
```
http://127.0.0.1:3030/torrents/0/stream/0
http://127.0.0.1:3030/torrents/abc123.../stream/1
```

**Implementation Details:**

- **Buffer Size:** The HTTP streaming handler uses a 64KB buffer for reading from the file stream
- **Piece Prioritization:** 
  - Pieces from different active streams are interleaved for fair download
  - By default, 32MB ahead of current position is prioritized
  - Streams are woken when pieces they need become available
- **State Management:** Works across torrent states:
  - **Live:** Active downloading/seeding - pieces fetched on demand
  - **Paused:** Previously downloaded content served from disk

---

### Playlist API

Generate M3U8 playlists for media players.

#### Global Playlist (All Torrents)

**Endpoint:** `GET /torrents/playlist`

Returns an M3U8 playlist containing all playable (video/audio) files from all torrents.

#### Single Torrent Playlist

**Endpoint:** `GET /torrents/{id_or_infohash}/playlist`

Returns an M3U8 playlist for a specific torrent.

**Response Headers:**
- `Content-Type: application/mpegurl; charset=utf-8`
- `Content-Disposition: attachment; filename="rqbit-playlist.m3u8"`

---

### Get Torrent Statistics

**Endpoint:** `GET /torrents/{id_or_infohash}/stats/v1`

**Description:** Get detailed download statistics.

**Response (Live Torrent):**
```json
{
  "state": "live",
  "file_progress": [5702520832],
  "error": null,
  "progress_bytes": 5702520832,
  "uploaded_bytes": 872448,
  "total_bytes": 5702520832,
  "finished": true,
  "live": {
    "snapshot": {
      "downloaded_and_checked_bytes": 5702520832,
      "downloaded_and_checked_pieces": 21754,
      "fetched_bytes": 6735450112,
      "uploaded_bytes": 872448,
      "peer_stats": {
        "queued": 0,
        "connecting": 0,
        "live": 0,
        "seen": 12999,
        "dead": 0,
        "not_needed": 12999,
        "steals": 5694
      },
      "total_piece_download_ms": 38607086
    },
    "average_piece_download_time": {
      "secs": 1,
      "nanos": 774712053
    },
    "download_speed": {
      "mbps": 0.0,
      "human_readable": "0.00 MiB/s"
    },
    "upload_speed": {
      "mbps": 0.0,
      "human_readable": "0.00 MiB/s"
    },
    "time_remaining": null
  }
}
```

**Response (Error State):**
```json
{
  "state": "error",
  "file_progress": [],
  "error": "error writing to file...\n\nCaused by:\n    No space left on device (os error 28)",
  "progress_bytes": 0,
  "uploaded_bytes": 0,
  "total_bytes": 2969567232,
  "finished": false,
  "live": null
}
```

**Fields:**
- `state`: Torrent state ("live", "paused", "error")
- `file_progress`: Array of bytes downloaded per file
- `error`: Error message if in error state, null otherwise
- `progress_bytes`: Total bytes downloaded
- `uploaded_bytes`: Total bytes uploaded
- `total_bytes`: Total torrent size
- `finished`: Whether the torrent is complete
  - `live`: Live statistics (null if torrent is in error state)
    - `snapshot`: Detailed download progress information (does not include total_bytes)
    - `download_speed`: Current download speed
    - `upload_speed`: Current upload speed
    - `average_piece_download_time`: Average time to download a piece
    - `time_remaining`: Estimated time remaining

**Usage:** Show download progress, check if file is complete, monitor errors.

**Note:** The `live` field is null when the torrent is in an error state. Always check for null before accessing nested fields.

**Cache TTL:** 10 seconds

---

### Add Torrent

**Endpoint:** `POST /torrents`

**Content-Type:** Depends on method

**Methods:**

#### Magnet Link
```json
{
  "magnet_link": "magnet:?xt=urn:btih:..."
}
```

#### Torrent File URL
```json
{
  "torrent_link": "http://example.com/file.torrent"
}
```

#### Torrent File Upload
```
Content-Type: multipart/form-data
Body: torrent file bytes
```

**Response:**
```json
{
  "id": 1,
  "info_hash": "aabbccdd..."
}
```

**Usage:** Add torrents to rqbit (typically done via rqbit CLI, not torrent-fuse).

---

### Pause/Start Torrent

**Endpoints:**
- `POST /torrents/{id_or_infohash}/pause`
- `POST /torrents/{id_or_infohash}/start`

**Description:** Pause or resume torrent downloading.

**Usage:** Control download activity (optional for torrent-fuse).

---

### Delete/Forget Torrent

**Endpoints:**
- `POST /torrents/{id_or_infohash}/forget` - Remove from session, keep files
- `POST /torrents/{id_or_infohash}/delete` - Remove from session, delete files

**Usage:** Cleanup (optional for torrent-fuse).

---

## Error Responses

All endpoints return standard HTTP status codes:

- `200 OK` - Success
- `206 Partial Content` - Successful range request
- `400 Bad Request` - Invalid parameters
- `404 Not Found` - Torrent or file not found
- `416 Range Not Satisfiable` - Invalid byte range
- `500 Internal Server Error` - Server error

**Error Response Body:**
```json
{
  "error": "Error message"
}
```

## Rate Limiting

rqbit does not implement rate limiting, but we should be respectful:

- Limit concurrent requests to `/stream` endpoint
- Reuse HTTP connections (connection pooling)
- Cache metadata appropriately

## FUSE Read Flow

```
FUSE read(offset=1048576, size=131072)
    │
    ▼
Calculate file_idx and byte range
    │
    ▼
GET /torrents/{id}/stream/{file_idx}
Range: bytes=1048576-1179647
    │
    ▼
rqbit receives request
    │
    ├── Maps bytes to pieces (piece 4-5)
    ├── Prioritizes pieces for download
    ├── Downloads with 32MB readahead
    └── Blocks until pieces available
    │
    ▼
Return data (131072 bytes)
    │
    ▼
FUSE returns to kernel
```

## Implementation Notes

### HTTP Client Configuration

```rust
use reqwest::Client;

let client = Client::builder()
    .timeout(Duration::from_secs(60))  // Long timeout for streaming
    .pool_max_idle_per_host(10)         // Connection pooling
    .build()?;
```

### Range Request Format

Always use inclusive byte ranges:
```
Range: bytes=0-1023      # First 1024 bytes
Range: bytes=1024-2047   # Second 1024 bytes
```

### Handling 206 Responses

```rust
let response = client.get(url)
    .header("Range", format!("bytes={}-{}", start, end))
    .send()
    .await?;

if response.status() == StatusCode::PARTIAL_CONTENT {
    let bytes = response.bytes().await?;
    // Return to FUSE
}
```

### Concurrent Reads

Use semaphore to limit concurrent requests:

```rust
use tokio::sync::Semaphore;

static CONCURRENT_READS: Semaphore = Semaphore::const_new(10);

async fn read_file(...) -> Result<Bytes> {
    let _permit = CONCURRENT_READS.acquire().await?;
    // Make HTTP request
}
```

## Usage Examples

### Stream to VLC

```bash
# Start streaming
vlc "http://127.0.0.1:3030/torrents/0/stream/0/movie.mp4"
```

### Use with curl

```bash
# Stream with range (seek to 1MB)
curl -H "Range: bytes=1048576-" http://127.0.0.1:3030/torrents/0/stream/0

# Get playlist
curl http://127.0.0.1:3030/torrents/playlist
```

### Download Playlist

```bash
curl -o playlist.m3u8 http://127.0.0.1:3030/torrents/0/playlist
```

## Related rqbit Source Files

- `crates/librqbit/src/http_api/handlers/streaming.rs` - HTTP handler implementation
- `crates/librqbit/src/http_api/handlers/playlist.rs` - Playlist generation
- `crates/librqbit/src/torrent_state/streaming.rs` - Core streaming logic
- `crates/librqbit/src/api.rs` - Public API methods
