# Research: FUSE Testing Specification Review

**Date:** 2024-02-14
**Task:** FS-007.1 - Read testing specification
**Source:** spec/testing.md

## Summary

Reviewed the comprehensive testing specification (1691 lines) for rqbit-fuse FUSE filesystem implementation. This document provides a detailed roadmap for implementing proper FUSE operation tests.

## Key Findings

### 1. FUSE Testing Approaches (Section 1)

Four main approaches identified:

1. **Mock FUSE Operations** (Section 1.1)
   - Fast unit tests without kernel interaction
   - No root privileges required
   - Deterministic execution
   - Uses `MockFuseSession` pattern
   - Limitation: Doesn't test real kernel FUSE integration

2. **Docker-based Integration Tests** (Section 1.2)
   - Real FUSE kernel integration
   - Isolated environment with FUSE dependencies
   - CI/CD friendly
   - Test scenarios: mount/unmount, file operations, concurrent access

3. **CI Testing via GitHub Actions** (Section 1.3)
   - Unit tests (fast feedback)
   - Integration tests
   - FUSE tests with privileged containers
   - Property tests
   - Benchmarks for performance regression
   - Coverage tracking with tarpaulin

4. **Real Filesystem Operation Tests** (Section 1.4)
   - Tests actual filesystem operations through kernel
   - Uses `TempDir` for mount points
   - Tests open, read, close, directory listing, error scenarios

### 2. Test Types Needed (Section 2)

**Unit Tests:**
- Cache: TTL boundary, eviction under load, statistics accuracy
- Inode: Parent consistency, path uniqueness, torrent mapping, deep nesting
- API Client: Retry logic, error mapping, circuit breaker

**Integration Tests:**
- FUSE operation tests (new file: tests/fuse_operations.rs)
- Cache integration tests
- Mock verification tests

**Concurrent Access Tests:**
- Fix `test_concurrent_torrent_additions` (TEST-003)
- Use barriers for proper synchronization

### 3. Test Infrastructure (Section 4)

**Common Utilities Needed:**

1. **tests/common/mock_server.rs**
   - `setup_mock_server()` - Basic mock server
   - `setup_mock_server_with_torrents()` - With torrent data
   - `setup_mock_server_with_data()` - With streaming data
   - `create_test_config()` - Test configuration helper

2. **tests/common/fuse_helpers.rs**
   - `TestFilesystem` struct for lifecycle management
   - `wait_for_mount()` helper
   - Mount/unmount handling

3. **tests/common/fixtures.rs**
   - `single_file_torrent()`
   - `multi_file_torrent()`
   - `deeply_nested_torrent(depth)`
   - `unicode_torrent()`

### 4. Proposed Test File Organization

```
tests/
├── common/
│   ├── mod.rs
│   ├── mock_server.rs
│   └── fuse_helpers.rs
├── integration_tests.rs (existing)
├── performance_tests.rs (existing)
├── fuse_operations.rs (NEW - priority)
├── cache_tests.rs (NEW)
├── concurrent_tests.rs (NEW)
├── mock_verification_tests.rs (NEW)
└── property_*_tests.rs (NEW)
```

### 5. Priority Implementation Order

According to spec:
1. Fix `test_concurrent_torrent_additions` (TEST-003)
2. Create `tests/fuse_operations.rs` with basic FUSE tests (TEST-002)
3. Add cache integration tests (TEST-004)
4. Add mock verification tests (TEST-005)
5. Implement property-based tests (TEST-006, TEST-007)

## Next Steps

Per TODO.md FS-007.2, need to set up FUSE testing infrastructure:
- Create `tests/common/` directory
- Implement `tests/common/mock_server.rs`
- Implement `tests/common/fuse_helpers.rs`
- Create test fixtures module

## References

- TODO.md: FS-007 task group
- Source: spec/testing.md (1691 lines)
