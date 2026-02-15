# Streaming API Specification

This document describes the HTTP streaming API for rqbit, which allows streaming torrent files with seeking support.

## Overview

rqbit can stream torrent files and smartly block the stream until the pieces are available. The pieces being streamed are prioritized, allowing you to seek and live stream videos. The streaming API supports HTTP Range headers for seeking, making it compatible with media players like VLC.

## Streaming URLs

### Stream a Single File

```
GET /torrents/{id_or_infohash}/stream/{file_idx}
GET /torrents/{id_or_infohash}/stream/{file_idx}/{filename}
```

**Parameters:**

- `id_or_infohash` - Torrent ID (integer) or info hash (40-character hex string)
- `file_idx` - Zero-based index of the file within the torrent
- `filename` (optional) - The filename for URL decoration (ignored but improves readability)

**Example URLs:**

```
http://127.0.0.1:3030/torrents/0/stream/0
http://127.0.0.1:3030/torrents/0/stream/0/movie.mp4
http://127.0.0.1:3030/torrents/abc123.../stream/1/audio.mp3
```

## Request Headers

### Range Header

Supports standard HTTP Range header for seeking:

```
Range: bytes=start-[end]
```

**Examples:**

```
# Request first 1000 bytes
Range: bytes=0-999

# Request from byte 1000 to end
Range: bytes=1000-
```

### DLNA Headers

For UPnP Media Server compatibility:

- `transferMode.dlna.org` - Set to "Streaming" for DLNA streaming mode
- `getcontentFeatures.dlna.org` - Set to "1" to request content features

## Response Headers

| Header                     | Description                                                         |
| -------------------------- | ------------------------------------------------------------------- |
| `Accept-Ranges`            | Always set to "bytes"                                               |
| `Content-Type`             | MIME type of the file (determined from filename extension)          |
| `Content-Length`           | Size of the response body                                           |
| `Content-Range`            | Present when Range header is used (format: `bytes start-end/total`) |
| `transferMode.dlna.org`    | Set to "Streaming" if requested                                     |
| `contentFeatures.dlna.org` | DLNA content features if requested                                  |

## Response Status Codes

| Status                      | Description                              |
| --------------------------- | ---------------------------------------- |
| `200 OK`                    | Full content being streamed              |
| `206 Partial Content`       | Partial content (when Range header used) |
| `404 Not Found`             | Torrent or file not found                |
| `416 Range Not Satisfiable` | Invalid range requested                  |

## Playlist API

Generate M3U8 playlists for media players:

### Global Playlist (All Torrents)

```
GET /torrents/playlist
```

Returns an M3U8 playlist containing all playable (video/audio) files from all torrents.

### Single Torrent Playlist

```
GET /torrents/{id_or_infohash}/playlist
```

Returns an M3U8 playlist for a specific torrent.

**Response Headers:**

- `Content-Type: application/mpegurl; charset=utf-8`
- `Content-Disposition: attachment; filename="rqbit-playlist.m3u8"`

## Implementation Details

### Core Streaming Components

**File:** `crates/librqbit/src/torrent_state/streaming.rs`

The streaming implementation uses:

- `FileStream` - An async stream that implements `AsyncRead` and `AsyncSeek`
- `TorrentStreams` - Manages multiple concurrent streams with piece prioritization

### Piece Prioritization

When streaming:

1. **Interleaving**: Pieces from different active streams are interleaved to ensure fair download
2. **Lookahead**: By default, 32MB ahead of the current position is prioritized
3. **Waking**: Streams are woken when pieces they need become available

### Buffer Size

The HTTP streaming handler uses a 64KB buffer for reading from the file stream:

```rust
let s = tokio_util::io::ReaderStream::with_capacity(stream, 65536);
```

### State Management

Streaming works across torrent states:

- **Live**: Active downloading/seeding - pieces are fetched on demand
- **Paused**: Previously downloaded content is served from disk

## Library API

To create a stream programmatically using the `librqbit` library:

```rust
use librqbit::ManagedTorrent;

async fn stream_file(torrent: Arc<ManagedTorrent>, file_id: usize) -> anyhow::Result<FileStream> {
    let stream = torrent.stream(file_id).await?;
    // FileStream implements AsyncRead + AsyncSeek
    Ok(stream)
}
```

The `FileStream` type:

- Implements `tokio::io::AsyncRead` and `AsyncSeek`
- Blocks until required pieces are available
- Automatically manages piece prioritization
- Can seek to arbitrary positions

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

## Related Files

- `crates/librqbit/src/http_api/handlers/streaming.rs` - HTTP handler implementation
- `crates/librqbit/src/http_api/handlers/playlist.rs` - Playlist generation
- `crates/librqbit/src/torrent_state/streaming.rs` - Core streaming logic
- `crates/librqbit/src/api.rs` - Public API methods
