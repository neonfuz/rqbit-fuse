# Async FUSE Integration Specification

## Overview

This document addresses the critical issue of async-in-sync patterns in FUSE callbacks within the torrent-fuse project. FUSE callbacks are inherently synchronous (they must return immediately), but the project needs to perform async I/O operations (HTTP requests to rqbit). This document analyzes the current dangerous patterns and provides a comprehensive solution.

**Status**: Draft  
**Related Issues**: FS-001, FS-002  
**Priority**: Critical (must fix before production)

---

## 1. Current Async-in-Sync Problem

### 1.1 Problematic Code Locations

The dangerous `block_in_place` + `block_on` pattern exists in two locations:

#### Location 1: `read()` callback (filesystem.rs:931-947)

```rust
// Perform the read using persistent streaming for efficient sequential access
let result = tokio::task::block_in_place(|| {
    tokio::runtime::Handle::current().block_on(async {
        // Set a timeout to avoid blocking forever on slow pieces
        let timeout_duration =
            std::time::Duration::from_secs(self.config.performance.read_timeout);
        tokio::time::timeout(
            timeout_duration,
            self.api_client.read_file_streaming(
                torrent_id,
                file_index,
                offset,
                size as usize,
            ),
        )
        .await
    })
});
```

#### Location 2: `remove_torrent()` (filesystem.rs:2194-2198)

```rust
// Remove from rqbit (forget - keeps downloaded files)
tokio::task::block_in_place(|| {
    tokio::runtime::Handle::current()
        .block_on(async { self.api_client.forget_torrent(torrent_id).await })
})
.with_context(|| format!("Failed to remove torrent {} from rqbit", torrent_id))?;
```

### 1.2 Why This Pattern is Dangerous

The combination of `block_in_place` + `block_on` creates several deadlock risks:

#### Risk 1: Thread Pool Exhaustion

`block_in_place` moves the current task to a blocking thread pool, but:
- It requires a handle to the current runtime
- If the runtime is under heavy load, all threads may be occupied
- The blocking thread pool has a limited size (default 512 threads)
- When saturated, `block_in_place` will wait for a free thread, potentially indefinitely

#### Risk 2: Nested Executor Problem

Using `block_on` inside `block_in_place` creates a nested executor scenario:
- `block_in_place` enters blocking context
- `block_on` tries to enter a new async context
- This can cause panic or deadlock when the runtime detects nested execution
- The Tokio runtime does not support nested `block_on` calls

#### Risk 3: FUSE Timeout Deadlocks

FUSE has internal timeouts for operations:
- Default FUSE operation timeout is typically 30 seconds
- If `block_in_place` waits for a thread + `block_on` waits for async operation, total time can exceed FUSE timeout
- Kernel may abort the operation while the thread is still blocked
- Leaves the system in an inconsistent state

#### Risk 4: Resource Leak on Cancellation

When FUSE times out:
- The kernel sends an interrupt (FORGET)
- But the blocked thread continues executing
- HTTP connections may be left open
- File handles may not be properly cleaned up
- Memory leaks accumulate over time

### 1.3 Specific FUSE Callback Context

FUSE callbacks run in a special context that makes async patterns particularly dangerous:

1. **Kernel Synchronous Expectations**: The kernel expects FUSE callbacks to return immediately or block briefly. Long blocking operations cause:
   - Process stalls (applications reading files hang)
   - Mount point becoming unresponsive
   - Potential kernel panic on timeout

2. **Reentrancy Risks**: Some FUSE operations can be reentrant:
   - Reading a file may trigger a lookup for the same file
   - Directory operations may cascade
   - Nested `block_on` calls can occur unintentionally

3. **Thread Safety**: FUSE may call operations concurrently:
   - Multiple reads on different files
   - Lookup while read is in progress
   - Current pattern doesn't handle concurrent cancellation well

---

## 2. Alternative Approaches Analysis

### 2.1 Approach A: Spawn Tasks with Channels (RECOMMENDED)

**Concept**: Spawn async tasks and communicate results through channels.

**Implementation**:
- Create async task pool for I/O operations
- Use mpsc or oneshot channels for request/response
- FUSE callback sends request, waits on channel receive
- Channel receive has timeout to prevent indefinite blocking

