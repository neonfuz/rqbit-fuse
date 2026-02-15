# Testing Specification for rqbit-fuse

## Overview

This document outlines the comprehensive testing strategy for rqbit-fuse, covering unit tests, integration tests, property-based tests, and performance benchmarks. The testing approach ensures correctness, reliability, and performance of the FUSE filesystem implementation.

## Current Test Structure

### Existing Test Files

```
rqbit-fuse/
├── tests/
│   ├── integration_tests.rs    # Integration tests with WireMock
│   └── performance_tests.rs    # Performance and stress tests
├── benches/
│   └── performance.rs          # Criterion benchmarks
└── src/
    ├── cache.rs                # Unit tests in #[cfg(test)] module
    └── fs/inode.rs             # Unit tests in #[cfg(test)] module
```

### Current Test Coverage

**Unit Tests (in source files):**
- `cache.rs`: Basic operations, TTL, LRU eviction, concurrent access
- `inode.rs`: Allocation, lookup, path resolution, children management

**Integration Tests (`tests/integration_tests.rs`):**
- Filesystem creation and initialization
- Torrent addition from magnet links
- Multi-file torrent structure
- Duplicate torrent detection
- Error scenarios (API unavailable)
- File attribute generation
- Torrent removal with cleanup
- Deeply nested directory structures
- Unicode and special characters
- Empty torrent handling
- Concurrent torrent additions (needs fixing - TEST-003)
- Filesystem metrics collection

**Performance Tests (`tests/performance_tests.rs`):**
- Cache high throughput (5000 entries)
- Cache efficiency (Pareto access pattern)
- LRU eviction effectiveness
- Concurrent cache readers (10 tasks)
- Read operation timeout
- Cache with large values (1MB entries)

**Benchmarks (`benches/performance.rs`):**
- Cache throughput (insert/read hit/read mixed)
- Inode management (allocate/lookup/parent-child)
- Concurrent operations (cache reads, inode ops)
- Memory usage patterns

## 1. FUSE Testing Approaches

### 1.1 Testing with libfuse Mock

**Purpose:** Test FUSE operations without requiring actual FUSE kernel module

**Approach:**
```rust
// Mock FUSE filesystem operations
use fuser::Filesystem;

struct MockFuseSession {
    fs: TorrentFS,
    // Mock kernel requests/responses
}

impl MockFuseSession {
    fn mock_lookup(&self, parent: u64, name: &str) -> Result<FileAttr, libc::c_int> {
        // Call filesystem lookup without actual FUSE mount
        let mut reply = MockReplyEntry::new();
        self.fs.lookup(
            Request::default(),
            parent,
            &OsStr::new(name),
            reply.clone(),
        );
        reply.get_result()
    }
}
```

**Benefits:**
- Fast unit tests without kernel interaction
- No root privileges required
- Deterministic test execution
- Easy to simulate error conditions

**Limitations:**
- Doesn't test actual kernel FUSE integration
- May miss platform-specific behavior

### 1.2 Docker-based Integration Tests

**Purpose:** Run FUSE tests in isolated Linux environment with real FUSE support

**Setup:**
```dockerfile
# Dockerfile.test
FROM rust:1.75-slim-bookworm

# Install FUSE dependencies
RUN apt-get update && apt-get install -y \
    libfuse-dev \
    fuse3 \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Enable user_allow_other for FUSE
RUN echo "user_allow_other" >> /etc/fuse.conf

WORKDIR /app
COPY . .

RUN cargo build --release

# Entry point for tests
CMD ["cargo", "test", "--test", "fuse_operations"]
```

**Test Execution:**
```bash
# Build test image
docker build -f Dockerfile.test -t rqbit-fuse-test .

# Run with privileged mode for FUSE
docker run --rm --privileged \
    --device /dev/fuse \
    -v $(pwd):/app \
    rqbit-fuse-test
```

**Benefits:**
- Real FUSE kernel integration
- Isolated environment
- Reproducible across platforms
- CI/CD friendly

**Test Scenarios:**
- Mount/unmount cycles
- File read operations through kernel
- Directory listing via kernel
- Concurrent file access
- Error propagation through FUSE

### 1.3 CI Testing (GitHub Actions)

**Purpose:** Automated testing on every commit/PR

**Workflow Configuration:**
```yaml
# .github/workflows/test.yml
name: Tests

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --lib

  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install FUSE
        run: |
          sudo apt-get update
          sudo apt-get install -y libfuse-dev fuse3
      - run: cargo test --test integration_tests

  fuse-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Setup FUSE
        run: |
          sudo apt-get update
          sudo apt-get install -y libfuse-dev fuse3
          sudo modprobe fuse
          sudo chmod 666 /dev/fuse
      - name: Run FUSE tests
        run: cargo test --test fuse_operations -- --test-threads=1

  property-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - tests: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --test property_tests

  benchmarks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo bench -- --test

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@cargo-tarpaulin
      - uses: Swatinem/rust-cache@v2
      - run: cargo tarpaulin --out xml
      - uses: codecov/codecov-action@v3
        with:
          files: ./cobertura.xml
```

**CI Testing Strategy:**

1. **Fast Feedback (Unit Tests):** Run on every commit
2. **Integration Tests:** Run on PRs and main branch
3. **FUSE Tests:** Run with privileged containers
4. **Property Tests:** Run periodically or on main branch
5. **Benchmarks:** Run on main branch to track performance
6. **Coverage:** Track and report code coverage

