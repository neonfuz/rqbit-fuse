# Signal Handling Specification

## Overview

This document specifies the signal handling and graceful shutdown mechanism for the torrent-fuse application. The system must handle external signals to ensure clean resource cleanup, prevent data corruption, and maintain system integrity during shutdown.

## Requirements

### Signal Handling Requirements

#### Supported Signals

| Signal | Description | Action |
|--------|-------------|--------|
| `SIGINT` | Interrupt signal (Ctrl+C) | Initiate graceful shutdown |
| `SIGTERM` | Termination request | Initiate graceful shutdown |
| `SIGCHLD` | Child process status change | Monitor rqbit process |

#### Signal Handling Constraints

1. **Async-safe**: Signal handlers must only perform async-safe operations
2. **Non-blocking**: Signal handling must not block the main event loop
3. **Deterministic**: Signal handling order must be deterministic
4. **Cancellable**: Graceful shutdown must respect timeout limits

### Graceful Shutdown Sequence

```
Phase 1: Signal Reception
    ↓
Phase 2: Stop Accepting New Requests
    ↓
Phase 3: Wait for In-Flight Operations (with timeout)
    ↓
Phase 4: Flush Caches
    ↓
Phase 5: Unmount FUSE
    ↓
Phase 6: Terminate Child Processes
    ↓
Phase 7: Final Cleanup
```

## Implementation Approaches

### tokio::signal Usage

Tokio provides cross-platform signal handling through the `tokio::signal` module:

```rust
use tokio::signal::unix::{signal, SignalKind};
use tokio::select;

// Signal handler creation
let mut sigint = signal(SignalKind::interrupt())?;
let mut sigterm = signal(SignalKind::terminate())?;
let mut sigchld = signal(SignalKind::child())?;

// Signal monitoring loop
select! {
    _ = sigint.recv() => {
        info!("Received SIGINT, initiating graceful shutdown");
        shutdown_manager.initiate_shutdown();
    }
    _ = sigterm.recv() => {
        info!("Received SIGTERM, initiating graceful shutdown");
        shutdown_manager.initiate_shutdown();
    }
    _ = sigchld.recv() => {
        child_manager.handle_child_event().await;
    }
}
```

### Signal Handling Patterns

#### Pattern 1: Centralized Shutdown Manager

```rust
#[derive(Clone)]
pub struct ShutdownManager {
    shutdown_tx: watch::Sender<ShutdownState>,
    active_operations: Arc<AtomicUsize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShutdownState {
    Running,
    ShuttingDown,
    ForceShutdown,
}

impl ShutdownManager {
    pub fn initiate_shutdown(&self) {
        self.shutdown_tx.send(ShutdownState::ShuttingDown).ok();
    }
    
    pub fn force_shutdown(&self) {
        self.shutdown_tx.send(ShutdownState::ForceShutdown).ok();
    }
    
    pub fn is_shutting_down(&self) -> bool {
        *self.shutdown_tx.borrow() != ShutdownState::Running
    }
}
```

#### Pattern 2: Cooperative Cancellation

Components cooperate in shutdown by checking the shutdown state:

```rust
async fn handle_fuse_request(&self, req: Request) -> Result<()> {
    // Check if shutdown is in progress
    if self.shutdown_manager.is_shutting_down() {
        return Err(Error::ShuttingDown);
    }
    
    // Increment active operations counter
    let _guard = ActiveOperationGuard::new(&self.shutdown_manager);
    
    // Process the request
    self.process_request(req).await
}
```

### Graceful Shutdown with tokio::select!

```rust
async fn run_server(&self) -> Result<()> {
    let mut shutdown_rx = self.shutdown_manager.subscribe();
    
    loop {
        select! {
            biased; // Prioritize shutdown signal
            
            _ = shutdown_rx.changed() => {
                if shutdown_manager.is_shutting_down() {
                    break;
                }
            }
            
            req = self.request_rx.recv() => {
                match req {
                    Some(req) => self.handle_request(req).await,
                    None => break,
                }
            }
            
            // Other async operations...
        }
    }
    
    // Graceful shutdown sequence
    self.graceful_shutdown().await
}
```

