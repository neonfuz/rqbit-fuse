# Torrent-Fuse Implementation Tasks

Prioritized task list for building torrent-fuse. Tasks are ordered by dependency and importance.

## Phase 1: Foundation & Setup

- [x] **Initialize Rust project structure**
  - Create Cargo.toml with dependencies (fuser, tokio, reqwest, serde, anyhow, dashmap)
  - Set up src/ directory structure (main.rs, lib.rs, fs/ mod, api/ mod)
  - Configure cargo project for FUSE development

- [x] **Create core data structures**
  - Define `Torrent` struct with metadata fields
  - Define `TorrentFile` struct for individual files
  - Define `InodeEntry` enum (Directory, File)
  - Define `FileAttr` struct for FUSE attributes
  - Implement serialization/deserialization with serde

- [x] **Implement rqbit HTTP API client**
  - Create `api::RqbitClient` with base URL configuration
  - Implement `POST /torrents` - add torrent from magnet/link
  - Implement `GET /torrents` - list all torrents
  - Implement `GET /torrents/{id}` - get torrent details
  - Implement `GET /torrents/{id}/files` - list files in torrent
  - Implement `GET /torrents/{id}/haves` - get piece availability bitfield
  - Implement `GET /torrents/{id}/stream/{file_idx}` with HTTP Range support
  - Add retry logic with exponential backoff
  - Map API errors to appropriate types with FUSE error code conversion

- [x] **Create configuration system**
  - Define `Config` struct with all options
  - Support config file (JSON/YAML/TOML)
  - Support environment variables
  - Support CLI argument overrides
  - Set defaults for cache size, timeouts, mount point

## Phase 2: FUSE Filesystem Core

- [x] **Implement inode management** (2026-02-13)
  - Create `InodeManager` with DashMap for concurrent access
  - Implement inode allocation (starting at 1 for root)
  - Map inodes to paths and vice versa
  - Handle inode lifecycle (creation, lookup, forget)
  - Thread-safe inode generation

- [ ] **Implement FUSE trait: initialization**
  - Implement `init()` callback
  - Set up connection to rqbit server
  - Validate mount point and permissions
  - Initialize root inode (inode 1)

- [ ] **Implement FUSE trait: directory operations**
  - Implement `lookup()` - resolve path to inode
  - Implement `readdir()` - list directory entries
  - Implement `mkdir()` - create directories (if supported)
  - Implement `rmdir()` - remove directories
  - Handle `.` and `..` entries correctly
  - Populate directory entries from torrent file tree

- [ ] **Implement FUSE trait: file attributes**
  - Implement `getattr()` - get file attributes
  - Implement ` setattr()` - modify attributes (where applicable)
  - Map file sizes from torrent metadata
  - Set appropriate permissions (read-only mostly)
  - Handle timestamps (creation, modification, access)

## Phase 3: Read Operations & Caching

- [ ] **Implement FUSE read callback**
  - Implement `read()` - read file contents
  - Translate FUSE read requests to HTTP Range requests
  - Handle piece-aligned reads for efficiency
  - Map read offsets to piece indices
  - Wait for pieces to be available before reading

- [ ] **Implement cache layer**
  - Create `Cache` struct with TTL support
  - Implement piece-level caching
  - Implement LRU eviction policy
  - Support configurable cache size
  - Add cache hit/miss metrics
  - Ensure thread-safe cache access

- [ ] **Implement read-ahead optimization**
  - Detect sequential read patterns
  - Prefetch next pieces while serving current request
  - Make read-ahead size configurable
  - Cancel prefetch on random access detection

## Phase 4: Torrent Lifecycle & Management

- [ ] **Implement torrent addition flow**
  - Parse magnet links and .torrent files
  - Add torrents to rqbit via API
  - Map rqbit torrent IDs to filesystem paths
  - Create directory structure for new torrents
  - Handle duplicate torrent detection

- [ ] **Implement torrent status monitoring**
  - Poll rqbit for download progress
  - Expose piece availability via filesystem attributes
  - Handle stalled/failed torrents gracefully
  - Update file sizes as download progresses

- [ ] **Implement torrent removal**
  - Implement `unlink()` for files
  - Implement torrent removal from rqbit
  - Clean up inodes on torrent removal
  - Handle open file descriptors during removal

## Phase 5: Error Handling & Edge Cases

- [ ] **Implement comprehensive error mapping**
  - Map API errors to FUSE error codes (ENOENT, EACCES, EIO, etc.)
  - Handle network timeouts gracefully
  - Handle rqbit server disconnection
  - Implement retry with circuit breaker pattern

- [ ] **Handle edge cases**
  - Zero-byte files
  - Very large files (>4GB)
  - Torrents with single file vs directory
  - Symbolic links in torrents
  - Unicode filenames
  - Concurrent access to same file
  - Read requests spanning multiple pieces