### 1.4 Real Filesystem Operation Tests

**Purpose:** Test actual filesystem operations through the kernel

**Test Setup:**
```rust
// tests/fuse_operations.rs
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[tokio::test]
async fn test_real_file_read() {
    let mount_point = TempDir::new().unwrap();
    let mock_server = setup_mock_server().await;
    
    // Start FUSE filesystem in background
    let fs = start_fuse_filesystem(
        mount_point.path(),
        mock_server.uri()
    ).await;
    
    // Wait for mount
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Perform real filesystem operations
    let test_file = mount_point.path().join("test_torrent/test.txt");
    
    // Test file exists
    assert!(test_file.exists());
    
    // Test file read
    let contents = fs::read(&test_file).unwrap();
    assert_eq!(contents, b"Hello from FUSE!");
    
    // Test file metadata
    let metadata = fs::metadata(&test_file).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 18);
    
    // Cleanup
    fs.unmount().await;
}

#[tokio::test]
async fn test_directory_operations() {
    let mount_point = TempDir::new().unwrap();
    
    // Test directory listing
    let entries: Vec<_> = fs::read_dir(mount_point.path())
        .unwrap()
        .collect();
    
    // Verify torrent directories exist
    let names: Vec<_> = entries
        .iter()
        .map(|e| e.as_ref().unwrap().file_name())
        .collect();
    
    assert!(names.contains(&OsString::from("test_torrent")));
}
```

**Test Scenarios:**

1. **File Operations:**
   - Open, read, close files
   - Sequential and random reads
   - Large file reads (>4GB)
   - Empty file handling

2. **Directory Operations:**
   - List directory contents
   - Navigate nested directories
   - Special entries (. and ..)

3. **Error Scenarios:**
   - Non-existent files (ENOENT)
   - Permission errors (EACCES)
   - Network timeouts (EAGAIN)
   - Invalid offsets (EINVAL)

### 1.5 Using fuse_mt for Testing

**Purpose:** Multi-threaded FUSE testing framework

**Note:** The `fuse_mt` crate provides a multi-threaded FUSE filesystem wrapper that can be useful for testing concurrent FUSE operations.

**Alternative Approach:**
Since `fuser` (the crate we use) already supports multi-threaded operation, we can test concurrency directly:

```rust
// tests/concurrent_fuse_tests.rs
use std::sync::Arc;
use std::thread;

#[tokio::test]
async fn test_concurrent_file_reads() {
    let mount_point = TempDir::new().unwrap();
    let fs = start_fuse_filesystem(mount_point.path()).await;
    
    // Spawn multiple threads reading the same file
    let mut handles = vec![];
    for _ in 0..10 {
        let path = mount_point.path().join("test.txt");
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let _ = std::fs::read(&path);
            }
        }));
    }
    
    for h in handles {
        h.join().unwrap();
    }
}
```

## 2. Test Types Needed

### 2.1 Unit Tests

#### Cache Unit Tests

**Location:** `src/cache.rs` (inline `#[cfg(test)]` module)

**Current Coverage:**
- Basic operations (insert, get, remove)
- TTL expiration
- LRU eviction
- Concurrent access
- Statistics tracking

**Additional Tests Needed:**

```rust
#[tokio::test]
async fn test_cache_ttl_boundary() {
    // Test entries at exact TTL boundary
    // Verify race-free expiration
}

#[tokio::test]
async fn test_cache_eviction_under_load() {
    // Test eviction behavior with concurrent inserts
    // Verify LRU ordering is maintained
}

#[tokio::test]
async fn test_cache_statistics_accuracy() {
    // Verify hit/miss counts under various scenarios
    // Test stats don't drift under concurrent access
}
```

#### Inode Unit Tests

**Location:** `src/fs/inode.rs` (inline `#[cfg(test)]` module)

**Current Coverage:**
- Manager creation
- Directory allocation
- File allocation
- Torrent directory allocation
- Path lookup
- Children retrieval
- Inode removal
- Concurrent allocation
- Symlink handling

**Additional Tests Needed:**

```rust
#[test]
fn test_inode_parent_consistency() {
    // Verify parent-child relationships are always consistent
    // Test after various operations (add, remove, move)
}

#[test]
fn test_inode_path_uniqueness() {
    // Verify no two inodes have the same path
    // Test path collision detection
}

#[test]
fn test_inode_torrent_mapping() {
    // Verify torrent_id to inode mapping is correct
    // Test lookup by torrent_id after various operations
}

#[test]
fn test_inode_deep_nesting() {
    // Test deeply nested directory structures (100+ levels)
    // Verify path building doesn't overflow
}
```

#### API Client Unit Tests

**Location:** `src/api/client.rs` (new `#[cfg(test)]` module)

**Tests Needed:**

```rust
#[tokio::test]
async fn test_api_retry_logic() {
    // Test exponential backoff
    // Test max retry limits
    // Test circuit breaker integration
}

#[tokio::test]
async fn test_api_error_mapping() {
    // Test HTTP status to error type mapping
    // Test network error handling
    // Test timeout handling
}

#[tokio::test]
async fn test_api_circuit_breaker() {
    // Test circuit opens after failures
    // Test circuit closes after recovery
    // Test half-open state
}
```