**Pros**:
- Clean separation between sync and async contexts
- Proper cancellation handling
- No nested executors
- Scalable with proper backpressure

**Cons**:
- Additional complexity
- Channel overhead
- Need to manage task lifecycle

**Safety Rating**: ★★★★★

### 2.2 Approach B: Use `fuser` Async Support

**Concept**: Check if `fuser` crate provides async callback support.

**Investigation**:
The `fuser` crate (as of v0.14) provides a `Filesystem` trait with synchronous callbacks only. However, there are alternatives:

- **`fuser-async`**: Third-party crate that wraps `fuser` with async support
- **`polyfuse`**: Alternative FUSE library with native async support
- **`fuser` + `async-trait`**: Doesn't solve the fundamental problem

**Pros**:
- Native async integration
- Cleaner code

**Cons**:
- Requires switching FUSE libraries (major refactoring)
- `polyfuse` may have different API
- Testing needed for stability

**Safety Rating**: ★★★★☆ (requires migration)

### 2.3 Approach C: Restructure to Avoid Async-in-Sync

**Concept**: Move all async work outside FUSE callbacks.

**Implementation**:
- Pre-fetch and cache all data before FUSE operations
- Use synchronous I/O within callbacks
- Async work only in background tasks

**Pros**:
- Simple synchronous callbacks
- No async complexity in FUSE path

**Cons**:
- Not feasible for on-demand torrent reading
- Would require downloading entire torrents upfront
- Defeats the purpose of FUSE streaming

**Safety Rating**: ★☆☆☆☆ (not viable for this use case)

### 2.4 Approach D: Thread-Per-Request Model

**Concept**: Spawn a dedicated thread for each FUSE request that needs async I/O.

**Implementation**:
- Create thread pool for FUSE operations
- Each `read()` spawns thread that runs its own Tokio runtime
- Thread executes async work and returns result

**Pros**:
- Isolation between requests
- Simple mental model

**Cons**:
- Heavy resource usage (threads are expensive)
- Slow (thread creation overhead)
- No connection reuse
- Can overwhelm system under load

**Safety Rating**: ★★☆☆☆ (inefficient)

---

## 3. Recommended Solution: Task Spawn + Channel Pattern

### 3.1 Architecture Overview

```
┌─────────────────┐
│   FUSE Kernel   │
│   Callbacks     │
└────────┬────────┘
         │ sync
         ▼
┌─────────────────┐
│  FUSE Callback  │
│  (sync context) │
└────────┬────────┘
         │ send request
         ▼
┌─────────────────┐
│   Request Queue │
│   (mpsc channel)│
└────────┬────────┘
         │ recv
         ▼
┌─────────────────┐
│  Async Worker   │
│  Pool (tokio)   │
└────────┬────────┘
         │ async HTTP
         ▼
┌─────────────────┐
│   Rqbit API     │
│   (HTTP)        │
└─────────────────┘
```

### 3.2 Core Components

#### Component 1: Request/Response Types

```rust
/// Request sent from FUSE callback to async worker
#[derive(Debug)]
pub enum FuseRequest {
    ReadFile {
        torrent_id: u64,
        file_index: usize,
        offset: u64,
        size: usize,
        timeout: Duration,
        response_tx: oneshot::Sender<FuseResponse>,
    },
    ForgetTorrent {
        torrent_id: u64,
        response_tx: oneshot::Sender<FuseResponse>,
    },
}

/// Response from async worker to FUSE callback
#[derive(Debug)]
pub enum FuseResponse {
    ReadSuccess { data: Vec<u8> },
    ReadError { error_code: i32, message: String },
    ForgetSuccess,
    ForgetError { error: String },
}

/// Error types for FUSE operations
#[derive(Debug, Clone)]
pub enum FuseError {
    NotFound,
    PermissionDenied,
    TimedOut,
    IoError(String),
    NotReady,
}

impl FuseError {
    pub fn to_errno(&self) -> i32 {
        match self {
            FuseError::NotFound => libc::ENOENT,
            FuseError::PermissionDenied => libc::EACCES,
            FuseError::TimedOut => libc::ETIMEDOUT,
            FuseError::IoError(_) => libc::EIO,
            FuseError::NotReady => libc::EAGAIN,
        }
    }
}
```

#### Component 2: Async Worker Pool

