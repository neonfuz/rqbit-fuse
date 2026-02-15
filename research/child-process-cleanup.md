# Child Process Cleanup Research

## Overview

Research into child process cleanup mechanisms for torrent-fuse.

## Findings

### Current Implementation

The main child process spawned by torrent-fuse is `fusermount` (or `fusermount3`) for unmounting the FUSE filesystem. This is invoked via `std::process::Command`.

### Issues Identified

1. **No timeout on shutdown**: The graceful shutdown had no timeout, potentially causing the process to hang indefinitely.
2. **No force unmount fallback**: If graceful unmount failed, there was no fallback to force unmount.
3. **No cleanup on normal exit**: When the mount returns normally (filesystem externally unmounted), cleanup was not performed with timeout protection.

### Solution Implemented

1. **Shutdown timeout**: Added 10-second timeout for graceful shutdown operations in the signal handler.
2. **Force unmount fallback**: If graceful unmount fails, attempts force unmount with `fusermount -uz`.
3. **Timeout-protected cleanup**: Both signal handler and normal exit paths now use timeouts to prevent hanging.
4. **Proper error handling**: Logs appropriate warnings/errors at each failure stage.

### Configuration

- Graceful shutdown timeout: 10 seconds
- Cleanup timeout on normal exit: 5 seconds
- Force unmount uses `-uz` flag (force lazy unmount)

### Testing

- All existing tests pass
- Code compiles without warnings
- Code is properly formatted
