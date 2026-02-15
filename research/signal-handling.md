# Signal Handling Research - RES-001

## Current State

The current implementation (main.rs) doesn't explicitly handle signals - it relies on the default FUSE behavior when the process is killed.

## Options for Signal Handling

### Option 1: tokio::signal (Built-in)

```rust
use tokio::signal;

async fn run_with_shutdown() {
    tokio::select! {
        result = run_fs() => {
            // Handle result
        }
        _ = signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down...");
        }
    }
}
```

Pros:
- No external dependencies
- Built into tokio
- Simple to implement

Cons:
- Basic - only handles Ctrl+C
- Need to add SIGTERM handling separately

### Option 2: tokio-graceful-shutdown Crate

```rust
use tokio_graceful_shutdown::{SubsystemBuilder, Toplevel};

async fn main() -> Result<()> {
    Toplevel::new(|handle| async move {
        // Start subsystems
        handle.start_subsystem("fs", run_fs());
    })
    .await;
}
```

Pros:
- Handles SIGINT, SIGTERM, Ctrl+C automatically
- Subsystem-based architecture
- Handles panics gracefully
- Well-maintained (425K+ downloads)

Cons:
- External dependency
- More complex API
- ~1.5K LOC overhead

### Option 3: Manual Signal Handling

```rust
use tokio::signal::unix::{signal, SignalKind};

async fn run_with_signals() {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = run_fs() => {}
        _ = sigint.recv() => { /* handle */ }
        _ = sigterm.recv() => { /* handle */ }
    }
}
```

Pros:
- Full control
- No extra dependencies
- Can handle both signals

Cons:
- More boilerplate
- Platform-specific (unix only)

## Recommendations for torrent-fuse

### Immediate Needs:
1. **FUSE unmount on shutdown**: The filesystem should unmount cleanly on SIGINT/SIGTERM
2. **Cache flush**: Save any pending cache state
3. **Background task cleanup**: Stop discovery, handle cleanup tasks

### Recommended Approach: Option 1 (tokio::signal)

Keep it simple since:
- torrent-fuse is a relatively simple application
- No need for complex subsystem architecture
- FUSE mount/unmount can be handled in the shutdown path

### Implementation Plan:

1. Add signal handling in `run()` function in lib.rs
2. On shutdown signal:
   - Signal FUSE to unmount
   - Flush caches
   - Clean up background tasks
   - Exit cleanly

### Child Process Cleanup:

If rqbit is spawned as a child process, need to:
- Track the child process PID
- On shutdown, send SIGTERM to the process group
- Add timeout for graceful shutdown (e.g., 5 seconds)
- Force kill if needed

### FUSE Unmount on Signal:

The current implementation relies on `auto_unmount` config option which uses `fuser::Fuse::mount()` with auto-unmount. This should handle cleanup on process exit.

However, for explicit signal handling, we could:
1. Catch SIGINT/SIGTERM
2. Call `fuse.unmount()` explicitly
3. Then exit

## Conclusion

Use tokio's built-in signal handling (Option 1). It's sufficient for our needs and avoids adding external dependencies. The `tokio-graceful-shutdown` crate is more full-featured but adds complexity we don't need.