### 2.2 Integration Tests

#### FUSE Operation Tests

**Location:** `tests/fuse_operations.rs` (new file)

**Test Categories:**

```rust
// tests/fuse_operations.rs

//! FUSE filesystem operation tests
//! 
//! These tests verify FUSE callbacks work correctly:
//! - Mount/unmount cycles
//! - File operations (open, read, close)
//! - Directory operations (lookup, readdir)
//! - Error scenarios

use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path};

use rqbit_fuse::{Config, Metrics, TorrentFS};

/// Test mount and unmount cycle
#[tokio::test]
async fn test_mount_unmount_cycle() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = setup_mock_server().await;
    
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
    let metrics = Arc::new(Metrics::new());
    let fs = TorrentFS::new(config, metrics).unwrap();
    
    // Mount filesystem
    let mount_handle = tokio::spawn(async move {
        fs.mount().await
    });
    
    // Wait for mount
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify mount point is accessible
    assert!(temp_dir.path().exists());
    
    // Unmount
    fs.unmount().await.unwrap();
    
    // Wait for unmount
    let result = tokio::time::timeout(Duration::from_secs(5), mount_handle).await;
    assert!(result.is_ok());
}

/// Test file read operations through FUSE
#[tokio::test]
async fn test_fuse_file_read() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = setup_mock_server_with_data().await;
    
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
    let fs = start_fuse_filesystem(config).await;
    
    // Read file through standard filesystem API
    let file_path = temp_dir.path().join("test_torrent/test.txt");
    let contents = fs::read_to_string(&file_path).unwrap();
    
    assert_eq!(contents, "Hello, FUSE!");
    
    fs.unmount().await.unwrap();
}

/// Test directory listing through FUSE
#[tokio::test]
async fn test_fuse_directory_listing() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = setup_mock_server().await;
    
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
    let fs = start_fuse_filesystem(config).await;
    
    // List root directory
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    
    // Verify expected entries exist
    assert!(entries.contains(&OsString::from("test_torrent")));
    
    fs.unmount().await.unwrap();
}

/// Test error scenarios through FUSE
#[tokio::test]
async fn test_fuse_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = setup_mock_server().await;
    
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
    let fs = start_fuse_filesystem(config).await;
    
    // Test non-existent file (should return ENOENT)
    let result = fs::read(temp_dir.path().join("nonexistent"));
    assert!(result.is_err());
    
    // Verify it's a "not found" error
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    
    fs.unmount().await.unwrap();
}
```

#### Cache Integration Tests

**Location:** `tests/cache_tests.rs` (new file)

```rust
// tests/cache_tests.rs

//! Cache integration tests
//!
//! These tests verify cache behavior in realistic scenarios:
//! - TTL expiration under load
//! - LRU eviction with various access patterns
//! - Concurrent access from multiple threads
//! - Statistics accuracy

use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, Instant};

use rqbit_fuse::cache::Cache;

/// Test TTL expiration with concurrent access
#[tokio::test]
async fn test_cache_ttl_concurrent() {
    let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(100, Duration::from_millis(100)));
    
    // Insert entries
    for i in 0..50 {
        cache.insert(format!("key{}", i), i).await;
    }
    
    // Concurrently access while TTL expires
    let mut handles = vec![];
    
    for task_id in 0..5 {
        let cache = Arc::clone(&cache);
        handles.push(tokio::spawn(async move {
            for i in 0..100 {
                let key = format!("key{}", i % 50);
                let _ = cache.get(&key).await;
                sleep(Duration::from_millis(5)).await;
            }
        }));
    }
    
    for h in handles {
        h.await.unwrap();
    }
    
    // Verify stats are consistent
    let stats = cache.stats().await;
    assert!(stats.hits + stats.misses > 0);
}

/// Test LRU eviction under various access patterns
#[tokio::test]
async fn test_cache_lru_access_patterns() {
    let cache: Cache<String, i32> = Cache::new(10, Duration::from_secs(60));
    
    // Pattern 1: Sequential access
    for i in 0..20 {
        cache.insert(format!("seq{}", i), i).await;
    }
    
    // Only last 10 should remain
    assert_eq!(cache.len(), 10);
    assert!(!cache.contains_key(&"seq0".to_string()).await);
    assert!(cache.contains_key(&"seq19".to_string()).await);
    
    // Pattern 2: Hot keys (80/20 rule)
    cache.clear().await;
    
    // Insert 100 entries
    for i in 0..100 {
        cache.insert(format!("key{}", i), i).await;
    }
    
    // Access hot keys (0-19) frequently
    for _ in 0..10 {
        for i in 0..20 {
            let _ = cache.get(&format!("key{}", i)).await;
        }
    }
    
    // Insert more to trigger eviction
    for i in 100..150 {
        cache.insert(format!("key{}", i), i).await;
    }
    
    // Hot keys should still be present
    for i in 0..20 {
        assert!(cache.contains_key(&format!("key{}", i)).await,
            "Hot key {} should not be evicted", i);
    }
}

/// Test cache statistics accuracy under load
#[tokio::test]
async fn test_cache_statistics_accuracy() {
    let cache: Arc<Cache<String, i32>> = Arc::new(Cache::new(100, Duration::from_secs(60)));
    
    let mut expected_hits = 0u64;
    let mut expected_misses = 0u64;
    
    // Insert 50 entries
    for i in 0..50 {
        cache.insert(format!("key{}", i), i).await;
    }
    
    // Perform known operations
    for i in 0..100 {
        let key = format!("key{}", i % 100);
        if let Some(_) = cache.get(&key).await {
            if i % 100 < 50 {
                expected_hits += 1;
            }
        } else {
            if i % 100 >= 50 {
                expected_misses += 1;
            }
        }
    }
    
    let stats = cache.stats().await;
    assert_eq!(stats.hits, expected_hits, "Hit count mismatch");
    assert_eq!(stats.misses, expected_misses, "Miss count mismatch");
}
```

