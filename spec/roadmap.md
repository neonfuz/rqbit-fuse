# Development Roadmap

## Implementation Status

### ✅ Completed Features

#### Foundation (Week 1)
- ✅ Initialize Cargo project
- ✅ Add dependencies (fuser, tokio, reqwest, serde, clap, anyhow, thiserror, dashmap, etc.)
- ✅ Define API types (Torrent, TorrentFile, InodeEntry, FileAttr)
- ✅ Implement HTTP client for rqbit API
  - ✅ GET /torrents
  - ✅ GET /torrents/{id}
  - ✅ GET /torrents/{id}/haves
  - ✅ GET /torrents/{id}/stream/{file_idx} with Range
  - ✅ GET /torrents/{id}/stats/v1
  - ✅ POST /torrents/{id}/pause
  - ✅ POST /torrents/{id}/start
  - ✅ POST /torrents/{id}/forget
  - ✅ POST /torrents/{id}/delete
- ✅ Error types and conversion (ApiError with comprehensive mapping)
- ✅ Circuit breaker pattern for resilience

#### FUSE Core (Week 2)
- ✅ Implement InodeManager for inode management (replaces InodeTable)
- ✅ Implement FUSE callbacks:
  - ✅ init - Background tasks setup
  - ✅ lookup - Find files/directories by path
  - ✅ getattr - Return file attributes
  - ✅ readdir - List directory contents with torrent discovery
  - ✅ read - Read file data with persistent streaming
  - ✅ open - File open with permission checks
  - ✅ release - File handle cleanup
  - ✅ readlink - Symlink resolution
- ✅ Map torrents to directories, files to virtual files
- ✅ Support nested directory structures within torrents
- ✅ Single-file torrent optimization (added directly to root)

#### Caching & Performance (Week 3)
- ✅ Implement generic LRU cache with TTL
  - ✅ Torrent list cache (30s TTL, configurable)
  - ✅ Torrent details cache (60s TTL, configurable)
- ✅ LRU eviction when capacity reached
- ✅ Cache statistics (hits, misses, evictions, expired)
- ✅ PersistentStreamManager for connection reuse
- ✅ Retry logic with exponential backoff
- ✅ Circuit breaker pattern
- ✅ Read-ahead/prefetching for sequential reads
- ✅ Piece availability checking (optional EAGAIN)

#### CLI & Configuration (Week 4)
- ✅ CLI commands:
  - ✅ mount -m <path> [--options]
  - ✅ umount <path> [--force]
  - ✅ status [--format text|json]
- ❌ list (documented but not implemented)
- ❌ cache clear (documented but not implemented)
- ❌ daemon (documented but not implemented)
- ✅ Configuration file support (~/.config/torrent-fuse/config.toml)
- ✅ Multiple config file locations (user, system, local)
- ✅ Environment variable overrides
- ✅ Command-line flags override config
- ✅ Logging and tracing setup with -v/--verbose
- ✅ Additional config sections: [monitoring], [logging]

#### Background Tasks & Monitoring
- ✅ Background torrent discovery with configurable poll interval
- ✅ Status monitoring for download progress
- ✅ Stalled torrent detection
- ✅ Metrics collection (FUSE and API)

#### Error Handling & Resilience
- ✅ Comprehensive error mapping to FUSE error codes
- ✅ Circuit breaker pattern for API resilience
- ✅ Exponential backoff retry logic
- ✅ Transient error detection
- ✅ Timeout handling with tokio::time::timeout

---

### ❌ Not Implemented

- `list` command - Documented but not available
- `cache clear` command - Documented but not available
- `daemon` command - Documented but not available
- Write support (filesystem is read-only)
- macOS support (FUSE-T or macFUSE)
- Cache warm-up/preload
- Statistics/monitoring HTTP endpoint
- Search across all torrent files
- Multi-daemon support (multiple rqbit instances)

---

## Directory Structure

### Actual Implementation
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
│   ├── cache.rs                   # Generic LRU cache with TTL
│   ├── metrics.rs                 # Metrics collection
│   ├── config/
│   │   └── mod.rs                 # Configuration management
│   ├── fs/                        # FUSE filesystem (was "fuse/" in spec)
│   │   ├── mod.rs
│   │   ├── filesystem.rs          # FUSE callbacks implementation
│   │   └── inode.rs               # InodeManager
│   ├── api/                       # HTTP API client
│   │   ├── mod.rs
│   │   ├── client.rs              # HTTP client with circuit breaker
│   │   ├── types.rs               # API types and error mapping
│   │   └── streaming.rs           # PersistentStreamManager
│   └── types/                     # Core type definitions
│       ├── mod.rs
│       ├── torrent.rs             # Torrent structures
│       ├── file.rs                # File structures
│       ├── inode.rs               # InodeEntry enum
│       └── attr.rs                # File attribute helpers
├── tests/
│   ├── integration_tests.rs
│   └── fixtures/
└── spec/
    ├── architecture.md
    ├── api.md
    ├── technical-design.md
    ├── quickstart.md
    └── roadmap.md