### Timeout Handling

```rust
use tokio::time::{timeout, Duration};

pub const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
pub const FORCE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

async fn graceful_shutdown(&self) -> Result<()> {
    // Phase 1: Stop accepting new requests
    self.stop_accepting_requests();
    
    // Phase 2: Wait for in-flight operations with timeout
    match timeout(GRACEFUL_SHUTDOWN_TIMEOUT, self.wait_for_operations()).await {
        Ok(_) => info!("All operations completed gracefully"),
        Err(_) => {
            warn!("Timeout waiting for operations, forcing shutdown");
            self.force_shutdown().await;
        }
    }
    
    // Phase 3: Flush caches
    timeout(Duration::from_secs(10), self.flush_caches()).await
        .unwrap_or_else(|_| warn!("Cache flush timeout"));
    
    // Phase 4: Unmount FUSE
    timeout(Duration::from_secs(10), self.unmount_fuse()).await
        .unwrap_or_else(|_| warn!("FUSE unmount timeout"));
    
    // Phase 5: Terminate child processes
    timeout(Duration::from_secs(10), self.cleanup_children()).await
        .unwrap_or_else(|_| warn!("Child cleanup timeout"));
    
    Ok(())
}
```

## Shutdown Sequence Details

### Phase 1: Signal Reception

**Objective**: Receive and acknowledge shutdown signals

**Implementation**:
- Register signal handlers at application startup
- Use `tokio::signal` for async signal handling
- Log signal reception for debugging

**Edge Cases**:
- Multiple signals received simultaneously
- Signal received during startup
- Signal received during previous shutdown

### Phase 2: Stop Accepting New Requests

**Objective**: Prevent new FUSE operations from starting

**Implementation**:
```rust
impl Filesystem for TorrentFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if self.shutdown_manager.is_shutting_down() {
            reply.error(libc::ESHUTDOWN);
            return;
        }
        // ... normal lookup
    }
}
```

**Affected Operations**:
- `lookup`: Return `ESHUTDOWN`
- `open`: Return `ESHUTDOWN`
- `create`: Return `ESHUTDOWN`
- `mkdir`: Return `ESHUTDOWN`
- `symlink`: Return `ESHUTDOWN`
- `mknod`: Return `ESHUTDOWN`

### Phase 3: Wait for In-Flight Requests

**Objective**: Allow ongoing operations to complete naturally

**Implementation**:
```rust
pub struct ActiveOperationGuard {
    counter: Arc<AtomicUsize>,
}

impl ActiveOperationGuard {
    pub fn new(manager: &ShutdownManager) -> Self {
        manager.increment_operations();
        Self {
            counter: manager.active_operations(),
        }
    }
}

impl Drop for ActiveOperationGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Release);
    }
}

async fn wait_for_operations(&self) {
    while self.active_operations.load(Ordering::Acquire) > 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

**Timeout**: 30 seconds (configurable)

### Phase 4: Flush Caches

**Objective**: Persist any pending cache data

**Implementation**:
```rust
async fn flush_caches(&self) -> Result<()> {
    info!("Flushing caches...");
    
    // Flush dirty cache entries
    self.cache.flush().await?;
    
    // Persist cache metadata if applicable
    self.cache.persist_metadata().await?;
    
    info!("Caches flushed successfully");
    Ok(())
}
```

**Considerations**:
- Cancel any pending prefetch operations
- Persist metadata if using persistent cache
- Log cache statistics before shutdown

### Phase 5: Unmount FUSE

**Objective**: Cleanly unmount the FUSE filesystem

**Implementation**:
```rust
async fn unmount_fuse(&self) -> Result<()> {
    info!("Unmounting FUSE filesystem...");
    
    // Request filesystem unmount
    self.session.umount()?;
    
    // Wait for filesystem thread to complete
    if let Some(handle) = self.filesystem_handle.take() {
        tokio::time::timeout(Duration::from_secs(5), handle).await??;
    }
    
    info!("FUSE filesystem unmounted successfully");
    Ok(())
}
```

**Platform Differences**:
- **Linux**: Use `fusermount -u` or session unmount
- **macOS**: Use `umount` command

### Phase 6: Clean Up Background Tasks

**Objective**: Terminate all background async tasks

**Implementation**:
```rust
async fn cleanup_background_tasks(&self) {
    // Abort all background task handles
    for handle in &self.background_tasks {
        handle.abort();
    }
    
    // Wait for tasks to complete (or timeout)
    let results = join_all(self.background_tasks.drain(..)).await;
    
    // Log any panics or errors
    for (idx, result) in results.iter().enumerate() {
        if let Err(e) = result {
            warn!("Background task {} failed during shutdown: {:?}", idx, e);
        }
    }
}
```

### Phase 7: Terminate Child Processes

**Objective**: Clean up spawned rqbit processes

**Implementation**:
```rust
async fn cleanup_children(&self) -> Result<()> {
    info!("Cleaning up child processes...");
    
    for (pid, handle) in self.child_processes.iter() {
        // Attempt graceful termination
        match handle.try_graceful_termination().await {
            Ok(()) => {
                info!("Process {} terminated gracefully", pid);
            }
            Err(_) => {
                warn!("Force terminating process {}", pid);
                handle.kill().await?;
            }
        }
    }
    
    Ok(())
}
```

## Resource Cleanup

### Child Process Management

#### Process Lifecycle

```rust
pub struct ChildProcessManager {
    processes: Arc<Mutex<HashMap<u32, ChildHandle>>>,
    shutdown_tx: broadcast::Sender<()>,
}