### 2.3 Concurrent Access Tests

**Fixing `test_concurrent_torrent_additions` (TEST-003):**

```rust
// tests/integration_tests.rs - Fixed version

#[tokio::test]
async fn test_concurrent_torrent_additions() {
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let mock_server = setup_mock_server().await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(mock_server.uri(), temp_dir.path().to_path_buf());
    
    let metrics = Arc::new(Metrics::new());
    let fs = Arc::new(TorrentFS::new(config, metrics).unwrap());
    
    use rqbit_fuse::api::types::{FileInfo, TorrentInfo};
    
    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let success_count = Arc::new(AtomicUsize::new(0));
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let fs = Arc::clone(&fs);
        let barrier = Arc::clone(&barrier);
        let success_count = Arc::clone(&success_count);
        
        let handle = tokio::spawn(async move {
            // Wait for all threads to be ready
            barrier.wait();
            
            let torrent_info = TorrentInfo {
                id: 100 + thread_id as u64,
                info_hash: format!("concurrent{}", thread_id),
                name: format!("Torrent {}", thread_id),
                output_folder: "/downloads".to_string(),
                file_count: Some(1),
                files: vec![FileInfo {
                    name: format!("file{}.txt", thread_id),
                    length: 100,
                    components: vec![format!("file{}.txt", thread_id)],
                }],
                piece_length: Some(262144),
            };
            
            match fs.create_torrent_structure(&torrent_info) {
                Ok(_) => {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => {
                    eprintln!("Thread {} failed: {}", thread_id, e);
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for h in handles {
        h.await.unwrap();
    }
    
    // Verify all torrents were added successfully
    let success = success_count.load(Ordering::SeqCst);
    assert_eq!(success, num_threads, 
        "Expected {} successful additions, got {}", num_threads, success);
    
    // Verify all torrents are accessible
    let inode_manager = fs.inode_manager();
    for i in 0..num_threads {
        let torrent_id = 100 + i as u64;
        assert!(inode_manager.lookup_torrent(torrent_id).is_some(),
            "Torrent {} should be accessible after concurrent addition", torrent_id);
    }
}
```

### 2.4 Cache Integration Tests

**Location:** `tests/cache_integration_tests.rs` (new file)

```rust
//! Cache integration tests
//!
//! These tests verify cache behavior in realistic scenarios with the full system.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, Instant};

use rqbit_fuse::cache::Cache;

/// Test cache behavior under memory pressure
#[tokio::test]
async fn test_cache_memory_pressure() {
    let cache: Cache<String, Vec<u8>> = Cache::new(1000, Duration::from_secs(60));
    
    // Insert entries of varying sizes
    for i in 0..1000 {
        let size = if i % 10 == 0 {
            1024 * 1024 // 1MB for every 10th entry
        } else {
            4096 // 4KB for others
        };
        
        let key = format!("key_{}", i);
        let value = vec![0u8; size];
        cache.insert(key, value).await;
    }
    
    // Verify cache maintains size limit
    assert!(cache.len() <= 1000);
    
    // Verify frequently accessed large entries are retained
    for _ in 0..5 {
        let _ = cache.get(&"key_0".to_string()).await;
    }
    
    // Insert more to trigger eviction
    for i in 1000..1100 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 4096];
        cache.insert(key, value).await;
    }
    
    // Frequently accessed entry should still be present
    assert!(cache.contains_key(&"key_0".to_string()).await);
}

/// Test cache recovery after errors
#[tokio::test]
async fn test_cache_error_recovery() {
    let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));
    
    // Populate cache
    for i in 0..50 {
        cache.insert(format!("key{}", i), i).await;
    }
    
    // Simulate error condition by clearing
    cache.clear().await;
    
    // Verify cache is empty
    assert!(cache.is_empty().await);
    
    // Verify cache can be repopulated
    for i in 0..50 {
        cache.insert(format!("key{}", i), i * 2).await;
    }
    
    // Verify new values are present
    for i in 0..50 {
        let value = cache.get(&format!("key{}", i)).await;
        assert_eq!(value, Some(i * 2));
    }
}
```

### 2.5 Mock Verification Tests

**Location:** `tests/mock_verification_tests.rs` (new file)

