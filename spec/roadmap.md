# Development Roadmap

## Phase 1: Foundation (Week 1)

### Goals
- Set up Rust project structure
- Implement rqbit API client
- Basic error handling and types

### Tasks
- [ ] Initialize Cargo project
- [ ] Add dependencies (fuser, tokio, reqwest, serde, clap, anyhow)
- [ ] Define API types (Torrent, TorrentFile, etc.)
- [ ] Implement HTTP client for rqbit API
  - [ ] GET /torrents
  - [ ] GET /torrents/{id}
  - [ ] GET /torrents/{id}/stream/{file_idx} with Range
- [ ] Error types and conversion

### Deliverables
- API client can list torrents and read file data
- Unit tests for API client

---

## Phase 2: FUSE Core (Week 2)

### Goals
- Implement basic FUSE filesystem
- Directory structure (root -> torrents -> files)

### Tasks
- [ ] Implement InodeTable for inode management
- [ ] Implement FUSE callbacks:
  - [ ] init - Load torrents and create inodes
  - [ ] lookup - Find files/directories
  - [ ] getattr - Return file attributes
  - [ ] readdir - List directory contents
  - [ ] read - Read file data (basic)
- [ ] Map torrents to directories, files to virtual files

### Deliverables
- Can mount filesystem
- Can `ls` torrents and files
- Basic file reads work

---

## Phase 3: Caching & Performance (Week 3)

### Goals
- Add metadata caching
- Optimize read performance
- Handle concurrent reads

### Tasks
- [ ] Implement metadata cache with TTL
  - [ ] Torrent list cache (30s TTL)
  - [ ] Torrent details cache (60s TTL)
- [ ] Add semaphore for concurrent read limiting
- [ ] Implement retry with exponential backoff
- [ ] Add connection pooling

### Deliverables
- Reduced API calls via caching
- Better performance under load
- Resilient to transient failures

---

## Phase 4: CLI & Configuration (Week 4)

### Goals
- Implement full CLI interface
- Add configuration file support

### Tasks
- [ ] CLI commands:
  - [ ] mount <path> [--options]
  - [ ] unmount <path>
  - [ ] status
  - [ ] list
- [ ] Configuration file support (~/.config/torrent-fuse/config.toml)
- [ ] Command-line flags override config
- [ ] Logging and tracing setup

### Deliverables
- User-friendly CLI
- Persistent configuration
- Helpful error messages

---

## Phase 5: Polish & Testing (Week 5)

### Goals
- Testing and bug fixes
- Documentation
- Edge case handling

### Tasks
- [ ] Integration tests
  - [ ] Mount/unmount cycles
  - [ ] Concurrent reads
  - [ ] Large file reads
  - [ ] Error scenarios
- [ ] Handle edge cases:
  - [ ] Empty torrents
  - [ ] Single-file torrents
  - [ ] Multi-file torrents with directories
  - [ ] Torrents being added/removed
- [ ] Add health check for rqbit connection
- [ ] Document common issues and solutions

### Deliverables
- Stable, tested filesystem
- User documentation
- Known issues list

---

## Phase 6: Advanced Features (Week 6-8)

### Goals
- Add advanced features
- macOS support

### Tasks
- [ ] Live reload of torrent list (inotify-style)
- [ ] Cache warm-up/preload
- [ ] Statistics/monitoring endpoint
- [ ] Background cache refresh
- [ ] macOS support (FUSE-T or macFUSE)
- [ ] Performance profiling and optimization

### Deliverables
- Production-ready filesystem
- Cross-platform support
- Performance metrics

---

## Testing Strategy

### Unit Tests
- API client parsing
- Inode management
- Cache logic

### Integration Tests
- Full mount/unmount cycle
- File reads with various sizes
- Concurrent access patterns

### Manual Testing Checklist
- [ ] Add torrent to rqbit
- [ ] Mount filesystem
- [ ] `ls` mount point (shows torrents)
- [ ] `ls` torrent directory (shows files)
- [ ] `cat` small file (<1MB)
- [ ] `cat` large file (>100MB)
- [ ] `dd` with various block sizes
- [ ] Open video file with media player (seeking)
- [ ] Multiple concurrent reads
- [ ] Unmount while reading
- [ ] Mount with rqbit not running
- [ ] Add torrent while mounted

---

## Directory Structure

```
torrent-fuse/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── CHANGELOG.md
├── src/
│   ├── main.rs                    # CLI entry point
│   ├── lib.rs                     # Library exports
│   ├── error.rs                   # Error types
│   ├── config.rs                  # Configuration
│   ├── cli.rs                     # CLI argument parsing
│   ├── api/
│   │   ├── mod.rs
│   │   ├── client.rs              # HTTP client
│   │   └── types.rs               # API types
│   ├── fuse/
│   │   ├── mod.rs
│   │   ├── filesystem.rs          # FUSE implementation
│   │   ├── inode.rs               # Inode management
│   │   └── attr.rs                # File attributes
│   └── cache/
│       ├── mod.rs
│       └── metadata.rs            # Metadata cache
├── tests/
│   ├── integration_tests.rs
│   └── fixtures/
└── spec/
    ├── architecture.md
    ├── api.md
    ├── technical-design.md
    └── roadmap.md
```

---

## Dependencies

### Core
```toml
[dependencies]
fuser = "0.14"
tokio = { version = "1.35", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
clap = { version = "4.4", features = ["derive"] }
anyhow = "1.0"
thiserror = "1.0"
```

### Additional
```toml
tracing = "0.1"
tracing-subscriber = "0.3"
config = "0.14"
dashmap = "5.5"
once_cell = "1.19"
libc = "0.2"
```

---

## Release Checklist

- [ ] All tests pass
- [ ] Documentation complete
- [ ] CHANGELOG.md updated
- [ ] Version bumped in Cargo.toml
- [ ] Git tag created
- [ ] GitHub release created
- [ ] Binary artifacts uploaded
- [ ] Installation instructions tested
- [ ] README has quick start guide