pub struct ChildHandle {
    process: tokio::process::Child,
    graceful_shutdown: bool,
}

impl ChildProcessManager {
    pub async fn spawn(&self, cmd: &mut Command) -> Result<u32> {
        let mut child = cmd.spawn()?;
        let pid = child.id().ok_or_else(|| Error::MissingPid)?;
        
        let handle = ChildHandle {
            process: child,
            graceful_shutdown: true,
        };
        
        self.processes.lock().await.insert(pid, handle);
        
        // Spawn monitor task
        tokio::spawn(self.clone().monitor_child(pid));
        
        Ok(pid)
    }
    
    async fn monitor_child(self, pid: u32) {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        if let Some(mut handle) = self.processes.lock().await.get_mut(&pid) {
            select! {
                status = handle.process.wait() => {
                    match status {
                        Ok(code) => info!("Child {} exited with code {:?}", pid, code),
                        Err(e) => error!("Child {} wait error: {}", pid, e),
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received for child {}", pid);
                    self.terminate_child(pid).await;
                }
            }
        }
        
        self.processes.lock().await.remove(&pid);
    }
}
```

#### Graceful Termination Strategy

```rust
impl ChildHandle {
    pub async fn try_graceful_termination(&mut self) -> Result<()> {
        if !self.graceful_shutdown {
            return Err(Error::GracefulShutdownNotSupported);
        }
        
        // Send SIGTERM first
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            let pid = Pid::from_raw(self.process.id().unwrap() as i32);
            kill(pid, Signal::SIGTERM)?;
            
            // Wait for graceful shutdown with timeout
            let timeout = Duration::from_secs(5);
            match tokio::time::timeout(timeout, self.process.wait()).await {
                Ok(Ok(_)) => return Ok(()),
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => return Err(Error::Timeout),
            }
        }
        
        #[cfg(windows)]
        {
            // Windows: use taskkill / graceful equivalent
            unimplemented!("Windows child process termination")
        }
    }
    
    pub async fn kill(&mut self) -> Result<()> {
        self.process.kill().await?;
        self.process.wait().await?;
        Ok(())
    }
}
```

### Timeout Configuration

```rust
pub struct ShutdownConfig {
    /// Timeout for graceful operation completion
    pub graceful_timeout: Duration,
    
    /// Timeout for cache flush
    pub cache_flush_timeout: Duration,
    
    /// Timeout for FUSE unmount
    pub unmount_timeout: Duration,
    
    /// Timeout for child process graceful termination
    pub child_graceful_timeout: Duration,
    