- [ ] **Implement graceful degradation**
  - Serve partial data when pieces unavailable
  - Return EAGAIN for unavailable pieces (configurable)
  - Handle slow piece downloads without blocking

## Phase 6: CLI & User Experience

- [ ] **Build CLI interface**
  - Implement argument parsing with clap
  - Support `mount` command with options
  - Support `umount` command
  - Support `status` command to show mounted filesystems
  - Add verbose/quiet logging options

- [ ] **Implement logging and observability**
  - Add structured logging with tracing
  - Log all FUSE operations (debug mode)
  - Log API calls and responses
  - Add metrics: cache hit rate, read latency, throughput

- [ ] **Create user documentation**
  - Write comprehensive README
  - Document installation steps
  - Document configuration options
  - Provide usage examples
  - Document limitations and known issues

## Phase 7: Testing & Quality

- [ ] **Unit tests**
  - Test inode management
  - Test API client with mocked responses
  - Test cache operations
  - Test configuration parsing

- [ ] **Integration tests**
  - Test FUSE operations with memory filesystem
  - Test with actual rqbit server
  - Test with sample torrents
  - Test error scenarios

- [ ] **Performance tests**
  - Benchmark read throughput
  - Benchmark cache efficiency
  - Test with concurrent readers
  - Profile memory usage

- [ ] **Add CI/CD**
  - GitHub Actions workflow
  - Run tests on PR
  - Build releases for multiple platforms
  - Publish to crates.io

## Phase 8: Polish & Release

- [ ] **Security review**
  - Audit for path traversal vulnerabilities
  - Ensure proper permission checks
  - Review all unsafe code (if any)
  - Add security policy

- [ ] **Performance optimization**
  - Profile and optimize hot paths
  - Reduce allocations in read path
  - Optimize piece-to-offset calculations
  - Tune cache parameters

- [ ] **Final documentation**
  - API documentation (rustdoc)
  - Architecture decision records
  - Contributing guidelines
  - Changelog

- [ ] **Release preparation**
  - Version bump and tagging
  - Create release notes
  - Publish to crates.io
  - Create GitHub release with binaries

---

## In Progress

*Current task being worked on*

## Completed

*Tasks as they are finished*

- [x] **Initialize Rust project structure** (2024-02-13)
  - Created Cargo.toml with all required dependencies
  - Set up src/ directory with lib.rs, main.rs, and module structure
  - Created api/, config/, fs/, and types/ modules
  - Added basic stub implementations for core types

- [x] **Create core data structures** (2024-02-13)
  - Defined `Torrent` struct with metadata fields
  - Defined `TorrentFile` struct for individual files
  - Defined `InodeEntry` enum (Directory, File)
  - Defined helper functions for FUSE FileAttr
  - All types implement serde serialization

- [x] **Implement rqbit HTTP API client** (2026-02-13)
  - Created `api::RqbitClient` with configurable base URL and retry logic
  - Implemented torrent management: list, get, add (magnet/URL), delete, forget
  - Implemented file operations: read with HTTP Range support via `/stream/{file_idx}`
  - Implemented piece bitfield retrieval via `/haves` endpoint
  - Implemented torrent control: pause, start, get stats
  - Added exponential backoff retry logic for transient failures
  - Created comprehensive `ApiError` enum with FUSE error code mapping
  - Added health check endpoint for connection validation
  - All 4 API client tests passing

- [x] **Create configuration system** (2026-02-13)
  - Defined comprehensive `Config` struct with nested structures (ApiConfig, CacheConfig, MountConfig, PerformanceConfig)
  - Support for TOML and JSON config files
  - Support for environment variables with TORRENT_FUSE_* prefix
  - Support for CLI argument overrides using clap
  - Default config locations: ~/.config/torrent-fuse/config.toml, /etc/torrent-fuse/config.toml, ./torrent-fuse.toml
  - Configuration merging: defaults -> file -> env -> CLI
  - Added 4 unit tests for config parsing and merging
  - All 8 tests passing, no clippy warnings

- [x] **Implement inode management** (2026-02-13)
  - Created `InodeManager` in `src/fs/inode.rs` with DashMap for concurrent access
  - Thread-safe inode allocation using AtomicU64 (starting at 1 for root)
  - Bidirectional mapping: inode -> entry and path -> inode
  - Specialized methods: `allocate_torrent_directory()`, `allocate_file()`
  - Parent-child relationship tracking via `add_child()` / `remove_child()`
  - Lifecycle methods: `remove_inode()`, `clear_torrents()`
  - Added 10 comprehensive unit tests including concurrent allocation test
  - All 18 tests passing, no clippy warnings

## Discovered Issues

*New issues found during implementation*

## Notes

*Additional context, decisions, and learnings*

- **Important**: Treat `src/lib/` as shared utilities
- Run tests after each task: `cargo test`
- Run linting: `cargo clippy`
- Format code: `cargo fmt`
