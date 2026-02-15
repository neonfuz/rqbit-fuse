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
│                  rqbit-fuse FUSE Client                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ FUSE Handler │  │ HTTP Client  │  │ Cache Mgr    │       │
│  │ (fuser)      │  │ (reqwest)    │  │ (in-mem)     │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│  ┌──────────────┐  ┌──────────────┐                          │
│  │ Metrics      │  │ Streaming    │                          │
│  │ Collection   │  │ Manager      │                          │
│  └──────────────┘  └──────────────┘                          │
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

### 1. FUSE Client (rqbit-fuse)

**Language:** Rust

**Responsibilities:**
- Mount/Unmount FUSE filesystem
- Handle FUSE callbacks (lookup, readdir, read, getattr, open, release, readlink)
- Communicate with rqbit via HTTP API
- Cache metadata to reduce API calls
- Manage concurrent reads with piece prioritization via HTTP Range requests
- Collect metrics on FUSE operations and API calls
- Background torrent discovery and status monitoring

**Key Modules:**

#### fs/filesystem.rs
- Implements FUSE filesystem callbacks
- Maps FUSE operations to torrent file operations
- Directory structure: One directory per torrent, containing torrent files
- Background tasks for torrent discovery and status monitoring

#### api/client.rs
- HTTP client for rqbit API with circuit breaker pattern
- Endpoints used:
  - `GET /torrents` - List all torrents
  - `GET /torrents/{id}` - Get torrent details
  - `GET /torrents/{id}/haves` - Get piece availability bitfield
  - `GET /torrents/{id}/stream/{file_idx}` - Read file data with Range support
  - `GET /torrents/{id}/stats/v1` - Get torrent statistics
  - `POST /torrents/{id}/pause` - Pause torrent
  - `POST /torrents/{id}/start` - Resume torrent
  - `POST /torrents/{id}/forget` - Remove torrent (keep files)
  - `POST /torrents/{id}/delete` - Remove torrent and delete files

#### api/streaming.rs
- PersistentStreamManager for efficient sequential reads
- Connection pooling and reuse for HTTP streaming
- Handles rqbit's behavior of returning 200 OK instead of 206 Partial Content

#### cache.rs
- Generic LRU cache with TTL support
- Caches metadata to reduce API calls
- Statistics tracking (hits, misses, evictions)

#### fs/inode.rs
- InodeManager for inode allocation and management
- Thread-safe concurrent access using DashMap
- Supports directories, files, and symlinks
- Path-to-inode and torrent-to-inode mappings

#### types/
- `torrent.rs` - Torrent metadata structures
- `file.rs` - TorrentFile structure
- `inode.rs` - InodeEntry enum (Directory, File, Symlink)
- `attr.rs` - File attribute helpers

#### metrics.rs
- FuseMetrics - Tracks FUSE operations (getattr, lookup, read, etc.)
- ApiMetrics - Tracks API calls, retries, circuit breaker state

#### main.rs (CLI)
- Commands:
  - `mount -m <mount-point>` - Start FUSE filesystem
  - `umount <mount-point>` - Unmount filesystem
  - `status [--format text|json]` - Show mount status and configuration

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
3. User mounts torrents: `rqbit-fuse mount -m /mnt/torrents`
4. Files appear as virtual files in mount point

## File Structure

```
rqbit-fuse/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── cache.rs             # Generic LRU cache with TTL
│   ├── metrics.rs           # Metrics collection
│   ├── config/
│   │   └── mod.rs           # Configuration management
│   ├── fs/                  # FUSE filesystem implementation
│   │   ├── mod.rs
│   │   ├── filesystem.rs    # FUSE callbacks
│   │   └── inode.rs         # Inode management
│   ├── api/                 # HTTP API client
│   │   ├── mod.rs
│   │   ├── client.rs        # HTTP client with retry logic
│   │   ├── types.rs         # API types and error mapping
│   │   └── streaming.rs     # Persistent streaming manager
│   └── types/               # Core type definitions
│       ├── mod.rs
│       ├── torrent.rs       # Torrent structures
│       ├── file.rs          # File structures
│       ├── inode.rs         # Inode entry types
│       └── attr.rs          # File attribute helpers
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

**Note:** Single-file torrents are added directly to root (not in subdirectory) as an optimization.

### Inode Management

- Root inode (1): `/mnt/torrents`
- All entries: Sequential starting from 2
- Uses `InodeManager` with concurrent access via DashMap

**InodeEntry Types:**
```rust
pub enum InodeEntry {
    Directory { ino, name, parent, children },
    File { ino, name, parent, torrent_id, file_index, size },
    Symlink { ino, name, parent, target },
}
```

**InodeManager Structure:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    path_to_inode: DashMap<String, u64>,
    torrent_to_inode: DashMap<u64, u64>,
}
```

### FUSE Callbacks

