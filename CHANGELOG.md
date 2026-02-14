# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release of torrent-fuse
- FUSE filesystem implementation for mounting torrents as read-only filesystem
- rqbit HTTP API client with retry logic and circuit breaker pattern
- Inode management with thread-safe concurrent access
- Cache layer with TTL and LRU eviction policies
- Read-ahead optimization for sequential file access
- Torrent lifecycle management (add, monitor, remove)
- Comprehensive error handling with FUSE error code mapping
- CLI interface with mount, umount, and status commands
- Structured logging and metrics collection
- Extended attributes support for torrent status
- Symbolic link support
- Security features: path traversal protection, input sanitization
- Support for single-file and multi-file torrents
- Unicode filename support
- Large file support (>4GB)
- Graceful degradation with configurable piece checking
- Background torrent status monitoring
- 76 unit tests with mocked API responses
- 12 integration tests covering filesystem operations
- 10 performance tests with Criterion benchmarks
- CI/CD pipeline with GitHub Actions
- Automated release workflow for multiple platforms

### Security

- Path traversal protection in filename sanitization
- Circuit breaker pattern to prevent cascade failures
- Input validation for all user-provided paths
- Read-only filesystem permissions (0o444 for files, 0o555 for directories)

## [0.1.0] - 2026-02-13

### Added

- First stable release
- Complete FUSE filesystem implementation
- Full rqbit API integration
- Comprehensive test suite (88 tests total)
- Documentation (README, API docs, architecture)
- Multi-platform support (Linux, macOS)

[Unreleased]: https://github.com/anomalyco/torrent-fuse/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/anomalyco/torrent-fuse/releases/tag/v0.1.0