```rust
/// Manages async operations for FUSE filesystem
pub struct AsyncFuseWorker {
    request_tx: mpsc::Sender<FuseRequest>,
    worker_handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AsyncFuseWorker {
    pub fn new(api_client: Arc<RqbitClient>, config: Config) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<FuseRequest>(100);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        
        let worker_handle = tokio::spawn(async move {
            let api_client = api_client;
            
            loop {
                tokio::select! {
                    biased;
                    
                    // Handle shutdown signal
                    _ = &mut shutdown_rx => {
                        info!("AsyncFuseWorker shutting down");
                        break;
                    }
                    
                    // Handle incoming requests
                    Some(request) = request_rx.recv() => {
                        Self::handle_request(&api_client, request).await;
                    }
                }
            }
        });
        
        Self {
            request_tx,
            worker_handle: Some(worker_handle),
            shutdown_tx: Some(shutdown_tx),
        }
    }
    
    async fn handle_request(api_client: &Arc<RqbitClient>, request: FuseRequest) {
        match request {
            FuseRequest::ReadFile { 
                torrent_id, 
                file_index, 
                offset, 
                size, 
                timeout,
                response_tx 
            } => {
                let result = tokio::time::timeout(
                    timeout,
                    api_client.read_file_streaming(torrent_id, file_index, offset, size)
                ).await;
                
                let response = match result {
                    Ok(Ok(data)) => FuseResponse::ReadSuccess { data },
                    Ok(Err(e)) => {
                        let error_code = if e.to_string().contains("not found") {
                            libc::ENOENT
                        } else {
                            libc::EIO
                        };
                        FuseResponse::ReadError { 
                            error_code, 
                            message: e.to_string() 
                        }
                    }
                    Err(_) => FuseResponse::ReadError {
                        error_code: libc::ETIMEDOUT,
                        message: "Operation timed out".to_string(),
                    }
                };
                
                // Ignore send failure (receiver dropped = FUSE timeout)
                let _ = response_tx.send(response);
            }
            
            FuseRequest::ForgetTorrent { torrent_id, response_tx } => {
                let result = api_client.forget_torrent(torrent_id).await;
                
                let response = match result {
                    Ok(_) => FuseResponse::ForgetSuccess,
                    Err(e) => FuseResponse::ForgetError { error: e.to_string() },
                };
                
                let _ = response_tx.send(response);
            }
        }
    }
    
    /// Send a request and wait for response (blocking, with timeout)
    pub fn send_request(&self, request: FuseRequest, timeout: Duration) -> Result<FuseResponse, FuseError> {
        let (tx, rx) = oneshot::channel();
        
        // Create request with response channel
        let request = match request {
            FuseRequest::ReadFile { torrent_id, file_index, offset, size, timeout, .. } => {
                FuseRequest::ReadFile {
                    torrent_id,
                    file_index,
                    offset,
                    size,
                    timeout,
                    response_tx: tx,
                }
            }
            FuseRequest::ForgetTorrent { torrent_id, .. } => {
                FuseRequest::ForgetTorrent { torrent_id, response_tx: tx }
            }
        };
        
        // Send request to worker
        self.request_tx
            .try_send(request)
            .map_err(|_| FuseError::IoError("Request channel full".to_string()))?;
        
        // Wait for response with timeout
        match rx.recv_timeout(timeout) {
            Ok(response) => Ok(response),
            Err(RecvTimeoutError::Timeout) => Err(FuseError::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(FuseError::IoError("Worker disconnected".to_string())),
        }
    }
    
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
```

#### Component 3: Safe FUSE Callback Implementation

```rust
impl Filesystem for TorrentFS {
    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        // ... validation code remains the same ...
        
        // Send request to async worker
        let request = FuseRequest::ReadFile {
            torrent_id,
            file_index,
            offset: offset as u64,
            size: size as usize,
            timeout: Duration::from_secs(self.config.performance.read_timeout),
            response_tx: (), // Will be created in send_request
        };
        
        match self.async_worker.send_request(request, Duration::from_secs(
            self.config.performance.read_timeout + 5 // Add buffer for channel overhead
        )) {
            Ok(FuseResponse::ReadSuccess { data }) => {
                reply.data(&data);
            }
            Ok(FuseResponse::ReadError { error_code, message }) => {
                error!("Read error for ino {}: {}", ino, message);
                reply.error(error_code);
            }
            Err(FuseError::TimedOut) => {
                warn!("Read timeout for ino {}", ino);
                reply.error(libc::ETIMEDOUT);
            }
            Err(e) => {
                error!("Unexpected error reading ino {}: {:?}", ino, e);
                reply.error(libc::EIO);
            }
        }
    }
}
```

