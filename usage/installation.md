# Installation Guide

This guide will walk you through installing rqbit-fuse on your system.

## Prerequisites

Before installing rqbit-fuse, you need:

1. **rqbit server** - The BitTorrent client that powers rqbit-fuse
2. **FUSE libraries** - Required for creating the virtual filesystem

### Step 1: Install rqbit

First, install and set up rqbit:

```bash
# Install rqbit
cargo install rqbit

# Start the server
rqbit server start
```

By default, rqbit runs on `http://127.0.0.1:3030`.

### Step 2: Install FUSE

#### Linux

**Ubuntu/Debian:**
```bash
sudo apt-get update
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

#### macOS

```bash
brew install macfuse
```

**Note:** After installing macFUSE, you may need to approve it in System Preferences > Security & Privacy.

#### Windows

Windows is not directly supported. Use WSL2 instead:

1. Install WSL2 with Ubuntu
2. Inside WSL2, follow the Linux installation steps
3. Access the mount through `\\wsl$\Ubuntu\path\to\mount`

## Step 3: Install rqbit-fuse

### From Source

```bash
git clone https://github.com/yourusername/rqbit-fuse
cd rqbit-fuse
cargo install --path .
```

### From crates.io (when available)

```bash
cargo install rqbit-fuse
```

## Verify Installation

Check that rqbit-fuse is installed correctly:

```bash
rqbit-fuse --version
```

You should see the version number printed.

## System Requirements

- **Operating System**: Linux (kernel 3.0+) or macOS (10.14+)
- **Memory**: 512MB minimum, 2GB recommended for large torrents
- **Disk Space**: Minimal (<50MB for the program itself)
- **Network**: Broadband connection recommended for streaming

## Optional: Enable User Mounts (Linux)

To allow mounting without sudo on Linux:

```bash
# Add your user to the fuse group
sudo usermod -a -G fuse $USER

# Log out and back in for changes to take effect
```

## Next Steps

Once installed, proceed to:
- [Configuration](configuration.md) - Set up your preferences
- [Commands](commands.md) - Learn how to use rqbit-fuse
