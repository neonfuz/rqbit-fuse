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
│  ┌──────────────┐                                           │
│  │ Streaming    │                                           │
│  │ Manager      │                                           │
│  └──────────────┘                                           │
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
- Bridge sync FUSE callbacks to async operations via AsyncFuseWorker

**Key Modules:**

#### fs/filesystem.rs
- Implements FUSE filesystem callbacks
- Maps FUSE operations to torrent file operations
- Directory structure: One directory per torrent, containing torrent files

#### fs/async_bridge.rs
- AsyncFuseWorker bridges sync FUSE callbacks to async operations
- Uses channel-based request/response pattern
- Prevents deadlocks from block_in_place + block_on patterns
- Handles read operations and torrent removal through async worker

#### fs/inode_manager.rs
- InodeManager for inode allocation and management
- Thread-safe concurrent access using DashMap
- Supports directories, files, and symlinks
- Path-to-inode and torrent-to-inode mappings
- Configurable maximum inode limit

#### fs/inode_entry.rs
- InodeEntry enum definition (Directory, File, Symlink)
- Serialize/deserialize support for inode entries
- Helper methods for inode operations

#### fs/inode.rs
- Re-exports from inode_manager and inode_entry for backward compatibility

#### api/client.rs
- HTTP client for rqbit API with retry logic
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
- Simple in-memory caching for torrent list (30 second TTL)

#### api/streaming.rs
- PersistentStreamManager for efficient sequential reads
- Connection pooling and reuse for HTTP streaming
- Handles rqbit's behavior of returning 200 OK instead of 206 Partial Content

#### api/types.rs
- API response types (TorrentInfo, FileInfo, TorrentStats, etc.)
- PieceBitfield for tracking piece availability
- ListTorrentsResult with partial failure handling

#### error.rs
- RqbitFuseError enum for unified error handling
- Error to FUSE errno mapping via to_errno()
- Conversion implementations from std::io::Error, reqwest::Error, etc.

#### types/
- `mod.rs` - Module exports and re-exports
- `attr.rs` - File attribute helpers
- `handle.rs` - File handle types and FileHandleManager

#### main.rs (CLI)
- Commands:
  - `mount -m <mount-point>` - Start FUSE filesystem
  - `umount <mount-point>` - Unmount filesystem

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
│   ├── mount.rs             # Mount point utilities
│   ├── error.rs             # Unified error types (RqbitFuseError)
│   ├── metrics.rs           # Metrics collection
│   ├── config/
│   │   └── mod.rs           # Configuration management
│   ├── fs/                  # FUSE filesystem implementation
│   │   ├── mod.rs           # Module exports
│   │   ├── filesystem.rs    # FUSE callbacks (TorrentFS)
│   │   ├── inode.rs         # Re-exports for backward compatibility
│   │   ├── inode_manager.rs # InodeManager implementation
│   │   ├── inode_entry.rs   # InodeEntry enum definition
│   │   └── async_bridge.rs  # Async/sync bridge for FUSE
│   ├── api/                 # HTTP API client
│   │   ├── mod.rs           # Module exports
│   │   ├── client.rs        # HTTP client with retry logic
│   │   ├── types.rs         # API types and structures
│   │   └── streaming.rs     # Persistent streaming manager
│   └── types/               # Core type definitions
│       ├── mod.rs           # Module exports
│       ├── attr.rs          # File attribute helpers
│       └── handle.rs        # File handle types
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

**Note:** Single-file torrents are also placed in a subdirectory (consistent with multi-file torrents).

### Inode Management

- Root inode (1): `/mnt/torrents`
- All entries: Sequential starting from 2
- Uses `InodeManager` with concurrent access via DashMap
- Configurable maximum inode limit (0 = unlimited)

**InodeEntry Types:**
```rust
pub enum InodeEntry {
    Directory { 
        ino: u64, 
        name: String, 
        parent: u64, 
        children: DashSet<u64>,
        canonical_path: String 
    },
    File { 
        ino: u64, 
        name: String, 
        parent: u64, 
        torrent_id: u64, 
        file_index: u64, 
        size: u64,
        canonical_path: String 
    },
    Symlink { 
        ino: u64, 
        name: String, 
        parent: u64, 
        target: String,
        canonical_path: String 
    },
}
```