### 3.3 Thread Safety and Concurrency

#### Thread-Safe Architecture

```rust
/// Thread-safe handle for use in FUSE callbacks
pub struct FuseAsyncHandle {
    request_tx: mpsc::Sender<FuseRequest>,
    runtime_handle: tokio::runtime::Handle,
}

impl FuseAsyncHandle {
    /// Execute async operation synchronously with proper timeout
    pub fn execute_blocking<F, T>(&self, operation: F, timeout: Duration) -> Result<T, FuseError>
    where
        F: Future<Output = Result<T, anyhow::Error>> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = self.runtime_handle.clone();
        
        // Spawn the async operation on the runtime
        handle.spawn(async move {
            let result = tokio::time::timeout(timeout, operation).await;
            let _ = tx.send(result);
        });
        
        // Block this thread waiting for result (but NOT with block_on!)
        match rx.recv_timeout(timeout + Duration::from_secs(1)) {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(e))) => Err(FuseError::IoError(e.to_string())),
            Ok(Err(_)) => Err(FuseError::TimedOut),
            Err(_) => Err(FuseError::TimedOut),
        }
    }
}
```

#### Concurrency Limits

```rust
/// Configuration for async worker pool
#[derive(Debug, Clone)]
pub struct AsyncWorkerConfig {
    /// Maximum concurrent read operations
    pub max_concurrent_reads: usize,
    /// Maximum concurrent forget operations  
    pub max_concurrent_forgets: usize,
    /// Channel buffer size
    pub channel_buffer_size: usize,
    /// Thread pool size for blocking operations
    pub thread_pool_size: usize,
}

impl Default for AsyncWorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_reads: 100,
            max_concurrent_forgets: 10,
            channel_buffer_size: 1000,
            thread_pool_size: 8,
        }
    }
}
```

### 3.4 Error Handling Strategy

#### FUSE Error Mapping

```rust
/// Convert internal errors to FUSE error codes
pub trait ToFuseError {
    fn to_fuse_error(&self) -> i32;
}

impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types
        if let Some(api_err) = self.downcast_ref::<ApiError>() {
            return api_err.to_fuse_error();
        }
        
        // String matching (temporary, should use typed errors)
        let err_str = self.to_string().to_lowercase();
        if err_str.contains("not found") {
            libc::ENOENT
        } else if err_str.contains("permission") || err_str.contains("access") {
            libc::EACCES
        } else if err_str.contains("timeout") {
            libc::ETIMEDOUT
        } else if err_str.contains("range") {
            libc::EINVAL
        } else {
            libc::EIO
        }
    }
}

impl ToFuseError for ApiError {
    fn to_fuse_error(&self) -> i32 {
        match self {
            ApiError::NotFound => libc::ENOENT,
            ApiError::Unauthorized => libc::EACCES,
            ApiError::BadRequest => libc::EINVAL,
            ApiError::ServerError(_) => libc::EIO,
            ApiError::NetworkError(_) => libc::EIO,
            ApiError::Timeout => libc::ETIMEDOUT,
        }
    }
}
```

---

## 4. Implementation Plan

### Phase 1: Foundation (1-2 days)

#### 4.1 Files to Create

1. **`src/fs/async_bridge.rs`** - Async/sync bridge module
   - `FuseRequest` / `FuseResponse` enums
   - `AsyncFuseWorker` struct
   - `FuseAsyncHandle` for thread-safe access

2. **`src/fs/error.rs`** - Error types
   - `FuseError` enum
   - `ToFuseError` trait
   - Error conversion implementations

#### 4.2 Files to Modify

1. **`src/fs/filesystem.rs`**
   - Remove `block_in_place` + `block_on` patterns
   - Add `async_worker` field to `TorrentFS`
   - Update `read()` callback to use channel pattern
   - Update `remove_torrent()` to use channel pattern

2. **`src/fs/mod.rs`**
   - Add `async_bridge` and `error` modules

