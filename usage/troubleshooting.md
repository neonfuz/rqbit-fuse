# Troubleshooting

Common issues and how to resolve them.

## Installation Issues

### "FUSE library not found"

**Problem:** The FUSE development libraries are not installed.

**Solution:**

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install libfuse-dev

# Fedora/RHEL
sudo dnf install fuse-devel

# Arch
sudo pacman -S fuse2
```

**macOS:**
```bash
brew install macfuse
```

After installing macFUSE, you may need to approve it in System Preferences > Security & Privacy.

### "cargo: command not found"

**Problem:** Rust is not installed.

**Solution:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### "Permission denied" during install

**Problem:** Cannot write to cargo bin directory.

**Solution:**
```bash
# Ensure cargo bin is in PATH
export PATH="$HOME/.cargo/bin:$PATH"

# Or install to a local directory
cargo install --path . --root ~/.local
```

## Mount Issues

### "Transport endpoint is not connected"

**Problem:** The FUSE filesystem crashed or was killed.

**Solution:**
```bash
# Force unmount
fusermount -u ~/torrents
# or
rqbit-fuse umount ~/torrents --force

# Then remount
rqbit-fuse mount ~/torrents
```

### "Connection refused" or "Connection reset by peer"

**Problem:** Cannot connect to rqbit server.

**Solution:**
```bash
# Check if rqbit is running
curl http://127.0.0.1:3030/

# Start rqbit if not running
rqbit server start

# Verify API URL in config
rqbit-fuse status -f json
```

### "Mount point does not exist"

**Problem:** The mount directory doesn't exist.

**Solution:**
```bash
mkdir -p ~/torrents
rqbit-fuse mount ~/torrents
```

### "Permission denied" when mounting

**Problem:** Insufficient permissions to mount.

**Solutions:**

1. **Add user to fuse group (Linux):**
```bash
sudo usermod -a -G fuse $USER
# Log out and back in
```

2. **Allow other users:**
```bash
rqbit-fuse mount ~/torrents --allow-other
```

3. **Use sudo (not recommended):**
```bash
sudo rqbit-fuse mount ~/torrents
```

### "Device or resource busy"

**Problem:** Mount point is already in use.

**Solution:**
```bash
# Check if already mounted
mount | grep torrents

# Find processes using the mount
lsof ~/torrents
fuser -m ~/torrents

# Kill processes or wait for them to finish
# Then unmount and remount
fusermount -u ~/torrents
rqbit-fuse mount ~/torrents
```

## Runtime Issues

### "Input/output error" when reading files

**Problem:** Error reading from torrent.

**Possible causes and solutions:**

1. **rqbit server stopped:**
```bash
# Restart rqbit
rqbit server start
```

2. **Torrent not fully available:**
```bash
# Check torrent status in rqbit
rqbit stats
```

3. **Piece not available:**
- Wait for peers to provide the piece
- Try accessing a different part of the file

### Files appear empty or with wrong size

**Problem:** Metadata cache is stale.

**Solution:**
```bash
# Reduce cache TTL in config
# Or restart the mount
rqbit-fuse umount ~/torrents
rqbit-fuse mount ~/torrents
```

### Slow performance or stuttering video

**Problem:** Streaming is not smooth.

**Solutions:**

1. **Increase read-ahead:**
```toml
[performance]
readahead_size = 67108864  # 64MB
```

2. **Increase cache:**
```toml
[cache]
max_entries = 5000
metadata_ttl = 120
```

3. **Check network connection:**
- Ensure good connectivity to rqbit server
- Check torrent swarm health with `rqbit stats`

4. **Use a player with better buffering:**
- mpv with cache settings: `mpv --cache=yes --cache-secs=60`
- VLC with increased buffer size

### "No such file or directory" for existing torrents

**Problem:** Torrent not found or not loaded.

**Solution:**
```bash
# List active torrents
rqbit list

# Add torrent if missing
rqbit download magnet:?xt=urn:btih:...

# Restart mount to refresh
rqbit-fuse umount ~/torrents
rqbit-fuse mount ~/torrents
```

## Permission Issues

### Cannot write to mounted filesystem

**This is expected behavior.** The filesystem is read-only for safety.

To modify torrents, use the rqbit CLI:
```bash
# Add torrents
rqbit download magnet:?xt=urn:btih:...

# Remove torrents
rqbit remove <torrent_id>
```

### Other users cannot access the mount

**Problem:** Mount is only accessible by the mounting user.

**Solution:**
```bash
# Allow other users
rqbit-fuse mount ~/torrents --allow-other

# Or set in config
# /etc/fuse.conf: user_allow_other
```

## macOS-Specific Issues

### "System Extension Blocked"

**Problem:** macOS blocked the macFUSE kernel extension.

**Solution:**
1. Open System Preferences → Security & Privacy
2. Click "Allow" next to the message about macFUSE
3. Restart if prompted

### "Operation not permitted"

**Problem:** macOS System Integrity Protection restrictions.

**Solution:**
1. Grant Full Disk Access to Terminal/iTerm:
   - System Preferences → Security & Privacy → Privacy → Full Disk Access
   - Add your terminal application

2. Or run with elevated permissions:
```bash
sudo rqbit-fuse mount ~/torrents
```

## Windows (WSL2) Issues

### Cannot access mount from Windows

**Problem:** Windows cannot see the WSL2 mount.

**Solution:**
Access through the WSL path:
```
\\wsl$\Ubuntu\home\username\torrents
```

Or map as a network drive:
```powershell
net use T: "\\wsl$\Ubuntu\home\username\torrents"
```

### WSL2 mount disappears after Windows restart

**Problem:** WSL2 doesn't persist mounts across reboots.

**Solution:**
Add to `~/.bashrc` or `~/.profile`:
```bash
# Auto-mount on WSL2 start
if [ -d ~/torrents ]; then
    rqbit-fuse mount ~/torrents 2>/dev/null || true
fi
```

## Getting Help

If you're still experiencing issues:

1. **Enable debug logging:**
```bash
rqbit-fuse mount ~/torrents -vv
```

2. **Check system logs:**
```bash
# Linux
dmesg | tail -50
journalctl -xe

# macOS
log show --predicate 'process == "rqbit-fuse"' --last 1h
```

3. **Verify your setup:**
```bash
# Check versions
rqbit-fuse --version
rqbit --version

# Check mount
mount | grep fuse

# Check rqbit connectivity
curl http://127.0.0.1:3030/
```

4. **Report issues:**
   - Include debug output
   - Specify your OS and version
   - Include rqbit and rqbit-fuse versions
   - Describe steps to reproduce
