# Command Reference

Complete reference for all rqbit-fuse commands.

## Global Options

These options work with any command:

| Option | Description |
|--------|-------------|
| `-h, --help` | Show help message |
| `-V, --version` | Show version information |
| `-c, --config <PATH>` | Use custom config file |
| `-v, --verbose` | Enable verbose logging (DEBUG) |
| `-vv, --very-verbose` | Enable very verbose logging (TRACE) |
| `-q, --quiet` | Only show errors |

## Commands

### mount

Mount the torrent filesystem to a directory.

```bash
rqbit-fuse mount [MOUNT_POINT] [OPTIONS]
```

**Arguments:**
- `MOUNT_POINT` - Directory where torrents will be mounted (uses config default if not specified)

**Options:**
| Option | Description |
|--------|-------------|
| `-u, --api-url <URL>` | rqbit API URL (default: http://127.0.0.1:3030) |
| `-a, --allow-other` | Allow other users to access the mount |
| `--auto-unmount` | Automatically unmount when process exits |

**Examples:**

```bash
# Mount to default location
rqbit-fuse mount

# Mount to specific directory
rqbit-fuse mount ~/torrents

# Mount with custom API URL
rqbit-fuse mount ~/torrents -u http://192.168.1.100:3030

# Mount with auto-unmount (useful for scripts)
rqbit-fuse mount ~/torrents --auto-unmount

# Mount with debug logging
rqbit-fuse mount ~/torrents -v
```

### umount

Unmount the torrent filesystem.

```bash
rqbit-fuse umount <MOUNT_POINT> [OPTIONS]
```

**Arguments:**
- `MOUNT_POINT` - Directory to unmount

**Options:**
| Option | Description |
|--------|-------------|
| `-f, --force` | Force unmount even if in use |

**Examples:**

```bash
# Unmount normally
rqbit-fuse umount ~/torrents

# Force unmount
rqbit-fuse umount ~/torrents --force
```

**Alternative unmount methods:**

```bash
# Using fusermount (Linux)
fusermount -u ~/torrents

# Using umount (macOS)
umount ~/torrents
```

### status

Check the status of the mounted filesystem.

```bash
rqbit-fuse status [MOUNT_POINT] [OPTIONS]
```

**Arguments:**
- `MOUNT_POINT` - Mount point to check (uses config default if not specified)

**Options:**
| Option | Description |
|--------|-------------|
| `-f, --format <FORMAT>` | Output format: `text` or `json` (default: text) |

**Examples:**

```bash
# Show status in text format
rqbit-fuse status

# Show status for specific mount
rqbit-fuse status ~/torrents

# Show status as JSON
rqbit-fuse status -f json
```

## Usage Examples

### Basic Workflow

```bash
# Start the filesystem
mkdir -p ~/torrents
rqbit-fuse mount ~/torrents

# List your torrents
ls ~/torrents

# List files in a torrent
ls ~/torrents/Ubuntu\ 24.04/

# Copy a file
cp ~/torrents/Ubuntu\ 24.04/ubuntu.iso ~/Downloads/

# Stream a video
mpv ~/torrents/Movie/video.mkv

# Check status
rqbit-fuse status

# Unmount when done
rqbit-fuse umount ~/torrents
```

### Streaming Workflow

```bash
# Mount with auto-unmount (good for media players)
rqbit-fuse mount ~/torrents --auto-unmount

# Play video (starts immediately)
mpv ~/torrents/Movie\ Name/movie.mkv

# Seeking works seamlessly - rqbit downloads needed pieces on demand
```

### Systemd Service

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

### Reading Specific File Ranges

```bash
# Read bytes 1048576-1049600 from a file
dd if=~/torrents/ISO/ubuntu.iso bs=1 skip=1048576 count=1024

# Works with any tool that supports seeking
head -c 1049600 ~/torrents/file.bin | tail -c 1024
```

### Checking Extended Attributes

Some torrent metadata is available via extended attributes:

```bash
# List available attributes
getfattr -d ~/torrents/TorrentName

# Get torrent status as JSON
getfattr -n user.torrent.status ~/torrents/TorrentName
```

## Tips and Best Practices

### Performance

1. **Use media players with buffering** - Players like mpv, vlc buffer ahead, triggering read-ahead
2. **Read sequentially** - Sequential access enables prefetching for better performance
3. **Pre-download for first-time access** - First access to a file may be slow while initial pieces download

### Safety

1. **The filesystem is read-only** - All write operations will fail
2. **Use --auto-unmount for scripts** - Ensures cleanup on exit
3. **Check status before unmounting** - Verify no active operations

### Debugging

```bash
# Run with debug output
rqbit-fuse mount ~/torrents -vv

# Check kernel messages (Linux)
dmesg | grep fuse

# Monitor FUSE operations (requires fuse debug)
cat /sys/kernel/debug/fuse/requests
```