3. **`src/main.rs`** (or wherever FUSE is mounted)
   - Initialize async worker before mounting
   - Pass handle to TorrentFS

### Phase 2: Core Implementation (2-3 days)

#### Step-by-Step Migration

1. **Create Error Types** (Day 1 morning)
   ```rust
   // src/fs/error.rs
   #[derive(Debug, Clone)]
   pub enum FuseError {
       NotFound,
       PermissionDenied,
       TimedOut,
       IoError(String),
       NotReady,
       ChannelFull,
       WorkerDisconnected,
   }
   ```

2. **Create Async Bridge** (Day 1 afternoon)
   - Implement `AsyncFuseWorker`
   - Add request/response handling
   - Implement timeout logic

3. **Migrate `read()` callback** (Day 2)
   - Replace blocking pattern with channel send
   - Add proper error handling
   - Test with simple reads

4. **Migrate `remove_torrent()`** (Day 2)
   - Similar migration for forget operation
   - Ensure cleanup happens correctly

5. **Integration Testing** (Day 3)
   - End-to-end mount test
   - Concurrent read test
   - Timeout scenario test

### Phase 3: Testing & Validation (2 days)

#### 4.3 Test Plan

1. **Unit Tests** (`tests/async_bridge_tests.rs`)
   - Request/response serialization
   - Timeout behavior
   - Error propagation

2. **Integration Tests** (`tests/fuse_async_tests.rs`)
   - Mount and read single file
   - Concurrent reads (10+ simultaneous)
   - Timeout recovery
   - Torrent removal during read

3. **Stress Tests**
   ```rust
   #[tokio::test]
   async fn test_concurrent_reads_no_deadlock() {
       // Spawn 100 concurrent reads
       // Verify all complete without deadlock
       // Check no thread panics
   }
   ```

4. **Error Scenario Tests**
   - Network failure during read
   - Rqbit API unavailable
   - FUSE timeout handling
   - Partial data scenarios

---

## 5. Migration Guide

### 5.1 From Current Pattern to New Pattern

#### Before (filesystem.rs:931-947):

```rust
let result = tokio::task::block_in_place(|| {
    tokio::runtime::Handle::current().block_on(async {
        let timeout_duration =
            std::time::Duration::from_secs(self.config.performance.read_timeout);
        tokio::time::timeout(
            timeout_duration,
            self.api_client.read_file_streaming(
                torrent_id,
                file_index,
                offset,
                size as usize,
            ),
        )
        .await
    })
});

match result {
    Ok(Ok(data)) => { /* ... */ }
    Ok(Err(e)) => { /* ... */ }
    Err(_) => { /* timeout */ }
}
```

#### After:

```rust
// Create request with response channel
let (tx, rx) = oneshot::channel();

let request = FuseRequest::ReadFile {
    torrent_id,
    file_index,
    offset: offset as u64,
    size: size as usize,
    timeout: Duration::from_secs(self.config.performance.read_timeout),
    response_tx: tx,
};

// Send to async worker (non-blocking send)
if let Err(_) = self.async_worker.send(request).await {
    reply.error(libc::EIO);
    return;
}

// Wait for response (with timeout)
match rx.recv_timeout(Duration::from_secs(35)) {
    Ok(FuseResponse::ReadSuccess { data }) => {
        reply.data(&data);
    }
    Ok(FuseResponse::ReadError { error_code, .. }) => {
        reply.error(error_code);
    }
    Err(_) => {
        reply.error(libc::ETIMEDOUT);
    }
}
```

### 5.2 State Management Migration

#### Current State Fields (filesystem.rs:73-83):

```rust
pub struct TorrentFS {
    // ... other fields ...
    read_states: Arc<Mutex<HashMap<u64, ReadState>>>,
    monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    discovery_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    last_discovery: Arc<AtomicU64>, // ms since Unix epoch for atomic check-and-set
}
```

#### Recommended Changes:

1. **Replace std::sync::Mutex with tokio::sync::Mutex** (as noted in FS-005):
   ```rust
   read_states: Arc<tokio::sync::Mutex<HashMap<u64, ReadState>>>,
   ```

2. **Add async worker field**:
   ```rust
   async_worker: Arc<AsyncFuseWorker>,
   ```

3. **Keep background task handles** (already spawned in async context):
   ```rust
   monitor_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
   ```

