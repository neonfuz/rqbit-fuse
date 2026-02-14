# rqbit-fuse

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

A read-only FUSE filesystem that mounts BitTorrent torrents as virtual directories, enabling seamless access to torrent content without waiting for full downloads.

Access your torrents through standard filesystem operations—stream videos while they download, copy files on-demand, or browse archives instantly. Powered by [rqbit](https://github.com/ikatson/rqbit) for the BitTorrent protocol.

## Features

- **🎬 Stream torrents as files** - Access torrent content through standard filesystem operations without waiting for full download
- **⬇️ On-demand downloading** - Files and pieces are downloaded only when accessed
- **📺 Video streaming** - Watch videos while they download with full seeking support
- **🚀 Read-ahead optimization** - Detects sequential reads and prefetches upcoming pieces (32MB default)
- **💾 Smart caching** - LRU cache with TTL for metadata and pieces, configurable size limits
- **🛡️ Resilient API client** - Circuit breaker pattern, exponential backoff, automatic retry logic
- **📊 Extended attributes** - Check torrent status via `user.torrent.status` xattr as JSON
- **🔒 Read-only filesystem** - Safe, secure access that cannot modify torrents (mode 0444/0555)
- **🔄 Torrent management** - Add via magnet/URL, monitor status, remove torrents
- **🔗 Symlink support** - Full symbolic link handling within torrents
- **🌍 Unicode support** - Handles filenames in any language (Chinese, Japanese, Russian, emoji)
- **📁 Large file support** - Full 64-bit file sizes (>4GB supported)
- **🔍 Path traversal protection** - Sanitizes filenames, prevents `..` attacks
- **⚡ Zero-byte file handling** - Properly handles empty files
- **🔧 Single-file torrents** - Files added directly to root instead of creating directories

## Prerequisites

1. **Rust** - Install via [rustup](https://rustup.rs/):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **FUSE** - Install FUSE development libraries:
   
   **Ubuntu/Debian:**
   ```bash
   sudo apt-get install libfuse-dev
   ```
   
   **Fedora/RHEL:**
   ```bash
   sudo dnf install fuse-devel
   ```
   
   **Arch Linux:**
   ```bash
   sudo pacman -S fuse2
   ```
   
   **macOS:**
   ```bash
   brew install macfuse
   ```

3. **rqbit** - Install and run rqbit server:
   ```bash
   cargo install rqbit
   rqbit server start
   ```

## Platform Support

| Platform | Support | Notes |
|----------|---------|-------|
| Linux    | Full    | Native FUSE support, all features available |
| macOS    | Full    | Requires macFUSE, all features available |
| Windows  | None    | Not supported - use WSL2 as alternative |

### Windows Alternative: WSL2

Since Windows does not have native FUSE support and the `fuser` crate used by this project does not support Windows, you can run torrent-fuse on Windows using WSL2:

1. Install WSL2 with a Linux distribution (Ubuntu recommended)
2. Install FUSE libraries inside WSL2:
   ```bash
   sudo apt-get update
   sudo apt-get install libfuse-dev
   ```
3. Install rqbit and torrent-fuse within WSL2
4. Mount the filesystem to a path inside WSL2:
   ```bash
   mkdir -p ~/torrents
   torrent-fuse mount ~/torrents
   ```
5. Access the mount from Windows via `\\wsl$\Ubuntu\home\<user>\torrents`

**Technical Note:** Windows support would require either:
- Using WinFsp or Dokan as FUSE alternatives (requires significant code changes)
- Implementing a native Windows filesystem driver (major undertaking)
- Using WSL2's FUSE passthrough (current recommended approach)

## Installation

### From source:

```bash
git clone https://github.com/yourusername/torrent-fuse
cd torrent-fuse
cargo install --path .
```

### From crates.io (once published):

```bash
cargo install torrent-fuse
```

## Quick Start

### 1. Start rqbit Server

```bash
rqbit server start
```

By default, the API runs on `http://127.0.0.1:3030`.

### 2. Add a Torrent

```bash
rqbit add magnet:?xt=urn:btih:...
```

Or from a URL:
```bash
rqbit add http://example.com/file.torrent
```

### 3. Mount the Filesystem

Create a mount point and mount:

```bash
mkdir -p ~/torrents
torrent-fuse mount ~/torrents
```

### 4. Browse and Stream

List torrents:
```bash
ls ~/torrents
```

List files in a torrent:
```bash
ls ~/torrents/"Ubuntu 24.04 ISO"
```

Stream a video:
```bash
mpv ~/torrents/"Movie Name"/movie.mkv
```

Copy a file:
```bash
cp ~/torrents/"Ubuntu 24.04 ISO"/ubuntu-24.04.iso ~/Downloads/
```

### 5. Check Status

```bash
torrent-fuse status
```

### 6. Unmount

```bash
torrent-fuse umount ~/torrents
```

Or use:
```bash
fusermount -u ~/torrents
```

## Configuration

Create a configuration file at `~/.config/torrent-fuse/config.toml`:

```toml
[api]
url = "http://127.0.0.1:3030"
timeout = 30
retry_attempts = 3

[cache]
metadata_ttl = 60
torrent_list_ttl = 30
piece_ttl = 300
max_size = 1073741824  # 1GB

[mount]
auto_unmount = true
allow_other = false

[performance]
read_timeout = 30
max_concurrent_reads = 10
readahead_size = 33554432  # 32MB

[logging]
level = "info"
fuse_operations = false
api_calls = false
metrics_enabled = true
metrics_interval = 60
```

### Environment Variables

All configuration options can be set via environment variables:

- `TORRENT_FUSE_API_URL` - rqbit API URL
- `TORRENT_FUSE_CACHE_MAX_SIZE` - Maximum cache size in bytes
- `TORRENT_FUSE_LOG_LEVEL` - Log level (error, warn, info, debug, trace)
- `TORRENT_FUSE_LOG_FUSE_OPS` - Enable FUSE operation logging (true/false)
- `TORRENT_FUSE_LOG_API_CALLS` - Enable API call logging (true/false)
- `TORRENT_FUSE_METRICS_ENABLED` - Enable metrics collection (true/false)
- `TORRENT_FUSE_STATUS_POLL_INTERVAL` - Torrent status poll interval in seconds
- `TORRENT_FUSE_STALLED_TIMEOUT` - Stalled torrent detection timeout in seconds
- `TORRENT_FUSE_PIECE_CHECK_ENABLED` - Enable piece availability checking (true/false)
- `TORRENT_FUSE_RETURN_EAGAIN` - Return EAGAIN for unavailable pieces (true/false)

## Performance Tips

### Optimizing Read Performance

1. **Use media players with buffering**: mpv, vlc, and other players buffer ahead, which triggers rqbit's readahead and improves streaming performance

2. **Read sequentially**: Sequential reads enable the read-ahead optimization (32MB default). Random access cancels prefetching.

3. **Wait for initial pieces**: First access to a file may be slow while the initial pieces download. rqbit prioritizes pieces needed for the requested range.

4. **Pre-download strategy**: Let torrent download some pieces before mounting for better initial performance:
   ```bash
   rqbit add magnet:?xt=urn:btih:...
   # Wait for 5-10% completion
   torrent-fuse mount ~/torrents
   ```

5. **Tune cache settings**: Increase cache size for better metadata caching if you have memory available:
   ```toml
   [cache]
   max_entries = 5000  # Increase from default 1000
   metadata_ttl = 120    # Increase TTL for less frequent API calls
   ```

6. **Adjust read-ahead for your connection**: For high-latency connections, increase read-ahead:
   ```toml
   [performance]
   readahead_size = 67108864  # 64MB for high-latency connections
   ```

### Understanding Performance Characteristics

- **Initial piece latency**: First read to a piece requires downloading from peers (typically 100ms-2s depending on swarm health)
- **Sequential bonus**: rqbit's 32MB readahead means sequential reads get ~32MB of prefetching
- **HTTP overhead**: Each FUSE read translates to one HTTP Range request (mitigated by kernel buffering)
- **Cache effectiveness**: Directory listings and file attributes are cached (30-60s TTL by default)

## Usage

### Mount Command

```bash
torrent-fuse mount <MOUNT_POINT> [OPTIONS]
```

Options:
- `-u, --api-url <URL>` - rqbit API URL (default: http://127.0.0.1:3030)
- `--mount-point <PATH>` - Mount point path
- `-a, --allow-other` - Allow other users to access the mount
- `--auto-unmount` - Automatically unmount on process exit
- `-v, --verbose` - Enable verbose logging (DEBUG level)
- `-vv, --very-verbose` - Enable very verbose logging (TRACE level)
- `-q, --quiet` - Only show errors

### Umount Command

```bash
torrent-fuse umount <MOUNT_POINT> [OPTIONS]
```

Options:
- `-f, --force` - Force unmount

### Status Command

```bash
torrent-fuse status [MOUNT_POINT] [OPTIONS]
```

Options:
- `-f, --format <FORMAT>` - Output format: text or json (default: text)

## Examples

### Stream a Video with mpv

```bash
# Mount the filesystem
torrent-fuse mount ~/torrents

# Play video (starts immediately, downloads on demand)
mpv ~/torrents/"Big Buck Bunny"/bbb_sunflower_1080p_60fps_normal.mp4

# Seeking works - rqbit prioritizes needed pieces
```

### Read Specific File Offset

```bash
# Read bytes 1048576-1049600 (1MiB offset, 1KiB size)
dd if=~/torrents/"Ubuntu ISO"/ubuntu.iso bs=1 skip=1048576 count=1024
```

### Check Torrent Status via Extended Attributes

```bash
# Get torrent status as JSON
getfattr -n user.torrent.status ~/torrents/"Ubuntu 24.04 ISO"

# List available extended attributes
getfattr -d ~/torrents/"Ubuntu 24.04 ISO"
```

### Run as a Systemd Service

Create `~/.config/systemd/user/torrent-fuse.service`:

```ini
[Unit]
Description=Torrent FUSE filesystem
After=network.target

[Service]
Type=forking
ExecStart=/usr/local/bin/torrent-fuse mount /home/user/torrents --auto-unmount
ExecStop=/usr/local/bin/torrent-fuse umount /home/user/torrents
Restart=on-failure

[Install]
WantedBy=default.target
```

Enable and start:
```bash
systemctl --user daemon-reload
systemctl --user enable torrent-fuse
systemctl --user start torrent-fuse
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     User Filesystem                          │
│  /mnt/torrents/                                              │
│  ├── ubuntu-24.04.iso/                                       │
│  └── big-buck-bunny/                                         │
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
│  │ Protocol     │  │ (port 3030)  │  │ (lib)        │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└─────────────────────────────────────────────────────────────┘
```

### Component Details

**FUSE Client (torrent-fuse)**
- Handles FUSE callbacks (lookup, readdir, read, getattr)
- Manages inode allocation and directory structure
- Implements LRU cache with TTL for metadata and pieces
- HTTP client with retry logic, circuit breaker, and exponential backoff
- Background torrent status monitoring with stalled detection

**rqbit Server (external dependency)**
- Runs as separate daemon managing torrent downloads
- Exposes HTTP API on port 3030
- Handles BitTorrent protocol, DHT, peer connections
- Automatic piece prioritization with 32MB readahead for streaming

## How It Works

### File Reading Flow

When you read from a file in the FUSE filesystem:

1. **FUSE Callback**: Kernel sends `read(inode, offset, size)` request
2. **Offset Translation**: FUSE offset is translated to HTTP Range request
3. **HTTP Range Request**: Client requests specific byte range from rqbit
4. **Piece Prioritization**: rqbit prioritizes pieces needed for the range
5. **On-Demand Download**: rqbit downloads pieces from peers as needed
6. **Streaming**: Data streams directly to FUSE as pieces become available
7. **Caching**: Metadata and frequently accessed pieces cached in memory
8. **Read-Ahead**: Sequential reads trigger prefetching of upcoming pieces

### HTTP Range Requests

```
FUSE Request:                    HTTP Request:
read(inode=42,                   GET /torrents/123/stream/0
     offset=1048576,             Range: bytes=1048576-1114111
     size=65536)
                                        │
                                        ▼
                              ┌──────────────────┐
                              │   rqbit Server   │
                              │  - Maps range to │
                              │    pieces 45-50  │
                              │  - Prioritizes   │
                              │    those pieces │
                              │  - Downloads     │
                              │    with readahead│
                              └──────────────────┘
```

### Error Handling Strategy

- **Connection failures**: Automatic retry with exponential backoff (3 attempts)
- **Server unavailable**: Circuit breaker opens after 5 failures, recovers after 30s
- **Timeout handling**: Configurable timeouts with EAGAIN for non-blocking behavior
- **Piece unavailability**: Returns EAGAIN when pieces aren't downloaded yet
- **Path traversal protection**: Sanitizes filenames and prevents `..` attacks

## Implementation Status

### ✅ Completed

**Phase 1-5: Core Implementation**
- Rust project structure with all dependencies
- Core data structures (Torrent, InodeEntry, FileAttr)
- Complete rqbit HTTP API client with retry logic
- Configuration system (file, env vars, CLI)
- Full FUSE implementation with all callbacks
- Inode management with DashMap for concurrent access
- Directory operations (lookup, readdir, mkdir, rmdir)
- File attributes (getattr, setattr)
- Read operations with HTTP Range support
- LRU cache with TTL and eviction policies
- Read-ahead optimization with sequential detection
- Torrent lifecycle management (add, monitor, remove)
- Comprehensive error handling with circuit breaker
- Edge case handling (symlinks, unicode, large files, path traversal)
- CLI with subcommands (mount, umount, status)
- Extended attributes for torrent status
- Background status monitoring with stalled detection

**Current Stats:**
- 50+ unit tests passing
- 8+ integration tests
- Zero clippy warnings
- Full error handling coverage

### 🚧 In Progress

**Phase 6: User Experience**
- Structured logging with tracing (partially implemented)
- Performance metrics and observability

### 📋 Planned

**Phase 7-8: Testing & Release**
- Additional integration tests with actual rqbit server
- Performance benchmarking
- CI/CD pipeline with GitHub Actions
- Multi-platform release builds
- Security audit and final documentation

See [TASKS.md](TASKS.md) for the complete development roadmap.

## Limitations and Known Issues

### Current Limitations

1. **Read-only filesystem** - Cannot create, modify, or delete files through the filesystem. Use rqbit CLI for torrent management.

2. **Single rqbit instance** - Currently only supports connecting to a single rqbit server.

3. **No write support** - The filesystem is intentionally read-only for safety.

4. **Performance depends on rqbit** - File read performance is limited by rqbit's download speed and piece availability.

5. **No symlink following across torrents** - Symlinks within a torrent work, but cannot follow symlinks between torrents.

### Known Issues

1. **Initial reads may be slow** - First access to a file piece requires downloading that piece from peers. Subsequent reads are cached.

2. **Random access penalty** - Reading files non-sequentially cancels read-ahead and may result in slower performance.

3. **Memory usage** - Cache size is configurable but unbounded during heavy concurrent access until eviction kicks in.

4. **Platform differences**:
   - Linux: Full feature support
   - macOS: Requires macFUSE, some features may behave differently
   - Windows: Not supported (FUSE not available)

### Troubleshooting

**"Transport endpoint is not connected"**

The FUSE filesystem crashed or was killed. Unmount and remount:
```bash
fusermount -u ~/torrents
torrent-fuse mount ~/torrents
```

**"Connection refused" to API**

rqbit server is not running. Start it:
```bash
rqbit server start
```

**Permission denied errors**

The filesystem is read-only. Writing operations will fail:
```bash
# These will fail
touch ~/torrents/newfile
rm ~/torrents/somefile
```

**Debug mode**

Run in foreground with debug logging:
```bash
torrent-fuse mount ~/torrents --auto-unmount -vv
```

## Development

### Running Tests

```bash
cargo test
```

### Running Linter

```bash
cargo clippy
```

### Formatting Code

```bash
cargo fmt
```

### Building Release Binary

```bash
cargo build --release
```

### Docker Development (Linux on macOS)

Since this is a FUSE-based project with platform-specific differences, you may need to build and test the Linux version while developing on macOS.

```bash
# Build the Docker image
docker build -t torrent-fuse-dev .

# Run all tests
docker run --rm -v "$(pwd):/app" torrent-fuse-dev

# Run specific test
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo test <test_name>

# Build release binary for Linux
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo build --release

# Run linter
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo clippy

# Format code
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo fmt

# Interactive shell
docker run --rm -it -v "$(pwd):/app" torrent-fuse-dev bash
```

## License

This project is licensed under the MIT OR Apache-2.0 license.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- [rqbit](https://github.com/ikatson/rqbit) - The torrent client that powers this filesystem
- [fuser](https://github.com/cberner/fuser) - Rust FUSE bindings