```rust
//! Mock verification tests
//!
//! These tests verify that WireMock expectations are met and API calls are efficient.

use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

/// Test that API calls are made efficiently (no redundant calls)
#[tokio::test]
async fn test_api_call_efficiency() {
    let mock_server = MockServer::start().await;
    
    // Expect exactly 1 call to /torrents
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "torrents": [{"id": 1, "name": "test"}]
        })))
        .expect(1)  // Exactly 1 call expected
        .mount(&mock_server)
        .await;
    
    // Create client and make operations that should cache
    let client = create_test_client(mock_server.uri());
    
    // Multiple operations that should use cache
    let _ = client.list_torrents().await;
    let _ = client.list_torrents().await;  // Should be cached
    let _ = client.list_torrents().await;  // Should be cached
    
    // Verify mock expectations
    mock_server.verify().await;  // Will fail if more than 1 call made
}

/// Test request patterns and headers
#[tokio::test]
async fn test_api_request_patterns() {
    let mock_server = MockServer::start().await;
    
    // Verify Range header format
    Mock::given(method("GET"))
        .and(path("/torrents/1/stream/0"))
        .and(header("Range", "bytes=0-4095"))
        .respond_with(ResponseTemplate::new(206))
        .expect(1)
        .mount(&mock_server)
        .await;
    
    // Make request and verify
    let client = create_test_client(mock_server.uri());
    let _ = client.read_file_streaming(1, 0, 0, 4096).await;
    
    mock_server.verify().await;
}
```

## 3. Property-Based Testing

### 3.1 Proptest Integration

**Dependencies:**
```toml
[dev-dependencies]
proptest = "1.4"
```

### 3.2 Invariants to Test

#### Inode Table Invariants

**Location:** `tests/property_inode_tests.rs`

```rust
//! Property-based tests for inode table invariants

use proptest::prelude::*;
use rqbit_fuse::fs::inode::InodeManager;
use rqbit_fuse::types::inode::InodeEntry;

proptest! {
    // Invariant: All inodes have unique numbers
    #[test]
    fn prop_inode_uniqueness(operations in inode_operations_strategy()) {
        let manager = InodeManager::new();
        let mut allocated = vec![];
        
        for op in operations {
            match op {
                InodeOp::AllocateFile { name, parent, .. } => {
                    let inode = manager.allocate_file(
                        name, parent, 1, 0, 1024
                    );
                    allocated.push(inode);
                }
                // ... other operations
            }
        }
        
        // Verify all inodes are unique
        let mut sorted = allocated.clone();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(allocated.len(), sorted.len(), "Duplicate inodes found");
    }
    
    // Invariant: Parent-child relationships are consistent
    #[test]
    fn prop_parent_child_consistency(
        entries in prop::collection::vec(inode_entry_strategy(), 1..100)
    ) {
        let manager = InodeManager::new();
        let mut inode_to_parent = std::collections::HashMap::new();
        
        for entry in entries {
            let parent = entry.parent();
            let inode = manager.allocate(entry);
            inode_to_parent.insert(inode, parent);
        }
        
        // Verify each parent knows about its children
        for (inode, parent) in &inode_to_parent {
            if *parent != 1 {  // Skip root
                let children = manager.get_children(*parent);
                let found = children.iter().any(|(ino, _)| ino == inode);
                prop_assert!(found, 
                    "Inode {} has parent {} but parent doesn't list it as child", 
                    inode, parent);
            }
        }
    }
    
    // Invariant: Path lookup is consistent with inode structure
    #[test]
    fn prop_path_lookup_consistency(
        paths in prop::collection::vec(path_strategy(), 1..50)
    ) {
        let manager = InodeManager::new();
        
        // Create directory structure from paths
        for path in &paths {
            create_path_structure(&manager, path);
        }
        
        // Verify each created path can be looked up
        for path in &paths {
            let inode = manager.lookup_by_path(path);
            prop_assert!(inode.is_some(), "Path {} should be lookup-able", path);
            
            // Verify reverse lookup
            let entry = manager.get(inode.unwrap());
            prop_assert!(entry.is_some());
        }
    }
}

// Strategy implementations
fn inode_operations_strategy() -> impl Strategy<Value = Vec<InodeOp>> {
    prop::collection::vec(inode_op_strategy(), 0..100)
}

fn inode_op_strategy() -> impl Strategy<Value = InodeOp> {
    prop_oneof![
        (prop::string::string(), 1u64..100u64, 0u64..1000u64)
            .prop_map(|(name, parent, size)| InodeOp::AllocateFile { name, parent, size }),
        (prop::string::string(), 1u64..100u64)
            .prop_map(|(name, parent)| InodeOp::AllocateDirectory { name, parent }),
    ]
}

#[derive(Debug, Clone)]
enum InodeOp {
    AllocateFile { name: String, parent: u64, size: u64 },
    AllocateDirectory { name: String, parent: u64 },
}
```

#### Cache Consistency Properties

**Location:** `tests/property_cache_tests.rs`

