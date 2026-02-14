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

### Install torrent-fuse

```bash
cargo install --path .
```

Or from crates.io (once published):
```bash
cargo install torrent-fuse
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
torrent-fuse mount ~/torrents
```

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
torrent-fuse status
```

Output:
```
Mount point: /home/user/torrents
API URL: http://127.0.0.1:3030
Torrents: 5
Cache entries: 23
Active reads: 2
```

### 6. Unmount

```bash
torrent-fuse unmount ~/torrents
```

Or use fusermount:
```bash
fusermount -u ~/torrents
```

## Configuration

Create config file at `~/.config/torrent-fuse/config.toml`:

```toml
[api]
url = "http://127.0.0.1:3030"

[cache]
metadata_ttl = 60
torrent_list_ttl = 30
piece_ttl = 5

[mount]
auto_unmount = true

[performance]
read_timeout = 30
max_concurrent_reads = 10
```

## Command Reference

### Mount
```bash
torrent-fuse mount <PATH> [OPTIONS]

Options:
  -u, --api-url <URL>        rqbit API URL [default: http://127.0.0.1:3030]
  -c, --config <FILE>        Config file path
  -o, --allow-other          Allow other users to access the mount
  -f, --foreground           Run in foreground (don't daemonize)
  -d, --debug                Enable debug logging
```

### Unmount
```bash
torrent-fuse unmount <PATH>
```

### Status
```bash
torrent-fuse status [PATH]
```

### List
```bash
torrent-fuse list
```

Shows all mounted torrents with their download status.

## Examples

### Stream a Video

```bash
# Mount
torrent-fuse mount ~/torrents

# Play with mpv (starts immediately, downloads on demand)
mpv ~/torrents/"Big Buck Bunny"/bbb_sunflower_1080p_60fps_normal.mp4

# Seeking works - rqbit prioritizes needed pieces
```

### Read Specific Offset

```bash
# Read bytes 1048576-1049600 (1MiB offset, 1KiB size)
dd if=~/torrents/"Ubuntu ISO"/ubuntu.iso bs=1 skip=1048576 count=1024
```

### Background Mount

```bash
# Mount in background
torrent-fuse mount ~/torrents &

# Later, unmount
kill %1
```

### Systemd Service

Create `~/.config/systemd/user/torrent-fuse.service`:

```ini
[Unit]
Description=Torrent FUSE filesystem
After=network.target

[Service]
Type=forking
ExecStart=/usr/local/bin/torrent-fuse mount /home/user/torrents
ExecStop=/usr/local/bin/torrent-fuse unmount /home/user/torrents
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

## Troubleshooting

### "Transport endpoint is not connected"

The FUSE filesystem crashed or was killed. Unmount and remount:
```bash
fusermount -u ~/torrents
torrent-fuse mount ~/torrents
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

### Permission denied

torrent-fuse creates read-only filesystem. Cannot write:
```bash
# This will fail
touch ~/torrents/newfile
```

### Debug mode

Run in foreground with debug logging:
```bash
torrent-fuse mount ~/torrents -f -d
```

## Performance Tips

1. **Buffering**: Media players buffer ahead, triggering rqbit's readahead
2. **Sequential reads**: Read files sequentially when possible
3. **Wait for initial pieces**: First read may be slow while pieces download
4. **Pre-download**: Let torrent download some pieces before mounting

## Security

- Filesystem is **read-only**
- Files are **not executable**
- Runs as **regular user** (no root needed)
- API connection is **local only** by default
