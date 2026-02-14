# Torrent-Fuse Architecture Specification

## Overview

Torrent-Fuse is a read-only FUSE filesystem that mounts BitTorrent torrents as virtual directories. It uses [rqbit](https://github.com/ikatson/rqbit) as the torrent client daemon and communicates via its HTTP API.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     User Filesystem                          │
│  /mnt/torrents/                                              │
│  ├── ubuntu-24.04.iso/                                       │
│  │   └── ubuntu-24.04.iso (1 file torrent)                  │
│  └── linux-distro-collection/                                │
│       ├── ubuntu-24.04.iso                                  │
│       ├── fedora-40.iso                                     │
│       └── archlinux-2024.iso                                │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  torrent-fuse FUSE Client                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ FUSE Handler │  │ HTTP Client  │  │ Cache Mgr    │       │
│  │ (fuser)      │  │ (reqwest)    │  │ (in-mem)     │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└─────────────────────────────────────────────────────────────┘
                              │
                        HTTP API
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    rqbit Server                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ BitTorrent   │  │ HTTP API     │  │ Piece Mgr    │       │
│  │ Protocol     │  │ (3030)       │  │ (lib)        │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└─────────────────────────────────────────────────────────────┘
```

## Components

### 1. FUSE Client (torrent-fuse)

**Language:** Rust

**Responsibilities:**
- Mount/Unmount FUSE filesystem
- Handle FUSE callbacks (lookup, readdir, read, getattr)
- Communicate with rqbit via HTTP API
- Cache metadata to reduce API calls
- Manage concurrent reads with piece prioritization via HTTP Range requests

**Key Modules:**

#### fuse_handler.rs
- Implements FUSE filesystem callbacks
- Maps FUSE operations to torrent file operations
- Directory structure: One directory per torrent, containing torrent files

#### api_client.rs
- HTTP client for rqbit API
- Endpoints used:
  - `GET /torrents` - List all torrents
  - `GET /torrents/{id}` - Get torrent details
  - `GET /torrents/{id}/haves` - Get piece availability bitfield
  - `GET /torrents/{id}/stream/{file_idx}` - Read file data with Range support

#### cache.rs
- In-memory metadata cache
- Torrent info (name, files, sizes)
- Piece availability bitfields (TTL: 5 seconds)
- Directory listings

#### main.rs (CLI)
- Commands:
  - `mount <mount-point>` - Start FUSE filesystem
  - `unmount <mount-point>` - Unmount filesystem
  - `status` - Show mounted filesystems and active torrents

### 2. rqbit Server (External Dependency)

**Responsibilities:**
- Run as separate daemon
- Manage torrent downloads
- Expose HTTP API on port 3030
- Handle BitTorrent protocol, DHT, peer connections
- Piece prioritization for streaming (32MB readahead)

**User Workflow:**
1. User starts rqbit: `rqbit server start`
2. User adds torrents via rqbit CLI or API
3. User mounts torrents: `torrent-fuse mount /mnt/torrents`
4. Files appear as virtual files in mount point

## File Structure

```
torrent-fuse/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── fuse/
│   │   ├── mod.rs           # FUSE module exports
│   │   ├── filesystem.rs    # FUSE filesystem implementation
│   │   └── inode.rs         # Inode management
│   ├── api/
│   │   ├── mod.rs           # API module exports
│   │   ├── client.rs        # HTTP client for rqbit
│   │   └── types.rs         # API response types
│   ├── cache/
│   │   ├── mod.rs
│   │   └── metadata.rs      # Metadata caching
│   └── error.rs             # Error types
└── spec/
    ├── architecture.md      # This file
    └── api.md               # API endpoint documentation
```

## FUSE Implementation Details

### Directory Layout

Each torrent is mounted as a directory:

```
/mnt/torrents/
├── {torrent-name-1}/           # Directory per torrent
│   ├── file1.iso
│   └── file2.txt
└── {torrent-name-2}/
    └── video.mkv
```

### Inode Management

- Root inode (1): `/mnt/torrents`
- Torrent directories: Sequential starting from 2
- Files: Sequential after torrent directories

**Inode Mapping:**
```rust
struct InodeTable {
    next_inode: u64,
    torrents: HashMap<u64, TorrentInode>,  // inode -> torrent
    files: HashMap<u64, FileInode>,        // inode -> file
}
```

### FUSE Callbacks

#### lookup(parent, name)
- If parent is root: Find torrent by name
- If parent is torrent: Find file by name
- Return file attributes (size, mode, timestamps)

#### readdir(inode, offset)
- If root: List all torrent directories
- If torrent: List all files in torrent

#### read(inode, offset, size)
1. Get file info from cache
2. Determine which torrent and file index
3. Make HTTP Range request to rqbit:
   ```
   GET /torrents/{id}/stream/{file_idx}
   Range: bytes={offset}-{offset+size-1}
   ```
4. Return data to FUSE
5. rqbit automatically:
   - Prioritizes pieces needed for this range
   - Downloads with 32MB readahead
   - Blocks until data is available

#### getattr(inode)
- Return file/directory attributes
- Size from torrent metadata
- Mode: 0555 (read-only, executable for dirs)
- Timestamps: Use torrent add time or current time

## HTTP Range Request Strategy

When FUSE requests data at offset X of size Y:

```rust
// Calculate byte range
let start = offset;
let end = (offset + size - 1).min(file_size - 1);

// Make HTTP request
let response = client
    .get(&format!("{}/torrents/{}/stream/{}", api_url, torrent_id, file_idx))
    .header("Range", format!("bytes={}-{}"), start, end)
    .send()
    .await?;

// rqbit handles:
// - Mapping byte range to pieces
// - Prioritizing those pieces
// - Downloading with readahead
// - Blocking until available
```

## Caching Strategy

### Metadata Cache
- **Torrent list**: 30 second TTL
- **Torrent details**: 60 second TTL
- **Piece bitfields**: 5 second TTL (updates frequently during download)

### Cache Invalidation
- On lookup miss, fetch from API
- Background refresh for active reads
- Manual cache clear via CLI

## Error Handling

### FUSE Errors
- `ENOENT` - File/directory not found
- `EIO` - I/O error (API unavailable, network error)
- `EACCES` - Permission denied (read-only filesystem)

### HTTP Errors
- Retry with exponential backoff (max 3 retries)
- Circuit breaker pattern for rqbit unavailability
- Return EIO to FUSE after retries exhausted

### Piece Availability
- If pieces not available: rqbit blocks until downloaded
- Configurable timeout for reads (default: 30 seconds)
- User sees slow read, not error

## Configuration

### Config File (`~/.config/torrent-fuse/config.toml`)

```toml
[api]
url = "http://127.0.0.1:3030"  # rqbit HTTP API endpoint

[cache]
metadata_ttl = 60   # seconds
torrent_list_ttl = 30
piece_ttl = 5
max_entries = 1000

[mount]
default_mount_point = "/mnt/torrents"
allow_other = false
auto_unmount = true

[performance]
read_timeout = 30          # seconds to wait for data
max_concurrent_reads = 10  # concurrent HTTP requests
readahead_size = 33554432  # 32MB, match rqbit's default
```

## CLI Interface

```bash
# Start FUSE filesystem
torrent-fuse mount /mnt/torrents

# Mount with options
torrent-fuse mount /mnt/torrents --api-url http://localhost:3030 --read-timeout 60

# Unmount
torrent-fuse unmount /mnt/torrents

# Show status
torrent-fuse status

# Show active torrents and their mount status
torrent-fuse list

# Clear cache
torrent-fuse cache clear

# Daemon mode (auto-mount on startup)
torrent-fuse daemon --mount-point /mnt/torrents
```

## Dependencies

### Core
- `fuser` - FUSE implementation
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `serde` - Serialization
- `clap` - CLI parsing
- `anyhow` - Error handling

### Optional
- `tracing` - Logging
- `config` - Configuration management
- `dashmap` - Concurrent hash map for cache

## Security Considerations

1. **Read-only**: Filesystem is read-only, cannot modify torrents
2. **No execute**: Files are not executable (mode 0444)
3. **Local only**: API connection to localhost only by default
4. **User permissions**: Run as regular user, not root

## Performance Considerations

1. **HTTP overhead**: Each read = 1 HTTP request
   - Mitigate with larger read buffers in kernel
   - Sequential reads are batched by rqbit
   
2. **Concurrent reads**: 
   - Limit concurrent HTTP requests
   - Use connection pooling

3. **Metadata caching**: 
   - Cache directory listings
   - Cache file attributes

4. **Piece prioritization**: 
   - Rely on rqbit's 32MB readahead
   - Sequential reads get priority automatically

## Future Enhancements

1. **Write support** (if rqbit supports it): Read-write filesystem
2. **Symlinks**: Link to specific files within torrents
3. **Search**: Search across all torrent files
4. **Stats**: Bandwidth, download progress per file
5. **Multi-daemon**: Support multiple rqbit instances
6. **macOS support**: Use FUSE-T or macFUSE

## Implementation Phases

### Phase 1: MVP
- Basic FUSE mount/unmount
- Directory listing (torrents as dirs, files)
- File read via HTTP Range requests
- Basic caching

### Phase 2: Polish
- Better error handling
- Configuration file support
- Status/diagnostic commands
- Performance optimizations

### Phase 3: Advanced
- macOS support
- Background refresh
- Stats/monitoring
- Write support (if feasible)
