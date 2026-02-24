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
- **💾 Smart caching** - LRU cache with TTL for metadata, configurable size limits
- **🛡️ Resilient API client** - Exponential backoff, automatic retry logic
- **🔒 Read-only filesystem** - Safe, secure access that cannot modify torrents (mode 0444/0555)
- **🔄 Torrent management** - Add via magnet/URL, remove torrents
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
   rqbit server start <download_folder>
   ```

## Platform Support

| Platform | Support | Notes |
|----------|---------|-------|
| Linux    | Full    | Native FUSE support, all features available |
| macOS    | Full    | Requires macFUSE, all features available |
| Windows  | None    | Not supported - use WSL2 as alternative |

### Windows Alternative: WSL2

Since Windows does not have native FUSE support and the `fuser` crate used by this project does not support Windows, you can run rqbit-fuse on Windows using WSL2:

1. Install WSL2 with a Linux distribution (Ubuntu recommended)
2. Install FUSE libraries inside WSL2:
   ```bash
   sudo apt-get update
   sudo apt-get install libfuse-dev
   ```
3. Install rqbit and rqbit-fuse within WSL2
4. Mount the filesystem to a path inside WSL2:
   ```bash
   mkdir -p ~/torrents
   rqbit-fuse mount ~/torrents
   ```
5. Access the mount from Windows via `\\wsl$\Ubuntu\home\<user>\torrents`

**Technical Note:** Windows support would require either:
- Using WinFsp or Dokan as FUSE alternatives (requires significant code changes)
- Implementing a native Windows filesystem driver (major undertaking)
- Using WSL2's FUSE passthrough (current recommended approach)

## Installation

### From source:

```bash
git clone https://github.com/yourusername/rqbit-fuse
cd rqbit-fuse
cargo install --path .
```

### From crates.io (once published):

```bash
cargo install rqbit-fuse
```

## Quick Start

### 1. Start rqbit Server

```bash
rqbit server start
```

By default, the API runs on `http://127.0.0.1:3030`.

### 2. Add a Torrent

```bash
rqbit download magnet:?xt=urn:btih:...
```

Or from a URL:
```bash
rqbit download http://example.com/file.torrent
```

### 3. Mount the Filesystem

Create a mount point and mount:

```bash
mkdir -p ~/torrents
rqbit-fuse mount ~/torrents
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
rqbit-fuse status
```

### 6. Unmount

```bash
rqbit-fuse umount ~/torrents
```

Or use:
```bash
fusermount -u ~/torrents
```

## Configuration

Create a configuration file at `~/.config/rqbit-fuse/config.toml`:

```toml
[api]
url = "http://127.0.0.1:3030"

[cache]
metadata_ttl = 60
max_entries = 1000

[mount]
mount_point = "/mnt/torrents"

[performance]
read_timeout = 30
max_concurrent_reads = 10
readahead_size = 33554432  # 32MB

[logging]
level = "info"
```

### Minimal Configuration

Only the settings you want to change from defaults are needed:

```toml
[api]
url = "http://192.168.1.100:3030"

[mount]
mount_point = "~/torrents"
```

### Environment Variables

Essential configuration options can be set via environment variables:

- `TORRENT_FUSE_API_URL` - rqbit API URL (default: http://127.0.0.1:3030)
- `TORRENT_FUSE_MOUNT_POINT` - Mount point path (default: /mnt/torrents)
- `TORRENT_FUSE_METADATA_TTL` - Cache TTL in seconds (default: 60)
- `TORRENT_FUSE_MAX_ENTRIES` - Maximum cache entries (default: 1000)
- `TORRENT_FUSE_READ_TIMEOUT` - Read timeout in seconds (default: 30)
- `TORRENT_FUSE_LOG_LEVEL` - Log level (default: info)

## Performance Tips

### Optimizing Read Performance

1. **Use media players with buffering**: mpv, vlc, and other players buffer ahead, which triggers rqbit's readahead and improves streaming performance

2. **Read sequentially**: Sequential reads enable the read-ahead optimization (32MB default). Random access cancels prefetching.

3. **Wait for initial pieces**: First access to a file may be slow while the initial pieces download. rqbit prioritizes pieces needed for the requested range.

4. **Pre-download strategy**: Let torrent download some pieces before mounting for better initial performance:
   ```bash
   rqbit add magnet:?xt=urn:btih:...
   # Wait for 5-10% completion
   rqbit-fuse mount ~/torrents
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
rqbit-fuse mount <MOUNT_POINT> [OPTIONS]
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
rqbit-fuse umount <MOUNT_POINT> [OPTIONS]
```

Options:
- `-f, --force` - Force unmount

### Status Command

```bash
rqbit-fuse status [MOUNT_POINT] [OPTIONS]
```

Options:
- `-f, --format <FORMAT>` - Output format: text or json (default: text)

## Examples

### Stream a Video with mpv

```bash
# Mount the filesystem
rqbit-fuse mount ~/torrents

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

Create `~/.config/systemd/user/rqbit-fuse.service`:

```ini
[Unit]
Description=Torrent FUSE filesystem
After=network.target

[Service]
Type=forking
ExecStart=/usr/local/bin/rqbit-fuse mount /home/user/torrents --auto-unmount
ExecStop=/usr/local/bin/rqbit-fuse umount /home/user/torrents
Restart=on-failure

[Install]
WantedBy=default.target
```

Enable and start:
```bash
systemctl --user daemon-reload
systemctl --user enable rqbit-fuse
systemctl --user start rqbit-fuse
```

## Architecture

For detailed architecture documentation, see the [API documentation](https://docs.rs/rqbit-fuse) or the source code in `src/lib.rs`.

At a high level, rqbit-fuse implements a FUSE filesystem that translates file operations into HTTP Range requests to the rqbit server. Files are downloaded on-demand when accessed, enabling streaming of torrent content without waiting for complete downloads.

## How It Works

When you access a file through the FUSE filesystem:

1. **On-Demand Access**: Files are downloaded only when you read them
2. **HTTP Range Requests**: File reads are translated to HTTP Range requests
3. **Piece Prioritization**: rqbit prioritizes downloading pieces needed for your read
4. **Smart Caching**: Frequently accessed metadata and pieces are cached in memory
5. **Read-Ahead**: Sequential file reads trigger prefetching for better performance

### Error Handling

- **Automatic Retries**: Temporary failures are retried with exponential backoff
- **Graceful Degradation**: Returns EAGAIN when pieces aren't available yet
- **Path Security**: Sanitizes filenames and prevents directory traversal attacks

## Project Status

rqbit-fuse is feature-complete with comprehensive test coverage:

- **Core Features**: Full FUSE filesystem implementation with on-demand downloading
- **Performance**: LRU cache, read-ahead optimization, connection pooling
- **Reliability**: 350+ tests, zero clippy warnings, comprehensive error handling
- **Edge Cases**: Symlinks, unicode, large files, path traversal protection

See [TODO.md](TODO.md) for remaining documentation and testing tasks.

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
rqbit-fuse mount ~/torrents
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
rqbit-fuse mount ~/torrents --auto-unmount -vv
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
docker build -t rqbit-fuse-dev .

# Run all tests
docker run --rm -v "$(pwd):/app" rqbit-fuse-dev

# Run specific test
docker run --rm -v "$(pwd):/app" rqbit-fuse-dev cargo test <test_name>

# Build release binary for Linux
docker run --rm -v "$(pwd):/app" rqbit-fuse-dev cargo build --release

# Run linter
docker run --rm -v "$(pwd):/app" rqbit-fuse-dev cargo clippy

# Format code
docker run --rm -v "$(pwd):/app" rqbit-fuse-dev cargo fmt

# Interactive shell
docker run --rm -it -v "$(pwd):/app" rqbit-fuse-dev bash
```

## License

This project is licensed under the MIT OR Apache-2.0 license.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- [rqbit](https://github.com/ikatson/rqbit) - The torrent client that powers this filesystem
- [fuser](https://github.com/cberner/fuser) - Rust FUSE bindings