4. **Use AtomicU64 for last_discovery** (fixed FS-008):
   Changed from `Arc<Mutex<Instant>>` to `Arc<AtomicU64>` storing milliseconds since Unix epoch.
   This enables lock-free atomic check-and-set operations to prevent race conditions where
   multiple concurrent `readdir()` calls could all pass the cooldown check before any
   updated the timestamp. Now uses `compare_exchange` to atomically verify cooldown and
   claim the discovery slot, ensuring only one task proceeds even with concurrent calls.

### 5.3 Runtime Integration

#### Current Mount Flow (main.rs):

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let fs = TorrentFS::new(config)?;
    
    // ... initialization ...
    
    // This blocks the runtime!
    fs.mount()?;
    
    Ok(())
}
```

#### Recommended Mount Flow:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let rt = tokio::runtime::Handle::current();
    let metrics = Arc::new(Metrics::new());
    
    // Create async worker (needs runtime handle)
    let async_worker = Arc::new(AsyncFuseWorker::new(
        api_client.clone(),
        config.clone(),
        rt,
    ));
    
    let fs = TorrentFS::new(config, metrics, async_worker)?;
    
    // ... initialization ...
    
    // Spawn FUSE on a blocking thread
    let mount_handle = tokio::task::spawn_blocking(move || {
        fs.mount()
    });
    
    // Keep main task alive
    mount_handle.await??;
    
    Ok(())
}
```

---

## 6. Testing Strategy

### 6.1 Unit Tests for Async Bridge

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_async_worker_request_response() {
        let worker = AsyncFuseWorker::new(mock_client());
        
        let (tx, rx) = oneshot::channel();
        let request = FuseRequest::ReadFile {
            torrent_id: 1,
            file_index: 0,
            offset: 0,
            size: 1024,
            timeout: Duration::from_secs(5),
            response_tx: tx,
        };
        
        // Send request
        worker.send(request).await.unwrap();
        
        // Should receive response
        let response = rx.await.unwrap();
        assert!(matches!(response, FuseResponse::ReadSuccess { .. }));
    }
    
    #[tokio::test]
    async fn test_async_worker_timeout() {
        let worker = AsyncFuseWorker::new(slow_mock_client());
        
        let (tx, rx) = oneshot::channel();
        let request = FuseRequest::ReadFile {
            torrent_id: 1,
            file_index: 0,
            offset: 0,
            size: 1024,
            timeout: Duration::from_millis(10), // Very short timeout
            response_tx: tx,
        };
        
        worker.send(request).await.unwrap();
        
        let response = rx.await.unwrap();
        assert!(matches!(response, 
            FuseResponse::ReadError { error_code, .. } 
            if error_code == libc::ETIMEDOUT
        ));
    }
}
```

### 6.2 Integration Tests

```rust
#[test]
fn test_fuse_mount_and_read() {
    let temp_dir = TempDir::new().unwrap();
    let mount_point = temp_dir.path().to_path_buf();
    
    // Start FUSE filesystem
    let fs = setup_test_fs(mount_point.clone());
    
    // Spawn in background
    std::thread::spawn(move || {
        fs.mount().unwrap();
    });
    
    // Wait for mount
    std::thread::sleep(Duration::from_millis(100));
    
    // Try to read a file
    let test_file = mount_point.join("test_torrent").join("test.txt");
    let content = std::fs::read_to_string(&test_file).unwrap();
    
    assert!(!content.is_empty());
}

