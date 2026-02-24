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
rqbit-fuse mount [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-m, --mount-point <PATH>` | Directory where torrents will be mounted (uses config default if not specified) |
| `-u, --api-url <URL>` | rqbit API URL (default: http://127.0.0.1:3030) |
| `--username <USER>` | rqbit API username for HTTP Basic Auth |
| `--password <PASS>` | rqbit API password for HTTP Basic Auth |
| `-a, --allow-other` | Allow other users to access the mount |
| `--auto-unmount` | Automatically unmount when process exits |

**Examples:**

```bash
# Mount to default location
rqbit-fuse mount

# Mount to specific directory
rqbit-fuse mount -m ~/torrents

# Mount with custom API URL
rqbit-fuse mount -m ~/torrents -u http://192.168.1.100:3030

# Mount with authentication
rqbit-fuse mount -m ~/torrents --username admin --password secret

# Mount with auto-unmount (useful for scripts)
rqbit-fuse mount -m ~/torrents --auto-unmount

# Mount with debug logging
rqbit-fuse mount -m ~/torrents -v
```

### umount

Unmount the torrent filesystem.

```bash
rqbit-fuse umount [OPTIONS]
```

**Options:**
| `-m, --mount-point <PATH>` | Directory to unmount (uses config default if not specified) |
| Option | Description |
|--------|-------------|
| `-f, --force` | Force unmount even if in use |

**Examples:**

```bash
# Unmount using config default
rqbit-fuse umount

# Unmount specific directory
rqbit-fuse umount -m ~/torrents

# Force unmount
rqbit-fuse umount -m ~/torrents --force
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
rqbit-fuse status [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-c, --config <FILE>` | Use custom config file |
| `-v, --verbose` | Enable verbose output |
| `-q, --quiet` | Only show errors |

**Examples:**

```bash
# Show status
rqbit-fuse status

# Show status with custom config
rqbit-fuse status -c ~/my-config.toml
```

## Usage Examples

### Basic Workflow

```bash
# Start the filesystem
mkdir -p ~/torrents
rqbit-fuse mount -m ~/torrents

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
rqbit-fuse umount -m ~/torrents
```

### Streaming Workflow

```bash
# Mount with auto-unmount (good for media players)
rqbit-fuse mount -m ~/torrents --auto-unmount

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

### Reading Specific File Ranges

```bash
# Read bytes 1048576-1049600 from a file
dd if=~/torrents/ISO/ubuntu.iso bs=1 skip=1048576 count=1024

# Works with any tool that supports seeking
head -c 1049600 ~/torrents/file.bin | tail -c 1024
```

## Tips and Best Practices

### Performance

1. **Use media players with buffering** - Players like mpv, vlc buffer ahead for smoother playback
2. **Pre-download for first-time access** - First access to a file may be slow while initial pieces download

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