    /// Timeout for child process force kill
    pub child_kill_timeout: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            graceful_timeout: Duration::from_secs(30),
            cache_flush_timeout: Duration::from_secs(10),
            unmount_timeout: Duration::from_secs(10),
            child_graceful_timeout: Duration::from_secs(5),
            child_kill_timeout: Duration::from_secs(2),
        }
    }
}
```

### Force Kill Mechanism

```rust
impl ShutdownManager {
    pub async fn force_shutdown(&self) {
        warn!("Initiating forced shutdown");
        
        // Signal force shutdown state
        self.force_shutdown_tx.send(()).ok();
        
        // Immediately kill all child processes
        for pid in self.child_manager.get_pids().await {
            if let Err(e) = self.child_manager.force_kill(pid).await {
                error!("Failed to force kill process {}: {}", pid, e);
            }
        }
        
        // Force unmount FUSE
        if let Err(e) = self.force_unmount_fuse().await {
            error!("Failed to force unmount FUSE: {}", e);
        }
        
        // Abort all background tasks
        for handle in self.background_tasks.lock().await.iter() {
            handle.abort();
        }
    }
}
```

## Implementation Details

### Signal Handler Setup

```rust
pub struct SignalHandler {
    shutdown_manager: Arc<ShutdownManager>,
    child_manager: Arc<ChildProcessManager>,
}

impl SignalHandler {
    pub fn new(
        shutdown_manager: Arc<ShutdownManager>,
        child_manager: Arc<ChildProcessManager>,
    ) -> Self {
        Self {
            shutdown_manager,
            child_manager,
        }
    }
    
    pub async fn run(self) -> Result<()> {
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigchld = signal(SignalKind::child())?;
        
        info!("Signal handler started");
        
        loop {
            select! {
                _ = sigint.recv() => {
                    info!("Received SIGINT");
                    self.handle_shutdown_signal().await;
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM");
                    self.handle_shutdown_signal().await;
                }
                _ = sigchld.recv() => {
                    self.child_manager.handle_child_event().await;
                }
            }
        }
    }
    
    async fn handle_shutdown_signal(&self) {
        self.shutdown_manager.initiate_shutdown();
    }
}
```

### Shutdown Coordination

```rust
pub struct ShutdownCoordinator {
    manager: Arc<ShutdownManager>,
    components: Vec<Box<dyn ShutdownComponent>>,
}

#[async_trait]
pub trait ShutdownComponent: Send + Sync {
    async fn shutdown(&self) -> Result<()>;
    async fn force_shutdown(&self);
}

impl ShutdownCoordinator {
    pub async fn coordinate_shutdown(&self) -> Result<()> {
        // Phase 1: Signal all components to stop accepting new work
        for component in &self.components {
            component.stop_accepting_new().await;
        }
        
        // Phase 2: Wait for in-flight operations
        timeout(
            self.config.graceful_timeout,
            self.wait_for_operations(),
        ).await?;
        
        // Phase 3: Shutdown each component
        for component in &self.components {
            if let Err(e) = component.shutdown().await {
                warn!("Component shutdown failed: {}", e);
            }
        }
        
        Ok(())
    }
}
```

### Resource Limit Enforcement

```rust
pub struct ResourceLimiter {
    max_operations: usize,
    current_operations: Arc<AtomicUsize>,
}

impl ResourceLimiter {
    pub fn try_acquire_operation(&self) -> Option<OperationGuard> {
        let current = self.current_operations.fetch_add(1, Ordering::SeqCst);
        
        if current >= self.max_operations {
            self.current_operations.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(OperationGuard {
                counter: self.current_operations.clone(),
            })
        }
    }
}

pub struct OperationGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}
```

## Platform Considerations

### Unix/Linux

- Use `tokio::signal::unix` for signal handling
- Use `SIGTERM` for graceful shutdown requests
- Use `SIGKILL` only for force kill
- Use `fusermount -u` for FUSE unmounting

### macOS

- Signal handling similar to Unix
- Use `diskutil unmount` for FUSE unmounting
- Handle `SIGINFO` for status requests (optional)

### Windows

- Use `tokio::signal::ctrl_c()` for Ctrl+C handling
- Implement graceful shutdown via named events
- Handle `CTRL_CLOSE_EVENT`, `CTRL_LOGOFF_EVENT`, `CTRL_SHUTDOWN_EVENT`

## Testing Strategy

### Unit Tests

```rust
#[tokio::test]
async fn test_shutdown_manager_state_transition() {
    let manager = ShutdownManager::new();
    assert_eq!(*manager.state(), ShutdownState::Running);
    
    manager.initiate_shutdown();
    assert_eq!(*manager.state(), ShutdownState::ShuttingDown);
    
    manager.force_shutdown();
    assert_eq!(*manager.state(), ShutdownState::ForceShutdown);
}

