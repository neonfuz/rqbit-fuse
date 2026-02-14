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

- [x] **Implement FUSE trait: initialization** (2026-02-13)
  - Implement `init()` callback
  - Set up connection to rqbit server
  - Validate mount point and permissions
  - Initialize root inode (inode 1)

- [x] **Implement FUSE trait: directory operations** (2026-02-13)
  - Implement `lookup()` - resolve path to inode
  - Implement `readdir()` - list directory entries
  - Implement `mkdir()` - create directories (if supported)
  - Implement `rmdir()` - remove directories
  - Handle `.` and `..` entries correctly
  - Populate directory entries from torrent file tree

- [x] **Implement FUSE trait: file attributes** (2026-02-13)
  - Implement `getattr()` - get file attributes
  - Implement ` setattr()` - modify attributes (where applicable)
  - Map file sizes from torrent metadata
  - Set appropriate permissions (read-only mostly)
  - Handle timestamps (creation, modification, access)

## Phase 3: Read Operations & Caching

- [x] **Implement FUSE read callback** (2026-02-13)
  - Implement `read()` - read file contents
  - Translate FUSE read requests to HTTP Range requests
  - Handle piece-aligned reads for efficiency
  - Map read offsets to piece indices
  - Wait for pieces to be available before reading

- [x] **Implement cache layer** (2026-02-13)
  - Create `Cache` struct with TTL support
  - Implement piece-level caching
  - Implement LRU eviction policy
  - Support configurable cache size
  - Add cache hit/miss metrics
  - Ensure thread-safe cache access

- [x] **Implement read-ahead optimization** (2026-02-13)
  - Detect sequential read patterns
  - Prefetch next pieces while serving current request
  - Make read-ahead size configurable
  - Cancel prefetch on random access detection

## Phase 4: Torrent Lifecycle & Management

- [x] **Implement torrent addition flow** (2026-02-13)
  - Parse magnet links and .torrent files
  - Add torrents to rqbit via API
  - Map rqbit torrent IDs to filesystem paths
  - Create directory structure for new torrents
  - Handle duplicate torrent detection

- [x] **Implement torrent status monitoring** (2026-02-13)
  - Poll rqbit for download progress
  - Expose piece availability via filesystem attributes
  - Handle stalled/failed torrents gracefully
  - Update file sizes as download progresses

- [x] **Implement torrent removal** (2026-02-13)
  - Implemented `unlink()` FUSE callback for removing torrent directories from root
  - Implemented `remove_torrent()` method to remove torrents from rqbit (using `forget_torrent` API)
  - Implemented `remove_torrent_by_id()` convenience method
  - Clean up inodes recursively on torrent removal using `inode_manager.remove_inode()`
  - Handle open file descriptors during removal - returns EBUSY if files are open
  - Added comprehensive test `test_remove_torrent_cleans_up_inodes`
  - All 30 tests passing, no clippy warnings

## Phase 5: Error Handling & Edge Cases

- [x] **Implement comprehensive error mapping** (2026-02-13)
  - Added expanded `ApiError` types: `ConnectionTimeout`, `ReadTimeout`, `ServerDisconnected`, `CircuitBreakerOpen`, `NetworkError`, `ServiceUnavailable`
  - Implemented comprehensive FUSE error code mapping in `ApiError::to_fuse_error()` for 13+ error types
  - Added `ApiError::is_transient()` method to identify retryable errors
  - Added `ApiError::is_server_unavailable()` method for availability detection
  - Implemented `From<reqwest::Error>` for proper HTTP error classification
  - Created `CircuitBreaker` struct with Closed/Open/HalfOpen states
  - Added circuit breaker integration in `RqbitClient` with 5-failure threshold and 30s timeout
  - Enhanced `execute_with_retry()` to use circuit breaker and transient error detection
  - Improved `health_check()` with circuit breaker state tracking
  - Added `wait_for_server()` with exponential backoff for startup
  - Updated filesystem callbacks to use `ApiError::to_fuse_error()` via downcasting
  - Added comprehensive tests for error mapping, transient detection, and circuit breaker
  - All 32 tests passing, no clippy warnings

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

- Phase 4: Torrent Lifecycle & Management - Starting with torrent status monitoring

## Completed

*Tasks as they are finished*

- [x] **Implement torrent status monitoring** (2026-02-13)
  - Added `MonitoringConfig` struct with `status_poll_interval` (default 5s) and `stalled_timeout` (default 300s)
  - Created `TorrentState` enum with states: Downloading, Seeding, Paused, Stalled, Error, Unknown
  - Created `TorrentStatus` struct tracking: torrent_id, state, progress_pct, progress_bytes, total_bytes, downloaded_pieces, total_pieces, last_updated
  - Implemented background status monitoring task with configurable polling interval
  - Added `getxattr` FUSE callback exposing `user.torrent.status` extended attribute with JSON status
  - Added `listxattr` FUSE callback listing available extended attributes
  - Implemented stalled detection based on timeout without progress updates
  - Added `monitor_torrent()` and `unmonitor_torrent()` methods to add/remove torrents from monitoring
  - Updated `create_torrent_structure()` to automatically start monitoring new torrents
  - Status monitoring starts in `init()` and stops in `destroy()` callbacks
  - Environment variables: `TORRENT_FUSE_STATUS_POLL_INTERVAL` and `TORRENT_FUSE_STALLED_TIMEOUT`
  - All 29 tests passing, no clippy warnings

