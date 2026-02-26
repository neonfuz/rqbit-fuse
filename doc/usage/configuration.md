# Configuration Guide

rqbit-fuse can be configured through configuration files, environment variables, or command-line options.

## Configuration File

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

### Configuration Options

#### API Settings

| Option | Description | Default |
|--------|-------------|---------|
| `url` | rqbit server URL | `http://127.0.0.1:3030` |
| `username` | HTTP Basic Auth username (optional) | - |
| `password` | HTTP Basic Auth password (optional) | - |

#### Mount Settings

| Option | Description | Default |
|--------|-------------|---------|
| `mount_point` | Default mount directory | `/mnt/torrents` |

#### Performance Settings

| Option | Description | Default |
|--------|-------------|---------|
| `read_timeout` | Maximum time to wait for reads (seconds) | 30 |
| `max_concurrent_reads` | Simultaneous read operations | 10 |

#### Logging Settings

| Option | Description | Default |
|--------|-------------|---------|
| `level` | Log verbosity: error, warn, info, debug, trace | `info` |

### Minimal Configuration

You only need to specify settings you want to change:

```toml
[api]
url = "http://192.168.1.100:3030"
username = "admin"
password = "secret"

[mount]
mount_point = "~/torrents"
```

## Environment Variables

All settings can be overridden via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `TORRENT_FUSE_API_URL` | rqbit API URL | `http://127.0.0.1:3030` |
| `TORRENT_FUSE_AUTH_USERNAME` | HTTP Basic Auth username | - |
| `TORRENT_FUSE_AUTH_PASSWORD` | HTTP Basic Auth password | - |
| `TORRENT_FUSE_MOUNT_POINT` | Default mount point | `/mnt/torrents` |
| `TORRENT_FUSE_READ_TIMEOUT` | Read timeout in seconds | 30 |
| `TORRENT_FUSE_LOG_LEVEL` | Log level | `info` |

Example:
```bash
export TORRENT_FUSE_API_URL="http://192.168.1.100:3030"
export TORRENT_FUSE_MOUNT_POINT="~/torrents"
rqbit-fuse mount
```

## Performance Tuning

### For High-Latency Connections

Increase timeouts:

```toml
[performance]
read_timeout = 60
```

### For Low-Memory Systems

Reduce concurrent reads:

```toml
[performance]
max_concurrent_reads = 5
```

## Configuration Precedence

Settings are applied in this order (later overrides earlier):

1. Default values
2. Configuration file
3. Environment variables
4. Command-line options

## Example Configurations

### Home Media Server

```toml
[api]
url = "http://192.168.1.50:3030"

[mount]
mount_point = "/media/torrents"

[logging]
level = "warn"  # Less verbose for server use
```

### Development/Testing

```toml
[api]
url = "http://127.0.0.1:3030"

[mount]
mount_point = "./test-mount"

[logging]
level = "debug"  # Verbose logging for debugging
```