#[tokio::test]
async fn test_active_operation_guard() {
    let manager = ShutdownManager::new();
    
    {
        let _guard = ActiveOperationGuard::new(&manager);
        assert_eq!(manager.active_count(), 1);
    }
    
    assert_eq!(manager.active_count(), 0);
}
```

### Integration Tests

```rust
#[tokio::test]
#[ignore = "Requires FUSE setup"]
async fn test_graceful_shutdown_sequence() {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    
    // Start the filesystem
    let fs = start_test_filesystem().await;
    
    // Simulate operations
    let ops = spawn_test_operations(&fs);
    
    // Send SIGINT
    send_signal(Signal::SIGINT);
    
    // Wait for shutdown with timeout
    let result = tokio::time::timeout(
        Duration::from_secs(35),
        shutdown_rx,
    ).await;
    
    assert!(result.is_ok(), "Shutdown timed out");
    
    // Verify cleanup
    assert!(!fs.is_mounted());
    assert_eq!(fs.active_operations(), 0);
    assert!(fs.cache.is_flushed());
}
```

## Error Handling

### Signal Handling Errors

| Error | Cause | Response |
|-------|-------|----------|
| Signal registration failed | Platform limitation | Log error, continue without signal handling |
| Signal receive error | Internal tokio error | Retry with backoff, log warning |
| Multiple signals | Rapid signal reception | Debounce, log all signals |

### Shutdown Errors

| Error | Cause | Response |
|-------|-------|----------|
| Operation timeout | Long-running operation | Log operation details, proceed to force shutdown |
| Cache flush error | I/O error | Log error, proceed with remaining cleanup |
| FUSE unmount error | Mount point busy | Attempt lazy unmount, then force unmount |
| Child process kill failed | Process zombie | Log error, continue cleanup |

## Logging and Observability

### Structured Logging

```rust
info!(
    target: "torrent_fuse::shutdown",
    phase = "initiated",
    signal = "SIGTERM",
    active_operations = active_ops,
    "Graceful shutdown initiated"
);

info!(
    target: "torrent_fuse::shutdown",
    phase = "waiting_operations",
    count = active_ops,
    timeout_secs = 30,
    "Waiting for in-flight operations"
);

info!(
    target: "torrent_fuse::shutdown",
    phase = "complete",
    duration_ms = elapsed.as_millis(),
    "Graceful shutdown completed"
);
```

### Metrics

```rust
pub struct ShutdownMetrics {
    /// Number of graceful shutdowns initiated
    graceful_shutdowns: Counter,
    
    /// Number of force shutdowns initiated
    force_shutdowns: Counter,
    
    /// Shutdown duration histogram
    shutdown_duration: Histogram,
    
    /// Operations in flight at shutdown time
    operations_at_shutdown: Gauge,
}
```

## Security Considerations

1. **Signal Validation**: Ensure signals come from expected sources (parent process, user terminal)
2. **Cleanup Verification**: Verify all resources are freed, especially temp files
3. **Child Process Isolation**: Ensure child processes cannot interfere with shutdown
4. **Atomic Operations**: Use atomic state transitions to prevent race conditions

## References

- [Tokio Signal Documentation](https://docs.rs/tokio/latest/tokio/signal/index.html)
- [Unix Signal Handling](https://man7.org/linux/man-pages/man7/signal.7.html)
- [FUSE Unmount Best Practices](https://libfuse.github.io/doxygen/)
- [Rust Signal Safety](https://doc.rust-lang.org/nightly/std/os/unix/process/trait.CommandExt.html)