```rust
//! Property-based tests for cache consistency

use proptest::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

use rqbit_fuse::cache::Cache;

proptest! {
    // Invariant: Cache size never exceeds max capacity
    #[test]
    fn prop_cache_size_bounded(
        operations in cache_operations_strategy(1000),
        capacity in 10usize..1000usize
    ) {
        let rt = Runtime::new().unwrap();
        let cache: Cache<String, i32> = Cache::new(capacity, Duration::from_secs(60));
        
        rt.block_on(async {
            for op in operations {
                match op {
                    CacheOp::Insert { key, value } => {
                        cache.insert(key, value).await;
                    }
                    CacheOp::Get { key } => {
                        let _ = cache.get(&key).await;
                    }
                }
            }
            
            let size = cache.len();
            prop_assert!(
                size <= capacity,
                "Cache size {} exceeds capacity {}",
                size, capacity
            );
        });
    }
    
    // Invariant: Retrieved values match inserted values
    #[test]
    fn prop_cache_value_consistency(
        entries in prop::collection::vec(("[a-z]{1,20}", 0i32..1000i32), 1..100)
    ) {
        let rt = Runtime::new().unwrap();
        let cache: Cache<String, i32> = Cache::new(1000, Duration::from_secs(60));
        
        rt.block_on(async {
            // Insert all entries
            for (key, value) in &entries {
                cache.insert(key.clone(), *value).await;
            }
            
            // Verify all retrievals match
            for (key, expected_value) in &entries {
                if let Some(actual_value) = cache.get(key).await {
                    prop_assert_eq!(
                        actual_value, *expected_value,
                        "Value mismatch for key {}: expected {}, got {}",
                        key, expected_value, actual_value
                    );
                }
            }
        });
    }
    
    // Invariant: Statistics are monotonically increasing
    #[test]
    fn prop_cache_stats_monotonic(
        operations in cache_operations_strategy(100)
    ) {
        let rt = Runtime::new().unwrap();
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));
        
        rt.block_on(async {
            let mut prev_hits = 0u64;
            let mut prev_misses = 0u64;
            
            for op in operations {
                match op {
                    CacheOp::Insert { key, value } => {
                        cache.insert(key, value).await;
                    }
                    CacheOp::Get { key } => {
                        let _ = cache.get(&key).await;
                    }
                }
                
                let stats = cache.stats().await;
                prop_assert!(
                    stats.hits >= prev_hits,
                    "Hits decreased: {} -> {}",
                    prev_hits, stats.hits
                );
                prop_assert!(
                    stats.misses >= prev_misses,
                    "Misses decreased: {} -> {}",
                    prev_misses, stats.misses
                );
                
                prev_hits = stats.hits;
                prev_misses = stats.misses;
            }
        });
    }
}

// Strategy implementations
fn cache_operations_strategy(max_ops: usize) -> impl Strategy<Value = Vec<CacheOp>> {
    prop::collection::vec(cache_op_strategy(), 0..max_ops)
}

fn cache_op_strategy() -> impl Strategy<Value = CacheOp> {
    prop_oneof![
        ("[a-z]{1,20}", 0i32..10000i32)
            .prop_map(|(key, value)| CacheOp::Insert { key, value }),
        ("[a-z]{1,20}",)
            .prop_map(|(key,)| CacheOp::Get { key }),
    ]
}

#[derive(Debug, Clone)]
enum CacheOp {
    Insert { key: String, value: i32 },
    Get { key: String },
}
```

#### Path Resolution Properties

**Location:** `tests/property_path_tests.rs`

```rust
//! Property-based tests for path resolution

use proptest::prelude::*;

proptest! {
    // Invariant: Path resolution is deterministic
    #[test]
    fn prop_path_resolution_deterministic(
        paths in prop::collection::vec(valid_path_strategy(), 1..20)
    ) {
        let manager = InodeManager::new();
        
        // Create structure
        for path in &paths {
            create_path_structure(&manager, path);
        }
        
        // Resolve each path multiple times
        for path in &paths {
            let inode1 = manager.lookup_by_path(path);
            let inode2 = manager.lookup_by_path(path);
            let inode3 = manager.lookup_by_path(path);
            
            prop_assert_eq!(inode1, inode2, "Path resolution not deterministic for {}", path);
            prop_assert_eq!(inode2, inode3, "Path resolution not deterministic for {}", path);
        }
    }
    
    // Invariant: Path components are valid
    #[test]
    fn prop_path_components_valid(
        components in prop::collection::vec("[a-zA-Z0-9_]{1,50}", 0..10)
    ) {
        let path = components.join("/");
        
        // Verify path doesn't contain invalid sequences
        prop_assert!(!path.contains("//"), "Path contains double slash: {}", path);
        prop_assert!(!path.contains("/./"), "Path contains /./: {}", path);
        prop_assert!(!path.contains("/../"), "Path contains /../: {}", path);
    }
}

fn valid_path_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-zA-Z0-9_]{1,30}", 1..5)
        .prop_map(|components| format!("/{}", components.join("/")))
}
```

## 4. Testing Infrastructure

### 4.1 WireMock Setup for API Mocking

**Location:** `tests/common/mock_server.rs` (new file)

```rust
//! Shared mock server utilities for tests

use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header, body_json};
use serde_json::json;

/// Standard mock server setup with common endpoints
pub async fn setup_mock_server() -> MockServer {
    let mock_server = MockServer::start().await;
    
    // Health check endpoint
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "torrents": []
        })))
        .mount(&mock_server)
        .await;
    
    mock_server
}

/// Mock server with torrent data
pub async fn setup_mock_server_with_torrents() -> MockServer {
    let mock_server = MockServer::start().await;
    
    // Torrent list
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "torrents": [
                {"id": 1, "info_hash": "abc123", "name": "Test Torrent"}
            ]
        })))
        .mount(&mock_server)
        .await;
    
    // Torrent details
    Mock::given(method("GET"))
        .and(path("/torrents/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "info_hash": "abc123",
            "name": "Test Torrent",
            "output_folder": "/downloads",
            "file_count": 1,
            "files": [
                {"name": "test.txt", "length": 1024, "components": ["test.txt"]}
            ],
            "piece_length": 1048576
        })))
        .mount(&mock_server)
        .await;
    
    mock_server
}

/// Mock server with streaming data
pub async fn setup_mock_server_with_data() -> MockServer {
    let mock_server = setup_mock_server_with_torrents().await;
    
    // File streaming endpoint
    Mock::given(method("GET"))
        .and(path("/torrents/1/stream/0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hello from FUSE!"))
        .mount(&mock_server)
        .await;
    
    mock_server
}

/// Helper to create test configuration
pub fn create_test_config(mock_uri: String, mount_point: std::path::PathBuf) -> rqbit_fuse::Config {
    let mut config = rqbit_fuse::Config::default();
    config.api.url = mock_uri;
    config.mount.mount_point = mount_point;
    config.mount.allow_other = false;
    config
}
```

