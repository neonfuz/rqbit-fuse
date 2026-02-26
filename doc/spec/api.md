# API Documentation

## Overview

This document describes the rqbit HTTP API endpoints used by rqbit-fuse and how we interact with them.

## Base URL

Default: `http://127.0.0.1:3030`

## Endpoints

### List All Torrents

**Endpoint:** `GET /torrents`

**Description:** Returns list of all torrents in the session with basic information only. Does not include file lists.

**Response:**
```json
{
  "torrents": [
    {
      "id": 1,
      "info_hash": "aabbccdd...",
      "name": "Ubuntu 24.04 ISO",
      "output_folder": "/home/user/Downloads"
    }
  ]
}
```

**Fields:**
- `id`: Torrent ID (integer)
- `info_hash`: Info hash (40-character hex string)
- `name`: Torrent name
- `output_folder`: Download output directory

**Usage:** Get torrent IDs and basic metadata. Use `GET /torrents/{id}` for full details including files.

**Cache TTL:** 30 seconds

---

### Get Torrent Details

**Endpoint:** `GET /torrents/{id_or_infohash}`

**Description:** Get detailed information about a specific torrent, including file list and piece length.

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

**Cache TTL:** Not cached (always fetches fresh data)

---

### Get Piece Availability (Bitfield)

**Endpoint:** `GET /torrents/{id_or_infohash}/haves`

**Headers:**
- `Accept: application/octet-stream` - Binary bitfield (required)

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
- Used for piece availability checking (synchronous, no caching)

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
| `Authorization` | Optional. HTTP Basic Auth header if authentication is enabled |

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

**Implementation Notes:**
- Uses `PersistentStreamManager` for connection reuse
- Handles rqbit bug: may return 200 OK instead of 206 Partial Content
- Implements persistent streaming for efficient sequential reads

**Example URLs:**
```
http://127.0.0.1:3030/torrents/0/stream/0
http://127.0.0.1:3030/torrents/abc123.../stream/1
```

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

**Usage:** Show download progress, check if file is complete, monitor errors, detect stalled downloads.

**Note:** The `live` field is null when the torrent is in an error state. Always check for null before accessing nested fields.

**Cache TTL:** 10 seconds (used by background monitoring)

---

### Add Torrent

**Endpoint:** `POST /torrents`

**Content-Type:** `application/json`

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

**Response:**
```json
{
  "id": 1,
  "info_hash": "aabbccdd..."
}
```

**Usage:** Add torrents to rqbit (typically done via rqbit CLI, not rqbit-fuse).

---

### Pause/Start Torrent

**Endpoints:**
- `POST /torrents/{id_or_infohash}/pause`
- `POST /torrents/{id_or_infohash}/start`

**Description:** Pause or resume torrent downloading.

**Usage:** Control download activity (optional for rqbit-fuse).

---

### Delete/Forget Torrent

**Endpoints:**
- `POST /torrents/{id_or_infohash}/forget` - Remove from session, keep files
- `POST /torrents/{id_or_infohash}/delete` - Remove from session, delete files

**Usage:** Cleanup (optional for rqbit-fuse).

---

## Error Responses

All endpoints return standard HTTP status codes:

| Status Code | Description | FUSE Error |
|-------------|-------------|------------|
| `200 OK` | Success | - |
| `206 Partial Content` | Successful range request | - |
| `400 Bad Request` | Invalid parameters | `EINVAL` |
| `401 Unauthorized` | Authentication failed | `EACCES` |
| `403 Forbidden` | Permission denied | `EACCES` |
| `404 Not Found` | Torrent or file not found | `ENOENT` |
| `416 Range Not Satisfiable` | Invalid byte range | `EINVAL` |
| `500 Internal Server Error` | Server error | `EIO` |
| `503 Service Unavailable` | Server temporarily unavailable | `EAGAIN` |
| Timeout | Request timeout | `ETIMEDOUT` |
| Connection error | Network/transport failure | `ENETUNREACH` |

**Error Response Body:**
```json
{
  "error": "Error message"
}
```

### Error Types

The rqbit-fuse implementation uses the following error types:

| Error Type | HTTP Status | FUSE Error | Description |
|------------|-------------|------------|-------------|
| `NotFound` | 404 | `ENOENT` | Resource doesn't exist |
| `PermissionDenied` | 401, 403 | `EACCES` | Authentication/authorization failed |
| `InvalidArgument` | 400, 416 | `EINVAL` | Bad request parameters |
| `TimedOut` | - | `ETIMEDOUT` | Request timeout |
| `NetworkError` | - | `ENETUNREACH` | Connection/transport failure |
| `NotReady` | - | `EAGAIN` | Service not ready (e.g., retry limit exceeded) |
| `ApiError` | Various | Mapped by status | API returned error status |
| `IoError` | - | `EIO` | IO operation failed |

## Rate Limiting

rqbit does not implement rate limiting, but rqbit-fuse implements protective measures:

- **Connection pooling**: Reuses HTTP connections via `PersistentStreamManager`
- **Cache**: Caches torrent list to reduce API load
- **Retry logic**: Implements exponential backoff for transient failures

## FUSE Read Flow

```
FUSE read(offset=1048576, size=131072)
    |
    ▼
Calculate file_idx and byte range
    |
    ▼
GET /torrents/{id}/stream/{file_idx}
Range: bytes=1048576-1179647
    |
    ▼
PersistentStreamManager checks for existing connection
    |
    ├── Reuse connection if at correct offset
    └── Create new connection if needed
    |
    ▼
rqbit receives request
    |
    ├── Maps bytes to pieces (piece 4-5)
    ├── Prioritizes pieces for download
    ├── Downloads with 32MB readahead
    └── Blocks until pieces available
    |
    ▼
Return data (131072 bytes)
    |
    ▼
Update read position for connection reuse
    |
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

match response.status() {
    StatusCode::PARTIAL_CONTENT | StatusCode::OK => {
        // rqbit may return 200 OK even for range requests
        let bytes = response.bytes().await?;
        // Return to FUSE
    }
    _ => {
        // Handle error
    }
}
```

### Persistent Streaming

Uses `PersistentStreamManager` for efficient sequential access:

```rust
// Read via persistent stream manager for connection reuse
let data = client
    .read_file_streaming(torrent_id, file_idx, offset, size)
    .await?;
```

## Usage Examples

### Stream with curl

```bash
# Stream with range (seek to 1MB)
curl -H "Range: bytes=1048576-" http://127.0.0.1:3030/torrents/0/stream/0

# Get torrent list
curl http://127.0.0.1:3030/torrents

# Get torrent details
curl http://127.0.0.1:3030/torrents/0
```

## Related rqbit Source Files

- `crates/librqbit/src/http_api/handlers/streaming.rs` - HTTP handler implementation
- `crates/librqbit/src/torrent_state/streaming.rs` - Core streaming logic
- `crates/librqbit/src/api.rs` - Public API methods

Last updated: 2025-02-24
