# Signal Handling Specification

## Overview

This document specifies the signal handling and graceful shutdown mechanism for the rqbit-fuse application. The system handles termination signals to ensure clean FUSE unmounting and resource cleanup.

## Current Implementation

### Signals Handled

| Signal | Description | Action |
|--------|-------------|--------|
| `SIGINT` | Interrupt signal (Ctrl+C) | Initiate graceful shutdown |
| `SIGTERM` | Termination request | Initiate graceful shutdown |

Note: `SIGCHLD` is NOT currently handled. The application assumes rqbit runs as a separate external process.

### Signal Handler Implementation

Signal handling is implemented directly in `src/lib.rs` using `tokio::signal::unix`:

```rust
use tokio::signal::unix::{signal, SignalKind};

let mut sigint = signal(SignalKind::interrupt()).unwrap();
let mut sigterm = signal(SignalKind::terminate()).unwrap();

tokio::select! {
    _ = sigint.recv() => {
        tracing::info!("Received SIGINT, initiating graceful shutdown...");
    }
    _ = sigterm.recv() => {
        tracing::info!("Received SIGTERM, initiating graceful shutdown...");
    }
}
```

### Shutdown Sequence

The actual shutdown sequence is simpler than the original design:

```
Phase 1: Signal Reception
    ↓
Phase 2: Signal Mount Task
    ↓
Phase 3: Call Filesystem Shutdown
    ↓
Phase 4: Unmount FUSE (graceful, then force)
    ↓
Phase 5: Log Metrics
```

### Shutdown Implementation Details

#### Phase 1-2: Signal Reception and Coordination

```rust
// Channel to signal shutdown from signal handler to mount task
let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

// Signal handler task sends shutdown signal
let _ = shutdown_tx.send(());

// Mount task receives signal
tokio::select! {
    _ = shutdown_rx => {
        tracing::info!("Shutdown signal received, mount task is completing...");
    }
    _ = async {} => {}
}
```

#### Phase 3: Filesystem Shutdown

The `TorrentFS::shutdown()` method stops background tasks:

```rust
pub fn shutdown(&self) {
    info!("Initiating graceful shutdown...");
    self.stop_torrent_discovery();
    info!("Graceful shutdown complete");
}

fn stop_torrent_discovery(&self) {
    if let Ok(handle) = self.discovery_handle.try_lock() {
        if let Some(h) = handle.as_ref() {
            h.abort();
            info!("Stopped torrent discovery");
        }
    }
}
```

Background tasks stopped:
- **Torrent discovery**: Polls rqbit for new torrents every 30 seconds

#### Phase 4: FUSE Unmount

Uses `fusermount` command for unmounting:

```rust
// Try graceful unmount first
let result = tokio::task::spawn_blocking(move || {
    std::process::Command::new("fusermount")
        .arg("-u")
        .arg(&mount_point)
        .output()
}).await;

// If graceful fails, try force/lazy unmount
if let Err(_) = result {
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("fusermount")
            .arg("-uz")  // -z for lazy unmount
            .arg(&mount_point_force)
            .output()
    }).await;
}
```

#### Phase 5: Cleanup and Metrics

```rust
// Final cleanup with timeout
let cleanup_timeout = Duration::from_secs(5);
let cleanup = async {
    fs_arc.shutdown();
    // Attempt unmount if still mounted
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("fusermount")
            .arg("-u")
            .arg(mount_point_cleanup)
            .output()
    }).await.ok();
};
let _ = tokio::time::timeout(cleanup_timeout, cleanup).await;

// Wait for signal handler to complete
let _ = tokio::time::timeout(Duration::from_secs(5), signal_handler).await;

// Log final metrics
metrics.log_summary();
```

### Timeout Configuration

Timeouts are currently hardcoded (not configurable):

| Operation | Timeout | Location |
|-----------|---------|----------|
| Graceful shutdown | 10 seconds | `lib.rs` signal handler |
| Cleanup operations | 5 seconds | `lib.rs` cleanup phase |
| Signal handler completion | 5 seconds | `lib.rs` final wait |

### Async Worker Shutdown

The `AsyncFuseWorker` handles shutdown via an oneshot channel:

```rust
pub struct AsyncFuseWorker {
    request_tx: mpsc::Sender<FuseRequest>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

pub fn shutdown(&mut self) {
    if let Some(tx) = self.shutdown_tx.take() {
        info!("Sending shutdown signal to AsyncFuseWorker");
        let _ = tx.send(());
    }
}
```

The worker task exits its loop when the shutdown signal is received:

```rust
loop {
    tokio::select! {
        biased;
        
        // Handle shutdown signal first
        _ = &mut shutdown_rx => {
            info!("AsyncFuseWorker received shutdown signal");
            break;
        }
        
        // Handle incoming requests
        Some(request) = request_rx.recv() => {
            // Process request...
        }
    }
}
```

## Not Implemented

The following features described in the original design are NOT currently implemented:

1. **ShutdownManager**: No centralized shutdown state management
2. **SIGCHLD handling**: No child process monitoring
3. **ActiveOperationGuard**: No tracking of in-flight operations
4. **Cache flush on shutdown**: Cache is in-memory only, no persistence
5. **Child process management**: rqbit is expected to be managed externally
6. **ShutdownConfig struct**: Timeouts are hardcoded
7. **ShutdownCoordinator**: No formal coordinator pattern
8. **Force shutdown mechanism**: Only timeout-based fallback
9. **ESHUTDOWN error code**: FUSE operations don't check for shutdown state
10. **Structured shutdown metrics**: Basic metrics only, no shutdown-specific counters

## Platform Considerations

### Linux
- Uses `tokio::signal::unix` for signal handling
- Uses `fusermount -u` for unmounting
- Uses `fusermount -uz` for lazy/force unmount

### macOS
- Signal handling same as Linux
- Note: macOS may require different unmount commands in future

### Windows
- Not currently supported (would need `tokio::signal::ctrl_c()`)

## Error Handling

| Error | Cause | Response |
|-------|-------|----------|
| Unmount failure | Mount point busy | Try lazy unmount (`fusermount -uz`) |
| Timeout | Shutdown taking too long | Force exit after timeout |
| Signal handler panic | Unexpected error | Log error, attempt cleanup |

## Testing

Current testing approach:
- Manual testing with Ctrl+C during operations
- Verify unmount occurs cleanly
- Check logs for proper shutdown sequence

## Future Improvements

Potential enhancements (not yet implemented):
1. Make timeouts configurable
2. Add graceful shutdown state checks to FUSE operations
3. Implement proper in-flight operation tracking
4. Add structured shutdown metrics
5. Support additional signals (SIGHUP for config reload)

## References

- Implementation: `src/lib.rs` (lines 110-238)
- Filesystem shutdown: `src/fs/filesystem.rs` (lines 377-390)
- Async worker shutdown: `src/fs/async_bridge.rs` (lines 112-113, 478-488)
- [Tokio Signal Documentation](https://docs.rs/tokio/latest/tokio/signal/index.html)