#### init()
- Validates mount point
- Checks root inode exists
- Starts background status monitoring task
- Starts background torrent discovery task
- Does NOT load torrents immediately (done by discovery task)

#### lookup(parent, name)
- Uses `inode_manager.lookup_by_path()`
- Builds path from parent and name
- Returns file attributes using `build_file_attr()`

#### readdir(inode, offset)
- Triggers torrent discovery when listing root
- Uses `inode_manager.get_children()` to get directory contents
- Handles `.` and `..` entries
- Supports symlinks in directory listings

#### read(inode, offset, size)
1. Get file info from inode manager
2. Determine which torrent and file index
3. Make HTTP request with persistent streaming:
   ```
   GET /torrents/{id}/stream/{file_idx}
   Range: bytes={offset}-{offset+size-1}
   ```
4. Uses `read_file_streaming()` with persistent connections
5. Implements read-ahead/prefetching for sequential reads
6. Handles piece availability checking (can return EAGAIN)
7. Return data to FUSE

#### getattr(inode)
- Returns file/directory attributes derived from InodeEntry
- Size from `InodeEntry::File { size, .. }`
- Mode: 0444 for files (read-only), 0555 for directories

#### open() and release()
- File open with permission checks
- File handle cleanup

#### readlink()
- Symlink resolution for InodeEntry::Symlink

## HTTP Range Request Strategy

When FUSE requests data at offset X of size Y:

```rust
// Calculate byte range
let start = offset;
let end = (offset + size - 1).min(file_size - 1);

// Make HTTP request with persistent streaming
let data = client
    .read_file_streaming(torrent_id, file_idx, offset, size)
    .await?;

// Implementation details:
// - Uses PersistentStreamManager for connection reuse
// - Handles rqbit returning 200 OK instead of 206 Partial Content
// - Clamps read size to 64KB (FUSE_MAX_READ)
// - Implements read-ahead/prefetching for sequential reads
```

## Caching Strategy

### Generic Cache Implementation

The cache is a generic LRU cache with TTL support:

```rust
pub struct Cache<K, V> {
    entries: DashMap<K, Arc<CacheEntry<V>>>,
    max_entries: usize,
    lru_counter: AtomicU64,
}

pub struct CacheEntry<T> {
    value: T,
    created_at: Instant,
    sequence: u64,
}
```

### Metadata Cache TTLs (Configurable)
- **Torrent list**: 30 second TTL (default)
- **Torrent details**: 60 second TTL (default)
- **Piece bitfields**: 5 second TTL (default)

### Cache Features
- LRU eviction when capacity reached
- TTL-based expiration
- Statistics tracking (hits, misses, evictions, expired)

## Error Handling

### FUSE Error Mapping

| Error Type | FUSE Error | Description |
|------------|------------|-------------|
| `TorrentNotFound` | `ENOENT` | Torrent not found |
| `FileNotFound` | `ENOENT` | File not found |
| `InvalidRange` | `EINVAL` | Invalid byte range |
| `ConnectionTimeout` | `EAGAIN` | Connection timeout |
| `ReadTimeout` | `EAGAIN` | Read timeout |
| `ServerDisconnected` | `ENOTCONN` | Server disconnected |
| `NetworkError` | `ENETUNREACH` | Network unreachable |
| `CircuitBreakerOpen` | `EAGAIN` | Circuit breaker open |
| HTTP 404 | `ENOENT` | Not found |
| HTTP 403 | `EACCES` | Permission denied |
| HTTP 503 | `EAGAIN` | Service unavailable |

### Retry and Circuit Breaker
- Exponential backoff with configurable max retries
- Circuit breaker pattern for rqbit unavailability
- Transient error detection for retry logic

### Piece Availability
- Configurable piece checking with `piece_check_enabled`
- Can return EAGAIN for unavailable pieces if `return_eagain_for_unavailable` is true
- Otherwise blocks until data is available (with timeout)

## Configuration

### Config File Locations (in order of priority)
1. `~/.config/rqbit-fuse/config.toml`
2. `/etc/rqbit-fuse/config.toml`
3. `./rqbit-fuse.toml`

### Config File Example

```toml
[api]
url = "http://127.0.0.1:3030"  # rqbit HTTP API endpoint

[cache]
metadata_ttl = 60        # seconds
torrent_list_ttl = 30
piece_ttl = 5
max_entries = 1000

[mount]
mount_point = "/mnt/torrents"
allow_other = false
auto_unmount = true

[performance]
read_timeout = 30              # seconds to wait for data
max_concurrent_reads = 10      # concurrent HTTP requests
readahead_size = 33554432      # 32MB, match rqbit's default
piece_check_enabled = true     # Check piece availability
return_eagain_for_unavailable = false  # Return EAGAIN for unavailable pieces

[monitoring]
status_poll_interval = 5       # seconds
stalled_timeout = 300          # seconds

[logging]
level = "info"
log_fuse_operations = true
log_api_calls = true
metrics_enabled = true
metrics_interval_secs = 60
```

