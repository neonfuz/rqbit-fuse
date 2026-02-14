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

**Endpoint:** `GET /torrents/{id}`

**Description:** Get detailed information about a specific torrent.

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

**Endpoint:** `GET /torrents/{id}/haves`

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

**Endpoint:** `GET /torrents/{id}/stream/{file_idx}`

**Headers:**
- `Range: bytes={start}-{end}` - Optional, for partial content

**Description:** Read data from a file within a torrent.

**Response:**
- Without Range header: Full file (200 OK)
- With Range header: Partial content (206 Partial Content)

**Response Headers (206):**
- `Content-Range: bytes {start}-{end}/{total}`
- `Content-Length: {size}`

**Behavior:**
1. Maps byte range to torrent pieces
2. Prioritizes downloading those pieces
3. Uses 32MB readahead buffer
4. Blocks until pieces are available
5. Returns data as it becomes available

**Usage:** Primary endpoint for FUSE read operations.

**No caching** - Always fresh data

---

### Get Torrent Statistics

**Endpoint:** `GET /torrents/{id}/stats/v1`

**Description:** Get detailed download statistics.

**Response:**
```json
{
  "file_count": 1,
  "files": [
    {
      "length": 5120000000,
      "included": true
    }
  ],
  "finished": false,
  "progress_bytes": 104857600,
  "progress_pct": 2.05,
  "total_bytes": 5120000000
}
```

**Usage:** Show download progress, check if file is complete.

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
- `POST /torrents/{id}/pause`
- `POST /torrents/{id}/start`

**Description:** Pause or resume torrent downloading.

**Usage:** Control download activity (optional for torrent-fuse).

---

### Delete/Forget Torrent

**Endpoints:**
- `POST /torrents/{id}/forget` - Remove from session, keep files
- `POST /torrents/{id}/delete` - Remove from session, delete files

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