- [x] **Implement torrent addition flow** (2026-02-13)
  - Added `add_torrent_magnet()` method to TorrentFS for adding torrents from magnet links
  - Added `add_torrent_url()` method for adding torrents from torrent file URLs
  - Implemented `create_torrent_structure()` to build filesystem hierarchy from torrent info
  - Added `create_file_entry()` helper to handle nested directory structures within torrents
  - Implemented duplicate torrent detection using `lookup_torrent()` before creating structure
  - Added `has_torrent()` and `list_torrents()` methods for torrent management
  - Added `sanitize_filename()` helper to handle problematic characters in torrent/filenames
  - Extended InodeManager with accessor methods: `entries()`, `torrent_to_inode()`, `get_all_torrent_ids()`
  - All 29 tests passing, no clippy warnings

- [x] **Implement read-ahead optimization** (2026-02-13)
  - Created `ReadState` struct to track sequential read patterns per file
  - Implemented `track_and_prefetch()` method to detect sequential access
  - Trigger prefetch after 2 consecutive sequential reads
  - Configurable read-ahead size via `config.performance.readahead_size` (default 32MB)
  - Spawn async prefetch tasks in background using tokio::spawn
  - Reset sequential counter on random access (non-sequential reads)
  - Use Arc<Mutex<HashMap>> for thread-safe read state tracking
  - All 29 tests passing, no clippy warnings

- [x] **Implement cache layer** (2026-02-13)
  - Created `Cache<K, V>` struct in `src/cache.rs` with thread-safe DashMap storage
  - Implemented TTL (time-to-live) support per entry with expiration checking
  - Implemented LRU (Least Recently Used) eviction using global sequence counter
  - Added `CacheStats` for hit/miss/eviction/expired metrics
  - Used AtomicU64 for efficient concurrent access counting
  - Implemented async API with proper locking for statistics
  - Added 6 comprehensive tests covering all cache operations
  - All 29 tests passing, no clippy warnings

- [x] **Implement FUSE read callback** (2026-02-13)
  - Implemented `read()` callback to read file contents via HTTP Range requests
  - Implemented `open()` callback with read-only access validation
  - Implemented `release()` callback for file close cleanup
  - Translate FUSE read requests to rqbit HTTP Range requests via `api_client.read_file()`
  - Handle offset validation and EOF boundary checks
  - Use `tokio::task::block_in_place()` to bridge async HTTP calls in sync FUSE callbacks
  - Map API errors to appropriate FUSE error codes (ENOENT, EINVAL, EIO)
  - All 23 tests passing, no clippy warnings

- [x] **Implement FUSE trait: file attributes** (2026-02-13)
  - Implemented `getattr()` callback to retrieve file/directory attributes
  - Implemented `setattr()` callback allowing only atime/mtime updates (read-only)
  - File sizes mapped from torrent metadata via `InodeEntry::File { size, .. }`
  - Permissions set to 0o444 for files (read-only) and 0o555 for directories
  - Timestamps use fixed creation time (UNIX_EPOCH + 1.7B seconds) and current time for atime/mtime
  - All 23 tests passing, no clippy warnings

- [x] **Implement FUSE trait: directory operations** (2026-02-13)
  - Implemented `lookup()` callback to resolve path components to inodes
  - Implemented `readdir()` callback to list directory contents with `.` and `..` entries
  - Implemented `mkdir()` callback returning EROFS (read-only filesystem)
  - Implemented `rmdir()` callback returning EROFS (read-only filesystem)
  - Added `build_file_attr()` helper to convert InodeEntry to FUSE FileAttr
  - Set appropriate permissions (0o555 for directories, 0o444 for files)
  - All 23 tests passing, no clippy warnings

- [x] **Implement FUSE trait: initialization** (2026-02-13)
  - Created `TorrentFS` struct in `src/fs/filesystem.rs` implementing `fuser::Filesystem`
  - Implemented `init()` callback with mount point validation and root inode verification
  - Implemented `destroy()` callback for cleanup on unmount
  - Added connection validation and health check methods for rqbit server
  - Created mount options builder with configurable options (RO, NoSuid, NoDev, etc.)
  - Added `mount()` method as main entry point for mounting
  - Added 6 comprehensive unit tests for filesystem initialization
  - Updated `lib.rs` to create and mount the filesystem in `run()` function
  - All 23 tests passing, no compilation errors

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
