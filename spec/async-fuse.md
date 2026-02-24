# Async FUSE Integration Specification

## Overview

This document describes the async/sync bridge architecture used in rqbit-fuse to enable async I/O operations from synchronous FUSE callbacks. FUSE callbacks are inherently synchronous (they must return immediately), but the project needs to perform async HTTP requests to rqbit. This document explains the channel-based solution that safely bridges these two worlds.

**Status**: Implemented  
**Related Issues**: FS-001, FS-002  
**Implementation**: Complete in `src/fs/async_bridge.rs`

---

## 1. Problem Analysis

### 1.1 The Async-in-Sync Challenge

FUSE callbacks are synchronous - they must return a result immediately. However, rqbit-fuse needs to perform async HTTP operations to the rqbit API. This creates a fundamental mismatch:

- **FUSE callbacks**: Run in synchronous threads, must return immediately
- **HTTP operations**: Async, may take seconds to complete
- **Dangerous pattern**: Using `block_in_place` + `block_on` can cause deadlocks

### 1.2 Risks of Blocking in FUSE Callbacks

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

---

## 2. Solution: Channel-Based Async/Sync Bridge

### 2.1 Architecture Overview

The implemented solution uses a hybrid channel architecture to bridge sync FUSE callbacks with async HTTP operations.

### 2.2 Hybrid Channel Architecture

The solution uses two different channel types:

**Request Channel: `tokio::sync::mpsc`**
- Used to send requests from sync FUSE callbacks to the async worker task
- Uses tokio's async channel because the worker task runs in an async context
- Using `std::sync::mpsc` would block the async executor
- The `select!` macro requires async-aware channels

**Response Channel: `std::sync::mpsc`**
- Used to send responses from the async worker back to sync FUSE callbacks
- Uses std's sync channel because FUSE callbacks run in synchronous threads
- Provides `recv_timeout()` for timeout handling
- `tokio::sync::mpsc` does not provide blocking recv with timeout

This hybrid approach is the correct pattern for async/sync bridging in FUSE.

---

## 3. Implementation Details

### 3.1 Request/Response Types

```rust
/// Request sent from FUSE callback to async worker
#[derive(Debug)]
pub enum FuseRequest {
    ReadFile {
        torrent_id: u64,
        file_index: u64,
        offset: u64,
        size: usize,
        timeout: Duration,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    CheckPiecesAvailable {
        torrent_id: u64,
        offset: u64,
        size: u64,
        timeout: Duration,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    ForgetTorrent {
        torrent_id: u64,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
}

#[derive(Debug, Clone)]
pub enum FuseResponse {
    ReadSuccess { data: Vec<u8> },
    ReadError { error_code: i32, message: String },
    PiecesAvailable,
    PiecesNotAvailable { reason: String },
    ForgetSuccess,
    ForgetError { error_code: i32, message: String },
}
```

### 3.2 Async Worker Implementation

The `AsyncFuseWorker` struct manages the bridge between sync FUSE callbacks and async operations:

- Uses `tokio::sync::mpsc` for the request channel (async side)
- Spawns a tokio task that listens for requests
- Each request spawns its own task for concurrent processing
- Uses `std::sync::mpsc` for response channels (sync side with timeout)

### 3.3 Synchronous API for FUSE Callbacks

The worker provides synchronous methods that FUSE callbacks can call:

- `send_request()` - Core method using builder pattern
- `read_file()` - Convenience method for reading file data
- `forget_torrent()` - Convenience method for removing torrents
- `check_pieces_available()` - Check piece availability

---

## 4. Usage in Filesystem

### 4.1 Integration with TorrentFS

The `AsyncFuseWorker` is integrated into `TorrentFS` as an `Arc<AsyncFuseWorker>` field.

### 4.2 Read Callback Implementation

The `read()` callback uses `self.async_worker.read_file()` to perform reads without blocking.

### 4.3 Remove Torrent Implementation

The `remove_torrent()` method uses `self.async_worker.forget_torrent()` for async removal.

---

## 5. Concurrency Model

### 5.1 Thread Safety

- Request Channel: `tokio::sync::mpsc` is thread-safe and async-aware
- Response Channels: Each request gets its own `std::sync::mpsc` channel
- Worker Task: Single async task handles all requests, spawning sub-tasks
- No Shared Mutable State: Uses `Arc<RqbitClient>` and `Arc<Metrics>`

### 5.2 Concurrent Request Handling

Each request spawns its own tokio task, allowing multiple HTTP requests in flight simultaneously.

### 5.3 Timeout Handling

Timeouts are handled at multiple levels:
1. HTTP Request Timeout via `tokio::time::timeout`
2. Channel Timeout via `recv_timeout()`
3. 5-second buffer added for channel overhead

---

## 6. Error Handling

Errors are mapped to appropriate FUSE error codes via the `ToFuseError` trait:
- `NotFound` -> `ENOENT`
- `PermissionDenied` -> `EACCES`
- `TimedOut` -> `ETIMEDOUT`
- `IoError` -> `EIO`
- `InvalidArgument` -> `EINVAL`

---

## 7. Testing

### 7.1 Unit Tests

The async bridge includes unit tests for request/response types in `src/fs/async_bridge.rs`.

### 7.2 Integration Tests

Integration tests verify the full flow including concurrent reads and timeout recovery.

---

## 8. Key Design Decisions

### 8.1 Why Hybrid Channels?

- Tokio mpsc for requests: Required for async `select!` macro
- Std mpsc for responses: Required for `recv_timeout()` in sync context

### 8.2 Why Spawn Per-Request?

- Allows true concurrency for multiple simultaneous reads
- Prevents one slow request from blocking others
- Simpler than a worker pool for current use case

### 8.3 Why Builder Pattern for Requests?

The `send_request` method takes a closure to build the request, ensuring each request gets its own unique response channel.

---

## 9. References

1. Tokio Documentation: https://tokio.rs/tokio/tutorial/channels
2. FUSE Documentation: https://libfuse.github.io/doxygen/index.html
3. Rust Async Patterns: https://tokio.rs/tokio/topics/bridging

---

## 10. Checklist

### Implementation Complete:
- [x] Created `src/fs/async_bridge.rs` with AsyncFuseWorker
- [x] Implemented hybrid channel architecture
- [x] Integrated with `TorrentFS`
- [x] Migrated `read()` callback to use async worker
- [x] Migrated `remove_torrent()` to use async worker
- [x] Added proper error handling and timeout logic
- [x] Added unit tests for request/response types

---

**Document Version**: 2.0  
**Last Updated**: February 24, 2026  
**Author**: AI Assistant  
**Status**: Implementation Complete