### Environment Variables

All config options can be overridden via environment variables:
- `TORRENT_FUSE_API_URL`
- `TORRENT_FUSE_MOUNT_POINT`
- `TORRENT_FUSE_METADATA_TTL`
- `TORRENT_FUSE_TORRENT_LIST_TTL`
- `TORRENT_FUSE_PIECE_TTL`
- `TORRENT_FUSE_MAX_ENTRIES`
- `TORRENT_FUSE_READ_TIMEOUT`
- `TORRENT_FUSE_MAX_CONCURRENT_READS`
- `TORRENT_FUSE_READAHEAD_SIZE`
- `TORRENT_FUSE_ALLOW_OTHER`
- `TORRENT_FUSE_AUTO_UNMOUNT`
- `TORRENT_FUSE_STATUS_POLL_INTERVAL`
- `TORRENT_FUSE_STALLED_TIMEOUT`
- `TORRENT_FUSE_PIECE_CHECK_ENABLED`
- `TORRENT_FUSE_RETURN_EAGAIN`
- `TORRENT_FUSE_LOG_LEVEL`
- `TORRENT_FUSE_LOG_FUSE_OPS`
- `TORRENT_FUSE_LOG_API_CALLS`
- `TORRENT_FUSE_METRICS_ENABLED`
- `TORRENT_FUSE_METRICS_INTERVAL`

## CLI Interface

```bash
# Start FUSE filesystem
rqbit-fuse mount -m /mnt/torrents
rqbit-fuse mount -m /mnt/torrents --api-url http://localhost:3030

# Unmount
rqbit-fuse umount /mnt/torrents
rqbit-fuse umount /mnt/torrents --force

# Show status
rqbit-fuse status
rqbit-fuse status --format json
```

### Mount Options
```
Options:
  -m, --mount-point <PATH>    Mount point (env: TORRENT_FUSE_MOUNT_POINT)
  -u, --api-url <URL>         API URL (env: TORRENT_FUSE_API_URL)
  -c, --config <FILE>         Config file path
  -v, --verbose               Increase verbosity (repeatable)
  -q, --quiet                 Suppress output except errors
      --allow-other           Allow other users (env: TORRENT_FUSE_ALLOW_OTHER)
      --auto-unmount          Auto-unmount on exit (env: TORRENT_FUSE_AUTO_UNMOUNT)
```

## Dependencies

### Core
- `fuser` - FUSE implementation
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `serde` - Serialization
- `clap` - CLI parsing
- `anyhow` - Error handling

### Additional
- `futures` - Async stream handling
- `serde_json` - JSON serialization
- `thiserror` - Error type definitions
- `bytes` - Byte handling
- `tracing-subscriber` - Logging implementation
- `toml` - Config file parsing
- `dirs` - Config directory detection
- `libc` - FUSE error codes
- `dashmap` - Concurrent hash map

## Security Considerations

1. **Read-only**: Filesystem is read-only, cannot modify torrents
2. **No execute**: Files are not executable (mode 0444)
3. **Local only**: API connection to localhost only by default
4. **User permissions**: Run as regular user, not root
5. **Circuit breaker**: Prevents cascading failures when rqbit is unavailable

## Performance Considerations

1. **HTTP overhead**: Each read = 1 HTTP request
   - Mitigate with persistent connections (PersistentStreamManager)
   - Sequential reads use connection reuse
   
2. **Concurrent reads**: 
   - Limit concurrent HTTP requests with semaphore
   - Use connection pooling

3. **Metadata caching**: 
   - Cache directory listings with TTL
   - Cache file attributes
   - LRU eviction prevents unbounded growth

4. **Piece prioritization**: 
   - Rely on rqbit's 32MB readahead
   - Sequential reads get priority automatically
   - Read-ahead detection for prefetching

5. **Background tasks**:
   - Torrent discovery with configurable poll interval
   - Status monitoring for download progress

## Future Enhancements

1. **Write support** (if rqbit supports it): Read-write filesystem
2. **Search**: Search across all torrent files
3. **Stats**: Bandwidth, download progress per file
4. **Multi-daemon**: Support multiple rqbit instances
5. **macOS support**: Use FUSE-T or macFUSE

## Implementation Status

### Implemented
- Basic FUSE mount/unmount
- Directory listing (torrents as dirs, files)
- File read via HTTP Range requests
- Metadata caching with TTL and LRU
- Background torrent discovery
- Status monitoring
- Circuit breaker pattern
- Metrics collection
- Persistent HTTP streaming
- Comprehensive error handling
- Configuration file support
- CLI with status command

### Not Implemented
- `list` command (documented but not implemented)
- `cache clear` command (documented but not implemented)
- `daemon` command (documented but not implemented)
- Write support
- macOS support

Last updated: 2024-02-14