### 4.2 FUSE Mount in Tests

**Location:** `tests/common/fuse_helpers.rs` (new file)

```rust
//! Helper functions for FUSE testing

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

use rqbit_fuse::{Config, Metrics, TorrentFS};

/// Test filesystem wrapper that handles lifecycle
pub struct TestFilesystem {
    fs: Arc<TorrentFS>,
    mount_point: TempDir,
    mount_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestFilesystem {
    /// Create and mount a test filesystem
    pub async fn new(mock_uri: String) -> anyhow::Result<Self> {
        let mount_point = TempDir::new()?;
        let mut config = Config::default();
        config.api.url = mock_uri;
        config.mount.mount_point = mount_point.path().to_path_buf();
        
        let metrics = Arc::new(Metrics::new());
        let fs = Arc::new(TorrentFS::new(config, metrics)?);
        
        // Start mount in background
        let fs_clone = Arc::clone(&fs);
        let mount_handle = tokio::spawn(async move {
            let _ = fs_clone.mount().await;
        });
        
        // Wait for mount to be ready
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        Ok(Self {
            fs,
            mount_point,
            mount_handle: Some(mount_handle),
        })
    }
    
    /// Get the mount point path
    pub fn mount_point(&self) -> &Path {
        self.mount_point.path()
    }
    
    /// Unmount the filesystem
    pub async fn unmount(mut self) -> anyhow::Result<()> {
        // Trigger unmount
        self.fs.unmount().await?;
        
        // Wait for mount task to complete
        if let Some(handle) = self.mount_handle.take() {
            timeout(Duration::from_secs(5), handle).await??;
        }
        
        Ok(())
    }
}

/// Helper to wait for filesystem to be ready
pub async fn wait_for_mount(mount_point: &Path, timeout_secs: u64) -> anyhow::Result<()> {
    timeout(Duration::from_secs(timeout_secs), async {
        loop {
            if mount_point.exists() && mount_point.read_dir().is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await?
}
```

## 5. Test File Organization

### 5.1 Proposed Test Structure

```
rqbit-fuse/
├── tests/
│   ├── common/                      # Shared test utilities
│   │   ├── mod.rs
│   │   ├── mock_server.rs          # WireMock helpers
│   │   └── fuse_helpers.rs          # FUSE test utilities
│   ├── integration_tests.rs         # Current integration tests
│   ├── performance_tests.rs         # Current performance tests
│   ├── fuse_operations.rs           # NEW: Real FUSE operation tests
│   ├── cache_tests.rs               # NEW: Cache integration tests
│   ├── concurrent_tests.rs          # NEW: Concurrent access tests
│   ├── mock_verification_tests.rs   # NEW: WireMock verification tests
│   ├── property_inode_tests.rs      # NEW: Property-based inode tests
│   ├── property_cache_tests.rs      # NEW: Property-based cache tests
│   └── property_path_tests.rs       # NEW: Property-based path tests
├── benches/
│   └── performance.rs               # Current benchmarks
└── src/
    └── ...                          # Source files with inline unit tests
```

### 5.2 Test Categories Summary

| Category | File | Purpose | Priority |
|----------|------|---------|----------|
| **Unit Tests** | Inline in source | Fast, isolated component tests | High |
| **Integration Tests** | `tests/integration_tests.rs` | Component interaction tests | High |
| **FUSE Operations** | `tests/fuse_operations.rs` | Real FUSE filesystem tests | High |
| **Cache Tests** | `tests/cache_tests.rs` | Cache behavior verification | Medium |
| **Concurrent Tests** | `tests/concurrent_tests.rs` | Race condition detection | High |
| **Mock Verification** | `tests/mock_verification_tests.rs` | API call efficiency | Medium |
| **Property Tests** | `tests/property_*_tests.rs` | Invariant verification | Medium |
| **Performance Tests** | `tests/performance_tests.rs` | Load and stress tests | Low |
| **Benchmarks** | `benches/performance.rs` | Performance regression | Low |

## 6. Running Tests

### 6.1 Test Commands

```bash
# Run all tests
cargo test

# Run only unit tests (fast)
cargo test --lib

# Run integration tests
cargo test --test integration_tests

# Run specific test
cargo test test_concurrent_torrent_additions

# Run with output
cargo test -- --nocapture

# Run benchmarks (as tests)
cargo bench -- --test

# Run with coverage
cargo tarpaulin --out html

# Run FUSE tests (requires privileges)
sudo cargo test --test fuse_operations

# Run Docker-based tests
docker build -f Dockerfile.test -t rqbit-fuse-test .
docker run --rm --privileged rqbit-fuse-test
```