```

**Changes from original spec:**
- `fuse/` directory renamed to `fs/`
- `cache/` directory replaced with single `cache.rs` file
- Added `types/` directory for type definitions
- Added `metrics.rs` for metrics collection
- Added `api/streaming.rs` for persistent streaming
- Removed `error.rs` - errors defined in `api/types.rs`

---

## Dependencies

### Core (Implemented)
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

### Additional (Implemented)
```toml
dashmap = "5.5"           # Concurrent hash map
tracing = "0.1"           # Logging
tracing-subscriber = "0.3"
config = "0.14"           # Configuration management
once_cell = "1.19"
libc = "0.2"              # FUSE error codes
futures = "0.3"           # Async stream handling
bytes = "1.5"             # Byte handling
toml = "0.8"              # Config file parsing
dirs = "5.0"              # Config directory detection
```

---

## Testing Strategy

### Unit Tests
- API client parsing
- Inode management (InodeManager)
- Cache logic (LRU eviction, TTL)
- Error mapping
- Configuration parsing

### Integration Tests
- Full mount/unmount cycle
- File reads with various sizes
- Concurrent access patterns
- Error scenarios (API unavailable, timeout)

### Manual Testing Checklist
- ✅ Add torrent to rqbit
- ✅ Mount filesystem
- ✅ `ls` mount point (shows torrents)
- ✅ `ls` torrent directory (shows files)
- ✅ `cat` small file (<1MB)
- ✅ `cat` large file (>100MB)
- ✅ `dd` with various block sizes
- ✅ Open video file with media player (seeking)
- ✅ Multiple concurrent reads
- ✅ Unmount while reading
- ✅ Mount with rqbit not running (circuit breaker)
- ✅ Add torrent while mounted (auto-discovery)

---

## Known Issues & Limitations

### 1. CLI Commands Not Implemented
The following commands are documented but not implemented:
- `torrent-fuse list` - Shows torrent status
- `torrent-fuse cache clear` - Clears metadata cache
- `torrent-fuse daemon` - Daemon mode

### 2. Spec vs Implementation Differences
- `fuse/` directory is actually `fs/`
- `unmount` command is actually `umount`
- Mount point requires `-m` flag, not positional argument
- No `-f/--foreground` or `-d/--debug` flags (use `-v/--verbose`)

### 3. API Compatibility
- rqbit may return 200 OK instead of 206 Partial Content for range requests
- Implementation handles this gracefully

---

## Release Checklist

- [x] All core features implemented
- [x] Basic CLI (mount, umount, status)
- [x] Configuration file support
- [x] Error handling and circuit breaker
- [x] Background tasks (discovery, monitoring)
- [ ] `list` command implementation
- [ ] `cache clear` command implementation
- [ ] `daemon` command implementation
- [ ] Unit tests for all modules
- [ ] Integration tests
- [ ] Documentation complete
- [ ] CHANGELOG.md updated
- [ ] Version bumped in Cargo.toml
- [ ] Git tag created
- [ ] GitHub release created
- [ ] Binary artifacts uploaded
- [ ] Installation instructions tested
- [ ] README has quick start guide

---

## Future Roadmap

### Short Term (Next 1-2 weeks)
- [ ] Implement `list` command
- [ ] Implement `cache clear` command
- [ ] Add comprehensive unit tests
- [ ] Add integration tests

### Medium Term (1-2 months)
- [ ] Daemon mode with auto-mount on startup
- [ ] macOS support (FUSE-T or macFUSE)
- [ ] Statistics/monitoring HTTP endpoint
- [ ] Cache warm-up/preload

### Long Term (3+ months)
- [ ] Write support (if rqbit supports it)
- [ ] Search across all torrent files
- [ ] Multi-daemon support
- [ ] Performance profiling and optimization

---

## Development Phases (Original Plan)

For reference, here was the original development plan:

### Phase 1: Foundation (Week 1)
- Set up Rust project structure
- Implement rqbit API client
- Basic error handling and types

### Phase 2: FUSE Core (Week 2)
- Implement basic FUSE filesystem
- Directory structure (root -> torrents -> files)

### Phase 3: Caching & Performance (Week 3)
- Add metadata caching
- Optimize read performance
- Handle concurrent reads

### Phase 4: CLI & Configuration (Week 4)
- Implement full CLI interface
- Add configuration file support

### Phase 5: Polish & Testing (Week 5)
- Testing and bug fixes
- Documentation
- Edge case handling

### Phase 6: Advanced Features (Week 6-8)
- Add advanced features
- macOS support

**Status:** Phases 1-4 mostly complete. Phase 5 partially complete. Phase 6 not started.

Last updated: 2024-02-14