**InodeManager Structure:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    path_to_inode: DashMap<String, u64>,
    torrent_to_inode: DashMap<u64, u64>,
    max_inodes: usize,  // 0 = unlimited
}
```

### FUSE Callbacks

#### init()
- Validates mount point
- Checks root inode exists
- Does NOT load torrents immediately

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
6. Return data to FUSE

#### getattr(inode)
- Returns file/directory attributes derived from InodeEntry
- Size from `InodeEntry::File { size, .. }`
- Mode: 0444 for files (read-only), 0555 for directories

#### open() and release()
- File open with permission checks
- File handle cleanup via FileHandleManager

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

### Torrent List Cache

The API client maintains a simple in-memory cache for the torrent list:

```rust
pub struct RqbitClient {
    // ... other fields ...
    list_torrents_cache: Arc<RwLock<Option<(Instant, ListTorrentsResult)>>>,
    list_torrents_cache_ttl: Duration,  // Default: 30 seconds
}
```

### Cache Features
- TTL-based expiration (30 seconds for torrent list)
- Manual invalidation on torrent add/remove operations
- Cache hit/miss metrics tracking

### Metadata Cache TTLs
- **Torrent list**: 30 second TTL (configurable via Config)

## Error Handling

### RqbitFuseError Types

| Error Type | Description |
|------------|-------------|
| `NotFound` | Entity not found (torrent, file, etc.) |
| `PermissionDenied` | Permission denied |
| `TimedOut` | Operation timed out |
| `NetworkError` | Server disconnected or network error |
| `ApiError { status, message }` | API returned error with HTTP status |
| `IoError` | General I/O error |
| `InvalidArgument` | Invalid argument (e.g., invalid byte range) |
| `ValidationError` | Configuration validation failed |
| `NotReady` | Resource temporarily unavailable |
| `ParseError` | Parse/serialization error |
| `IsDirectory` | Is a directory |
| `NotDirectory` | Not a directory |

### FUSE Error Mapping

| Error Type | FUSE Error | Description |
|------------|------------|-------------|
| `NotFound` | `ENOENT` | Not found |
| `PermissionDenied` | `EACCES` | Permission denied |
| `TimedOut` | `ETIMEDOUT` | Operation timed out |
| `NetworkError` | `ENETUNREACH` | Network unreachable |
| `NotReady` | `EAGAIN` | Resource temporarily unavailable |
| `InvalidArgument` | `EINVAL` | Invalid argument |
| `IoError` | `EIO` | I/O error |
| `IsDirectory` | `EISDIR` | Is a directory |
| `NotDirectory` | `ENOTDIR` | Not a directory |
| HTTP 400/416 | `EINVAL` | Bad request / Range not satisfiable |
| HTTP 401/403 | `EACCES` | Unauthorized / Forbidden |
| HTTP 404 | `ENOENT` | Not found |
| HTTP 408/423/429/503/504 | `EAGAIN` | Various retryable errors |
| HTTP 409 | `EEXIST` | Conflict |
| HTTP 413 | `EFBIG` | Entity too large |
| HTTP 500/502 | `EIO` | Server error |

### Retry Logic
- Exponential backoff with configurable max retries (default: 3)
- Transient error detection for retry logic
- Configurable retry delay (default: 500ms)

## Configuration

### Config File Locations (in order of priority)
1. `~/.config/rqbit-fuse/config.toml`
2. `/etc/rqbit-fuse/config.toml`
3. `./rqbit-fuse.toml`

### Config File Example

```toml
[api]
url = "http://127.0.0.1:3030"  # rqbit HTTP API endpoint
# username = "admin"             # Optional: HTTP Basic Auth username
# password = "secret"            # Optional: HTTP Basic Auth password

[cache]
metadata_ttl = 60        # seconds
max_entries = 1000

[mount]
mount_point = "/mnt/torrents"

[performance]
read_timeout = 30              # seconds to wait for data
max_concurrent_reads = 10      # concurrent HTTP requests
readahead_size = 33554432      # 32MB, match rqbit's default

[logging]
level = "info"
```

### Environment Variables

All config options can be overridden via environment variables:
- `TORRENT_FUSE_API_URL` - rqbit HTTP API endpoint
- `TORRENT_FUSE_MOUNT_POINT` - Mount point path
- `TORRENT_FUSE_METADATA_TTL` - Cache TTL in seconds
- `TORRENT_FUSE_MAX_ENTRIES` - Maximum cache entries
- `TORRENT_FUSE_READ_TIMEOUT` - Read timeout in seconds
- `TORRENT_FUSE_LOG_LEVEL` - Log level (error, warn, info, debug, trace)
- `TORRENT_FUSE_AUTH_USERPASS` - HTTP Basic Auth (username:password)
- `TORRENT_FUSE_AUTH_USERNAME` - HTTP Basic Auth username
- `TORRENT_FUSE_AUTH_PASSWORD` - HTTP Basic Auth password

## CLI Interface

```bash
# Start FUSE filesystem
rqbit-fuse mount -m /mnt/torrents
rqbit-fuse mount -m /mnt/torrents --api-url http://localhost:3030

# Unmount
rqbit-fuse umount /mnt/torrents
rqbit-fuse umount /mnt/torrents --force
```

### Mount Options
```
Options:
  -m, --mount-point <PATH>    Mount point (env: TORRENT_FUSE_MOUNT_POINT)
  -u, --api-url <URL>         API URL (env: TORRENT_FUSE_API_URL)
  -c, --config <FILE>         Config file path
  --username <USERNAME>       API username for HTTP Basic Auth
  --password <PASSWORD>       API password for HTTP Basic Auth
  -v, --verbose               Increase verbosity (repeatable)
  -q, --quiet                 Suppress output except errors
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
- `tokio-util` - Async utilities including cancellation tokens

## Security Considerations

1. **Read-only**: Filesystem is read-only, cannot modify torrents
2. **No execute**: Files are not executable (mode 0444)
3. **Local only**: API connection to localhost only by default
4. **User permissions**: Run as regular user, not root
5. **Retry with backoff**: Prevents cascading failures when rqbit is unavailable

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

4. **Piece prioritization**: 
   - Rely on rqbit's 32MB readahead
   - Sequential reads get priority automatically
   - Read-ahead detection for prefetching

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
- Simple metadata caching with TTL
- Background torrent discovery
- Persistent HTTP streaming
- Comprehensive error handling
- Configuration file support
- CLI with mount and umount commands
- AsyncFuseWorker for safe async/sync bridging
- RqbitFuseError types for proper error mapping
- Mount utilities module
- Signal handling for graceful shutdown
- File handle management with limits
- Inode management with configurable limits
- HTTP Basic Auth support

### Not Implemented
- `list` command (documented but not implemented)
- `cache clear` command (documented but not implemented)
- `daemon` command (documented but not implemented)
- Write support
- macOS support (FUSE-T or macFUSE)
- Generic LRU cache with LRU eviction
- Circuit breaker pattern

Last updated: 2026-02-24