### 6.2 Test Environment Variables

```bash
# Control test behavior
TORRENT_FUSE_TEST_TIMEOUT=30      # Test timeout in seconds
TORRENT_FUSE_TEST_VERBOSE=1       # Enable verbose output
TORRENT_FUSE_TEST_SKIP_FUSE=1     # Skip FUSE tests (no privileges)
TORRENT_FUSE_TEST_KEEP_MOUNTS=1   # Don't clean up mount points
```

## 7. Test Data and Fixtures

### 7.1 Test Torrent Fixtures

```rust
// tests/common/fixtures.rs

use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

/// Single file torrent fixture
pub fn single_file_torrent() -> TorrentInfo {
    TorrentInfo {
        id: 1,
        info_hash: "abc123".to_string(),
        name: "Single File".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "file.txt".to_string(),
            length: 1024,
            components: vec!["file.txt".to_string()],
        }],
        piece_length: Some(262144),
    }
}

/// Multi-file torrent fixture
pub fn multi_file_torrent() -> TorrentInfo {
    TorrentInfo {
        id: 2,
        info_hash: "def456".to_string(),
        name: "Multi File".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "readme.txt".to_string(),
                length: 100,
                components: vec!["readme.txt".to_string()],
            },
            FileInfo {
                name: "data.bin".to_string(),
                length: 1024000,
                components: vec!["subdir".to_string(), "data.bin".to_string()],
            },
            FileInfo {
                name: "info.txt".to_string(),
                length: 200,
                components: vec!["subdir".to_string(), "info.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    }
}

/// Deeply nested torrent fixture
pub fn deeply_nested_torrent(depth: usize) -> TorrentInfo {
    let mut files = vec![];
    let mut current_path = vec![];
    
    for i in 0..depth {
        current_path.push(format!("level{}", i));
        files.push(FileInfo {
            name: format!("file{}.txt", i),
            length: 100,
            components: current_path.clone(),
        });
    }
    
    TorrentInfo {
        id: depth as u64,
        info_hash: format!("nested{}", depth),
        name: format!("Nested {} levels", depth),
        output_folder: "/downloads".to_string(),
        file_count: Some(files.len() as u32),
        files,
        piece_length: Some(262144),
    }
}

/// Unicode torrent fixture
pub fn unicode_torrent() -> TorrentInfo {
    TorrentInfo {
        id: 100,
        info_hash: "unicode".to_string(),
        name: "Unicode Test 🎉".to_string(),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "中文文件.txt".to_string(),
                length: 100,
                components: vec!["中文文件.txt".to_string()],
            },
            FileInfo {
                name: "日本語ファイル.txt".to_string(),
                length: 200,
                components: vec!["日本語ファイル.txt".to_string()],
            },
            FileInfo {
                name: "файл.txt".to_string(),
                length: 300,
                components: vec!["файл.txt".to_string()],
            },
            FileInfo {
                name: "emoji_🎊_file.txt".to_string(),
                length: 400,
                components: vec!["emoji_🎊_file.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    }
}
```

## 8. Continuous Integration

### 8.1 GitHub Actions Workflow

See Section 1.3 for the complete CI workflow configuration.

### 8.2 Pre-commit Hooks

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: cargo-test
        name: Run Rust tests
        entry: cargo test --lib
        language: system
        pass_filenames: false
        
      - id: cargo-clippy
        name: Run Clippy
        entry: cargo clippy -- -D warnings
        language: system
        pass_filenames: false
        
      - id: cargo-fmt
        name: Check formatting
        entry: cargo fmt -- --check
        language: system
        pass_filenames: false
```

## 9. Test Maintenance

### 9.1 Adding New Tests

When adding new functionality:

1. **Unit tests first:** Add tests in the source file's `#[cfg(test)]` module
2. **Integration tests:** Add to appropriate `tests/*.rs` file
3. **Property tests:** If invariants can be defined, add property-based tests
4. **Documentation:** Update this spec if new test patterns are introduced

### 9.2 Test Naming Conventions

- `test_<component>_<scenario>` for unit tests
- `test_<feature>_<condition>` for integration tests
- `prop_<invariant>_<property>` for property tests
- `bench_<operation>_<metric>` for benchmarks

### 9.3 Test Isolation

All tests must be isolated:
- Use `TempDir` for filesystem operations
- Use unique mock server ports
- Clean up resources in `Drop` or explicitly
- Don't rely on test execution order

## 10. Summary

This testing specification provides a comprehensive approach to ensuring rqbit-fuse correctness and reliability:

1. **Multiple Testing Layers:** Unit, integration, property-based, and performance tests
2. **FUSE-Specific Testing:** Mock, Docker, and real filesystem approaches
3. **Concurrent Testing:** Proper synchronization and race condition detection
4. **CI/CD Integration:** Automated testing on every commit
5. **Maintainable:** Clear organization and documentation

**Priority Implementation Order:**
1. Fix `test_concurrent_torrent_additions` (TEST-003)
2. Create `tests/fuse_operations.rs` with basic FUSE tests (TEST-002)
3. Add cache integration tests (TEST-004)
4. Add mock verification tests (TEST-005)
5. Implement property-based tests (TEST-006, TEST-007)

---

*Last updated: 2024-02-14*
*Related: TODO.md TEST-001 through TEST-007*
