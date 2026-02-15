# Error Design Research

## Overview

Research and implementation of typed error handling for torrent-fuse to replace fragile string-based error detection.

## String-Based Error Detection Issues (RESOLVED)

### Problem
The codebase was using fragile string matching to classify errors:

```rust
// Before (src/fs/error.rs:96-107)
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
```

This pattern appeared in:
- `src/fs/error.rs` - `ToFuseError` trait for `anyhow::Error`
- `src/fs/async_bridge.rs` - Error handling in `handle_request()` (lines 189-195, 225-229)

### Issues with String Matching
1. Fragile - depends on error message text that can change
2. Case sensitivity issues
3. Locale-dependent error messages may not match
4. No compile-time checking of error types
5. Cannot distinguish between different "not found" scenarios

## Solution Implemented

### 1. Updated `ToFuseError` Implementation (src/fs/error.rs)

Removed string matching and replaced with proper error downcasting:

```rust
impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types through downcasting
        if let Some(api_err) = self.downcast_ref::<crate::api::types::ApiError>() {
            return api_err.to_fuse_error();
        }

        if let Some(fuse_err) = self.downcast_ref::<FuseError>() {
            return fuse_err.to_errno();
        }

        // Check for std::io::Error
        if let Some(io_err) = self.downcast_ref::<std::io::Error>() {
            return match io_err.kind() {
                std::io::ErrorKind::NotFound => libc::ENOENT,
                std::io::ErrorKind::PermissionDenied => libc::EACCES,
                std::io::ErrorKind::TimedOut => libc::ETIMEDOUT,
                std::io::ErrorKind::InvalidInput => libc::EINVAL,
                _ => libc::EIO,
            };
        }

        // Default to EIO for unknown errors
        libc::EIO
    }
}
```

### 2. Updated Async Bridge Error Handling (src/fs/async_bridge.rs)

Replaced string matching with typed error conversion:

```rust
// Before
let error_code = if e.to_string().contains("not found") {
    libc::ENOENT
} else if e.to_string().contains("range") {
    libc::EINVAL
} else {
    libc::EIO
};

// After
let error_code = e.to_fuse_error();
```

## Typed Error Hierarchy

### ApiError (src/api/types.rs)
Already existed with comprehensive error variants:
- `TorrentNotFound(u64)` → ENOENT
- `FileNotFound { torrent_id, file_idx }` → ENOENT
- `InvalidRange(String)` → EINVAL
- `ConnectionTimeout` → EAGAIN
- `ReadTimeout` → EAGAIN
- `ServerDisconnected` → ENOTCONN
- `NetworkError(String)` → ENETUNREACH
- `CircuitBreakerOpen` → EAGAIN
- And more...

### FuseError (src/fs/error.rs)
FUSE-specific errors:
- `NotFound` → ENOENT
- `PermissionDenied` → EACCES
- `TimedOut` → ETIMEDOUT
- `InvalidArgument` → EINVAL
- And more...

### std::io::Error
Mapped via `ErrorKind`:
- `NotFound` → ENOENT
- `PermissionDenied` → EACCES
- `TimedOut` → ETIMEDOUT
- `InvalidInput` → EINVAL

## Benefits

1. **Type Safety**: Errors are classified at the source using proper types
2. **Compile-Time Checking**: Error types are verified by the compiler
3. **Maintainability**: Adding new error types is explicit and structured
4. **Correctness**: No risk of missing error cases due to string mismatches
5. **Performance**: No string conversion and searching needed

## FUSE Error Code Mapping Strategy

See `[spec:error-handling]` for complete mapping documentation.

Key mappings:
- Not found errors (ENOENT): 2
- Permission denied (EACCES): 13
- Invalid argument (EINVAL): 22
- I/O error (EIO): 5
- Try again (EAGAIN): 11
- Connection refused (ENOTCONN): 107
- Network unreachable (ENETUNREACH): 101

## Testing

All existing tests pass:
- 109 unit tests
- 56 integration tests
- 10 performance tests
- 0 clippy warnings
- Code formatted with cargo fmt

## References

- `[spec:error-handling]` - Comprehensive error handling specification
- `src/fs/error.rs` - FuseError and ToFuseError implementation
- `src/fs/async_bridge.rs` - Async bridge with typed error handling
- `src/api/types.rs` - ApiError definition
