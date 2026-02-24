# rqbit-fuse User Guide

Welcome to rqbit-fuse! This guide will help you mount and stream torrents as a regular filesystem.

## What is rqbit-fuse?

rqbit-fuse is a tool that lets you access BitTorrent content through your regular file manager or command line. Instead of waiting for an entire torrent to download before you can use it, files become available instantly and download on-demand as you access them.

### Key Features

- **Stream videos while downloading** - Watch movies without waiting for the full download
- **Browse archives instantly** - Access files inside torrents immediately
- **Copy files on-demand** - Grab just the files you need, when you need them
- **Full filesystem support** - Works with any application that can read files
- **Safe and read-only** - Cannot modify or delete torrent content

## Quick Start

```bash
# 1. Start rqbit server (in another terminal)
rqbit server start

# 2. Add a torrent
rqbit download magnet:?xt=urn:btih:...

# 3. Mount the filesystem
mkdir -p ~/torrents
rqbit-fuse mount ~/torrents

# 4. Access your torrents
ls ~/torrents
mpv ~/torrents/MovieName/video.mkv
```

## Documentation

- **[Installation](installation.md)** - Install rqbit-fuse on your system
- **[Configuration](configuration.md)** - Customize settings and options
- **[Commands](commands.md)** - Command reference and usage examples
- **[Troubleshooting](troubleshooting.md)** - Solve common problems

## Platform Support

| Platform | Support | Notes |
|----------|---------|-------|
| Linux    | ✅ Full | Native support, all features available |
| macOS    | ✅ Full | Requires macFUSE installation |
| Windows  | ❌ None | Use WSL2 as alternative |

## How It Works

When you access a file through rqbit-fuse:

1. **Files appear instantly** - The filesystem shows all torrent content immediately
2. **On-demand downloading** - Files download only when you actually read them
3. **Smart streaming** - Video players can seek anywhere without waiting
4. **Automatic cleanup** - Files are managed by rqbit according to its settings

## Next Steps

1. Read the [Installation Guide](installation.md) to get rqbit-fuse running
2. Check [Configuration](configuration.md) to customize for your needs
3. See [Commands](commands.md) for detailed usage examples
4. Visit [Troubleshooting](troubleshooting.md) if you encounter issues
