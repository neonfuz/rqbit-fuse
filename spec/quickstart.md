# Quick Start Guide

## Installation

### Prerequisites

1. **Rust** - Install via [rustup](https://rustup.rs/)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **FUSE** - Install FUSE development libraries
   
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

3. **rqbit** - Install rqbit server
   ```bash
   cargo install rqbit
   ```

### Install rqbit-fuse

```bash
cargo install --path .
```

Or from crates.io (once published):
```bash
cargo install rqbit-fuse
```

## Usage

### 1. Start rqbit Server

```bash
rqbit server start
```

Default API runs on `http://127.0.0.1:3030`

### 2. Add a Torrent

```bash
rqbit add magnet:?xt=urn:btih:...
```

Or add from URL:
```bash
rqbit add http://example.com/file.torrent
```

### 3. Mount Filesystem

Create mount point:
```bash
mkdir -p ~/torrents
```

Mount:
```bash
rqbit-fuse mount -m ~/torrents
```

**Note:** The mount point is specified with `-m` or `--mount-point` flag, not as a positional argument.

### 4. Browse Files

List torrents:
```bash
ls ~/torrents
```

List files in a torrent:
```bash
ls ~/torrents/"Ubuntu 24.04 ISO"
```

Read a file:
```bash
cat ~/torrents/"Ubuntu 24.04 ISO"/ubuntu-24.04.iso | head -c 1024
```

Copy a file:
```bash
cp ~/torrents/"Ubuntu 24.04 ISO"/ubuntu-24.04.iso ~/Downloads/
```

Play a video (will stream as needed):
```bash
mpv ~/torrents/"Movie Name"/movie.mkv
```

### 5. Check Status

```bash
rqbit-fuse status
```

Output:
```
rqbit-fuse Status
===================

Configuration:
  Config file:    ~/.config/rqbit-fuse/config.toml
  API URL:        http://127.0.0.1:3030
  Mount point:    ~/torrents

Mount Status:
  Status:         MOUNTED
```

Or if not mounted:
```
Mount Status:
  Status:         NOT MOUNTED
```

**Note:** The status output shows only mount status, not detailed filesystem information.

### 6. Unmount

```bash
rqbit-fuse umount ~/torrents
```

With force option:
```bash
rqbit-fuse umount ~/torrents --force
```

Or use fusermount:
```bash
fusermount -u ~/torrents
```

## Configuration

Create config file at `~/.config/rqbit-fuse/config.toml`:

```toml
# rqbit-fuse configuration (6 essential fields)

api_url = "http://127.0.0.1:3030"
mount_point = "/mnt/torrents"
metadata_ttl = 60
max_entries = 1000
read_timeout = 30
log_level = "info"
```

### Alternative Config Locations

Config files are searched in this order:
1. `~/.config/rqbit-fuse/config.toml`
2. `/etc/rqbit-fuse/config.toml`
3. `./rqbit-fuse.toml`

### Environment Variables

All config options can be overridden via environment variables:
- `TORRENT_FUSE_API_URL` - rqbit API URL
- `TORRENT_FUSE_MOUNT_POINT` - Filesystem mount point
- `TORRENT_FUSE_METADATA_TTL` - Metadata cache TTL in seconds
- `TORRENT_FUSE_MAX_ENTRIES` - Maximum cache entries
- `TORRENT_FUSE_READ_TIMEOUT` - HTTP read timeout in seconds
- `TORRENT_FUSE_LOG_LEVEL` - Log level (error, warn, info, debug, trace)

## Command Reference

### Mount
```bash
rqbit-fuse mount [OPTIONS]

Options:
  -m, --mount-point <PATH>   Mount point [env: TORRENT_FUSE_MOUNT_POINT]
  -u, --api-url <URL>        rqbit API URL [env: TORRENT_FUSE_API_URL]
  -c, --config <FILE>        Config file path
  -v, --verbose              Increase verbosity (repeatable: INFO -> DEBUG -> TRACE)
  -q, --quiet                Suppress output except errors
      --username <USER>      rqbit API username for HTTP Basic Auth [env: TORRENT_FUSE_AUTH_USERNAME]
      --password <PASS>      rqbit API password for HTTP Basic Auth [env: TORRENT_FUSE_AUTH_PASSWORD]
```

**Examples:**
```bash
# Basic mount
rqbit-fuse mount -m ~/torrents

# With custom API URL
rqbit-fuse mount -m ~/torrents -u http://localhost:3030

# With verbose logging
rqbit-fuse mount -m ~/torrents -v -v  # TRACE level

# With config file
rqbit-fuse mount -c ~/my-config.toml
```

### Unmount
```bash
rqbit-fuse umount <PATH> [OPTIONS]

Options:
  -f, --force    Force unmount even if busy
```

**Note:** The command is `umount` (not `unmount` as in some earlier documentation).

### Status
```bash
rqbit-fuse status [OPTIONS]

Options:
  -c, --config <FILE>    Config file path
  -v, --verbose          Increase verbosity
  -q, --quiet            Suppress output except errors
```

**Not Implemented (documented but not available):**
- `rqbit-fuse list` - Not implemented
- `rqbit-fuse cache clear` - Not implemented
- `rqbit-fuse daemon` - Not implemented

## Examples

### Stream a Video

```bash
# Mount
rqbit-fuse mount -m ~/torrents

# Play with mpv (starts immediately, downloads on demand)
mpv ~/torrents/"Big Buck Bunny"/bbb_sunflower_1080p_60fps_normal.mp4

# Seeking works - rqbit prioritizes needed pieces
```

### Read Specific Offset

```bash
# Read bytes 1048576-1049600 (1MiB offset, 1KiB size)
dd if=~/torrents/"Ubuntu ISO"/ubuntu.iso bs=1 skip=1048576 count=1024
```

### Background Mount with Logging

```bash
# Mount with debug logging in background
rqbit-fuse mount -m ~/torrents -v -v &

# Later, unmount
rqbit-fuse umount ~/torrents
```

### Systemd Service

Create `~/.config/systemd/user/rqbit-fuse.service`:

```ini
[Unit]
Description=Torrent FUSE filesystem
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rqbit-fuse mount -m /home/user/torrents
ExecStop=/usr/local/bin/rqbit-fuse umount /home/user/torrents
Restart=on-failure

[Install]
WantedBy=default.target
```

**Note:** Uses `Type=simple` (not `forking`) and `-m` flag for mount point.

Enable and start:
```bash
systemctl --user daemon-reload
systemctl --user enable rqbit-fuse
systemctl --user start rqbit-fuse
```

## Troubleshooting

### "Transport endpoint is not connected"

The FUSE filesystem crashed or was killed. Unmount and remount:
```bash
fusermount -u ~/torrents
rqbit-fuse mount -m ~/torrents
```

### "Connection refused" to API

rqbit is not running. Start it:
```bash
rqbit server start
```

### Slow reads

This is normal - data is being downloaded. For better performance:
- Use media players that buffer (mpv, vlc)
- Copy files instead of reading directly
- Wait for more pieces to download
- Increase read-ahead in config

### Permission denied

rqbit-fuse creates read-only filesystem. Cannot write:
```bash
# This will fail
touch ~/torrents/newfile
```

### Debug Logging

Run with verbose logging:
```bash
# Info level
rqbit-fuse mount -m ~/torrents -v

# Debug level
rqbit-fuse mount -m ~/torrents -v -v

# Trace level
rqbit-fuse mount -m ~/torrents -v -v -v

# Or use quiet mode for errors only
rqbit-fuse mount -m ~/torrents -q
```

**Note:** There are no `-f/--foreground` or `-d/--debug` flags. Use `-v/--verbose` for logging control.

### Check Config

```bash
# View current configuration
rqbit-fuse status
```

## Performance Tips

1. **Buffering**: Media players buffer ahead, triggering rqbit's readahead
2. **Sequential reads**: Read files sequentially when possible
3. **Wait for initial pieces**: First read may be slow while pieces download
4. **Pre-download**: Let torrent download some pieces before mounting
5. **Read-ahead**: The filesystem detects sequential reads and prefetches data
6. **Persistent connections**: HTTP connections are reused for sequential reads

## Security

- Filesystem is **read-only**
- Files are **not executable** (mode 0444)
- Directories are **not writable** (mode 0555)
- Runs as **regular user** (no root needed)
- API connection is **local only** by default
- Circuit breaker prevents cascading failures

Last updated: 2024-02-14
