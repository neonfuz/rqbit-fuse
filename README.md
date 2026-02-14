# torrent-fuse

A FUSE filesystem for accessing torrents via rqbit. Mount your torrents as a regular filesystem and stream files on demand.

## Features

- **Stream torrents as files** - Access torrent content through standard filesystem operations
- **On-demand downloading** - Files are downloaded only when accessed
- **Video streaming** - Watch videos while they download (supports seeking)
- **Read-ahead optimization** - Detects sequential reads and prefetches pieces
- **Smart caching** - LRU cache with TTL for improved performance
- **Circuit breaker pattern** - Resilient API client with automatic retry logic
- **Extended attributes** - Check torrent status via `user.torrent.status` xattr
- **Read-only filesystem** - Safe, secure access that cannot modify torrents

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
┌─────────────────┐
│  FUSE Callbacks │  ← FUSE filesystem operations
└────────┬────────┘
         │
┌────────▼────────┐
│  Inode Manager  │  ← Maps paths to inodes, manages directory structure
└────────┬────────┘
         │
┌────────▼────────┐
│  Cache Layer    │  ← LRU cache with TTL for pieces
└────────┬────────┘
         │
┌────────▼────────┐
│   RqbitClient   │  ← HTTP client with retry logic and circuit breaker
└────────┬────────┘
         │
┌────────▼────────┐
│  rqbit Server   │  ← Torrent client with HTTP API
└─────────────────┘
```

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

## License

This project is licensed under the MIT OR Apache-2.0 license.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- [rqbit](https://github.com/ikatson/rqbit) - The torrent client that powers this filesystem
- [fuser](https://github.com/cberner/fuser) - Rust FUSE bindings