#[test]
fn test_concurrent_reads_no_deadlock() {
    let temp_dir = TempDir::new().unwrap();
    let mount_point = temp_dir.path().to_path_buf();
    let fs = setup_test_fs(mount_point.clone());
    
    // Spawn FUSE
    std::thread::spawn(move || {
        fs.mount().unwrap();
    });
    
    std::thread::sleep(Duration::from_millis(100));
    
    // Spawn 50 concurrent readers
    let mut handles = vec![];
    for i in 0..50 {
        let mount = mount_point.clone();
        handles.push(std::thread::spawn(move || {
            let file = mount.join(format!("file_{}.txt", i % 5));
            let _ = std::fs::read(&file);
        }));
    }
    
    // All should complete within 30 seconds (no deadlock)
    for handle in handles {
        handle.join_timeout(Duration::from_secs(30)).unwrap();
    }
}
```

### 6.3 Performance Benchmarks

```rust
#[test]
fn test_read_latency() {
    let fs = setup_test_fs();
    let start = Instant::now();
    
    // Read 100MB in chunks
    let mut offset = 0;
    while offset < 100 * 1024 * 1024 {
        let chunk = fs.read_file(...).unwrap();
        offset += chunk.len();
    }
    
    let elapsed = start.elapsed();
    println!("Read 100MB in {:?}", elapsed);
    
    // Assert reasonable performance
    assert!(elapsed < Duration::from_secs(30));
}
```

---

## 7. Rollback Plan

If issues arise during migration:

1. **Immediate Rollback** (if system becomes unstable):
   ```bash
   git checkout HEAD -- src/fs/filesystem.rs
   cargo build
   ```

2. **Feature Flag** (gradual rollout):
   ```rust
   #[cfg(feature = "async-fuse-v2")]
   impl Filesystem for TorrentFS {
       fn read(&mut self, ...) {
           self.read_async_bridge(...)
       }
   }
   
   #[cfg(not(feature = "async-fuse-v2"))]
   impl Filesystem for TorrentFS {
       fn read(&mut self, ...) {
           self.read_block_in_place(...)
       }
   }
   ```

3. **A/B Testing**:
   - Run both implementations side-by-side
   - Compare metrics: latency, error rate, deadlock occurrences
   - Gradually migrate traffic to new implementation

---

## 8. Future Enhancements

### 8.1 Connection Pooling

```rust
pub struct PooledAsyncWorker {
    clients: Vec<Arc<RqbitClient>>,
    round_robin: AtomicUsize,
}
```

### 8.2 Read-Ahead Integration

```rust
pub struct ReadAheadCache {
    cache: Arc<tokio::sync::RwLock<LruCache<u64, Vec<u8>>>>,
    prefetch_tx: mpsc::Sender<PrefetchRequest>,
}
```

### 8.3 Metrics Integration

```rust
impl AsyncFuseWorker {
    async fn handle_request(...) {
        let start = Instant::now();
        let result = execute_request().await;
        metrics.record_latency(start.elapsed());
        metrics.record_result(&result);
        result
    }
}
```

---

## 9. References

1. **Tokio Documentation**:
   - `block_in_place`: https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html
   - `block_on`: https://docs.rs/tokio/latest/tokio/runtime/struct.Handle.html#method.block_on
   - Runtime constraints: https://docs.rs/tokio/latest/tokio/runtime/index.html

2. **FUSE Documentation**:
   - FUSE protocol: https://libfuse.github.io/doxygen/index.html
   - FUSE timeouts: https://libfuse.github.io/doxygen/structfuse__config.html

3. **Rust Async Patterns**:
   - Bridge pattern: https://tokio.rs/tokio/topics/bridging
   - Channel communication: https://tokio.rs/tokio/tutorial/channels

4. **Related Issues**:
   - FS-001: Research async FUSE patterns
   - FS-002: Fix blocking async in sync callbacks
   - FS-005: Replace std::sync::Mutex with tokio::sync::Mutex

---

## 10. Checklist for Implementation

### Before Starting:
- [ ] Read this entire document
- [ ] Understand the deadlock risks
- [ ] Review current filesystem.rs implementation
- [ ] Set up feature flag for gradual rollout

### During Implementation:
- [ ] Create `src/fs/error.rs` with FuseError types
- [ ] Create `src/fs/async_bridge.rs` with AsyncFuseWorker
- [ ] Update `TorrentFS` struct with async_worker field
- [ ] Migrate `read()` callback
- [ ] Migrate `remove_torrent()` method
- [ ] Add proper error handling
- [ ] Update main.rs runtime integration

### Testing Phase:
- [ ] Unit tests for async bridge
- [ ] Integration test: mount + read
- [ ] Integration test: concurrent reads
- [ ] Stress test: 1000+ operations
- [ ] Timeout scenario test
- [ ] Memory leak check

### Before Merging:
- [ ] All tests pass
- [ ] Clippy warnings resolved
- [ ] Code formatted with `cargo fmt`
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
- [ ] Rollback procedure tested

---

**Document Version**: 1.0  
**Last Updated**: February 14, 2026  
**Author**: AI Assistant  
**Reviewers**: [To be filled]
