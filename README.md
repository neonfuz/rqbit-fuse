# rqbit-fuse

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

A read-only FUSE filesystem that mounts BitTorrent torrents as virtual directories, enabling seamless access to torrent content without waiting for full downloads.

Access your torrents through standard filesystem operations—stream videos while they download, copy files on-demand, or browse archives instantly. Powered by [rqbit](https://github.com/ikatson/rqbit) for the BitTorrent protocol.

## Features

- **🎬 Stream torrents as files** - Access torrent content through standard filesystem operations without waiting for full download
- **⬇️ On-demand downloading** - Files and pieces are downloaded only when accessed
- **📺 Video streaming** - Watch videos while they download with full seeking support
- **💾 Metadata caching** - Simple TTL-based caching for torrent list
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
   rqbit-fuse mount -m ~/torrents
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
rqbit-fuse mount -m ~/torrents
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
rqbit-fuse umount -m ~/torrents
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

[mount]
mount_point = "/mnt/torrents"

[performance]
read_timeout = 30
max_concurrent_reads = 10

[logging]
level = "info"
```

### Minimal Configuration

Only the settings you want to change from defaults are needed:

```toml
[api]
url = "http://192.168.1.100:3030"
username = "admin"
password = "secret"

[mount]
mount_point = "~/torrents"
```

### Environment Variables

Essential configuration options can be set via environment variables:

- `TORRENT_FUSE_API_URL` - rqbit API URL (default: http://127.0.0.1:3030)
- `TORRENT_FUSE_AUTH_USERNAME` - HTTP Basic Auth username
- `TORRENT_FUSE_AUTH_PASSWORD` - HTTP Basic Auth password
- `TORRENT_FUSE_MOUNT_POINT` - Mount point path (default: /mnt/torrents)
- `TORRENT_FUSE_READ_TIMEOUT` - Read timeout in seconds (default: 30)
- `TORRENT_FUSE_LOG_LEVEL` - Log level (default: info)

## Performance Tips

### Optimizing Read Performance

1. **Use media players with buffering**: mpv, vlc, and other players buffer ahead, which improves streaming performance

2. **Wait for initial pieces**: First access to a file may be slow while the initial pieces download. rqbit prioritizes pieces needed for the requested range.

3. **Pre-download strategy**: Let torrent download some pieces before mounting for better initial performance:
   ```bash
   rqbit add magnet:?xt=urn:btih:...
   # Wait for 5-10% completion
   rqbit-fuse mount -m ~/torrents
   ```

4. **Increase timeout for slow connections**:
   ```toml
   [performance]
   read_timeout = 60
   ```

### Understanding Performance Characteristics

- **Initial piece latency**: First read to a piece requires downloading from peers (typically 100ms-2s depending on swarm health)
- **HTTP overhead**: Each FUSE read translates to one HTTP Range request (mitigated by kernel buffering)
- **Metadata caching**: Torrent list is cached with 30-second TTL

## Usage

### Mount Command

```bash
rqbit-fuse mount [OPTIONS]
```

Options:
- `-m, --mount-point <PATH>` - Mount point path (required unless set in config)
- `-u, --api-url <URL>` - rqbit API URL (default: http://127.0.0.1:3030)
- `--username <USER>` - rqbit API username for HTTP Basic Auth
- `--password <PASS>` - rqbit API password for HTTP Basic Auth
- `-a, --allow-other` - Allow other users to access the mount
- `--auto-unmount` - Automatically unmount on process exit
- `-c, --config <FILE>` - Config file path
- `-v, --verbose` - Enable verbose logging (repeatable: INFO -> DEBUG -> TRACE)
- `-q, --quiet` - Only show errors

### Umount Command

```bash
rqbit-fuse umount [OPTIONS]
```

Options:
- `-m, --mount-point <PATH>` - Mount point path (required unless set in config)
- `-f, --force` - Force unmount
- `-c, --config <FILE>` - Config file path

### Status Command

```bash
rqbit-fuse status [OPTIONS]
```

Options:
- `-c, --config <FILE>` - Config file path
- `-v, --verbose` - Enable verbose output
- `-q, --quiet` - Only show errors

## Examples

### Stream a Video with mpv

```bash
# Mount the filesystem
rqbit-fuse mount -m ~/torrents

# Play video (starts immediately, downloads on demand)
mpv ~/torrents/"Big Buck Bunny"/bbb_sunflower_1080p_60fps_normal.mp4

# Seeking works - rqbit prioritizes needed pieces
```

### Read Specific File Offset

```bash
# Read bytes 1048576-1049600 (1MiB offset, 1KiB size)
dd if=~/torrents/"Ubuntu ISO"/ubuntu.iso bs=1 skip=1048576 count=1024
```

### Run as a Systemd Service

Create `~/.config/systemd/user/rqbit-fuse.service`:

```ini
[Unit]
Description=Torrent FUSE filesystem
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rqbit-fuse mount -m /home/user/torrents --auto-unmount
ExecStop=/usr/local/bin/rqbit-fuse umount -m /home/user/torrents
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
4. **Metadata Caching**: Torrent list is cached with 30-second TTL

### Error Handling

- **Automatic Retries**: Temporary failures are retried with exponential backoff
- **Graceful Degradation**: Returns EAGAIN when pieces aren't available yet
- **Path Security**: Sanitizes filenames and prevents directory traversal attacks

## Project Status

rqbit-fuse is feature-complete with comprehensive test coverage:

- **Core Features**: Full FUSE filesystem implementation with on-demand downloading
- **Performance**: Connection pooling, retry logic, concurrent read support
- **Reliability**: Comprehensive tests, zero clippy warnings, error handling
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

2. **Memory usage** - Concurrent reads use memory proportional to the number of simultaneous operations.

4. **Platform differences**:
   - Linux: Full feature support
   - macOS: Requires macFUSE, some features may behave differently
   - Windows: Not supported (FUSE not available)

### Troubleshooting

**"Transport endpoint is not connected"**

The FUSE filesystem crashed or was killed. Unmount and remount:
```bash
fusermount -u ~/torrents
rqbit-fuse mount -m ~/torrents
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

Run with debug logging:
```bash
rqbit-fuse mount -m ~/torrents --auto-unmount -vv
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
