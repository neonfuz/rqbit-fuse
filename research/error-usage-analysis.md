# Error Usage Analysis

**Task**: 3.1.1 - Research error type usage patterns  
**Date**: 2026-02-23  
**Purpose**: Document all RqbitFuseError variants and their usage to identify consolidation opportunities

---

## Current Error Variant Count: 32

The `RqbitFuseError` enum currently has 32 variants across 8 categories.

---

## Error Variant Usage Analysis

### 1. Not Found Errors (3 variants) → **Merge to 1**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `NotFound` | 8 | ENOENT | Generic not found |
| `TorrentNotFound(u64)` | 15 | ENOENT | Specific to torrent ID |
| `FileNotFound { torrent_id, file_idx }` | 6 | ENOENT | Specific to file in torrent |

**Recommendation**: Merge all three into a single `NotFound` variant with optional context string.
- All map to `ENOENT`
- Usage is split between generic and specific contexts
- Can be consolidated to: `NotFound(Option<String>)`

---

### 2. Permission/Auth Errors (2 variants) → **Merge to 1**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `PermissionDenied` | 6 | EACCES | Generic permission error |
| `AuthenticationError(String)` | 3 | EACCES | HTTP auth failure |

**Recommendation**: Merge into single `PermissionDenied` variant.
- Both map to `EACCES`
- Auth error is just a specific permission case
- Can add context string to distinguish

---

### 3. Timeout Errors (3 variants) → **Merge to 1**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `TimedOut` | 6 | ETIMEDOUT | Generic timeout |
| `ConnectionTimeout` | 10 | EAGAIN | Server not responding |
| `ReadTimeout` | 9 | EAGAIN | Request took too long |

**Recommendation**: Merge into single `TimedOut` variant.
- `ConnectionTimeout` and `ReadTimeout` both map to `EAGAIN`
- `TimedOut` maps to `ETIMEDOUT`
- Can differentiate by context, not separate variant
- Both are transient errors

---

### 4. I/O Errors (2 variants) → **Merge to 1**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `IoError(String)` | 10 | EIO | Generic I/O error |
| `ReadError(String)` | 2 | EIO | Config file read error |

**Recommendation**: Merge into single `IoError(String)`.
- Both map to `EIO`
- `ReadError` is just a specific I/O case
- Consolidate to single variant with descriptive message

---

### 5. Network/API Errors (8 variants) → **Merge to 3**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `HttpError(String)` | 5 | EIO | HTTP request failed |
| `ServerDisconnected` | 11 | ENOTCONN | Server not available |
| `NetworkError(String)` | 12 | ENETUNREACH | Network error |
| `ServiceUnavailable(String)` | 4 | EAGAIN | 503 errors |
| `CircuitBreakerOpen` | 7 | EAGAIN | Too many failures |
| `ApiError { status, message }` | 18 | Various | API returned error |
| `ClientInitializationError(String)` | 3 | EIO | HTTP client init failed |
| `RequestCloneError(String)` | 2 | EIO | Request cloning failed |

**Recommendation**: Consolidate to 3 variants:
1. `NetworkError(String)` - Covers: ServerDisconnected, NetworkError, ServiceUnavailable, CircuitBreakerOpen
2. `ApiError { status: u16, message: String }` - Keep as-is for HTTP status mapping
3. `IoError(String)` - Covers: HttpError, ClientInitializationError, RequestCloneError (merge with I/O)

All are transient except ApiError with certain status codes.

---

### 6. Validation Errors (4 variants) → **Merge to 2**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `InvalidArgument` | 3 | EINVAL | Generic invalid arg |
| `InvalidRange(String)` | 9 | EINVAL | Range request invalid |
| `InvalidValue(String)` | 4 | EINVAL | Config value invalid |
| `ValidationError(Vec<ValidationIssue>)` | 5 | EINVAL | Multiple validation errors |

**Recommendation**: Consolidate to 2 variants:
1. `InvalidArgument(String)` - Covers: InvalidArgument, InvalidRange, InvalidValue
2. `ValidationError(Vec<ValidationIssue>)` - Keep for config validation (multiple issues)

All map to `EINVAL`. Can merge the three single-value variants.

---

### 7. Resource Errors (3 variants) → **Merge to 2**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `NotReady` | 4 | EAGAIN | Resource temporarily unavailable |
| `DeviceBusy` | 2 | EBUSY | Device/resource busy |
| `ChannelFull` | 2 | EIO | Request channel full |

**Recommendation**: Consolidate to 2 variants:
1. `NotReady` - Keep for EAGAIN cases (includes DeviceBusy conceptually)
2. Merge `ChannelFull` into `IoError(String)` - Only 2 usages, maps to EIO

---

### 8. State Errors (3 variants) → **Merge to 2**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `WorkerDisconnected` | 3 | EIO | Async worker disconnected |
| `RetryLimitExceeded` | 8 | EAGAIN | Too many retries |
| `SerializationError(String)` | 1 | EINVAL | JSON/serialization error |
| `ParseError(String)` | 4 | EINVAL | Config parse error |

**Recommendation**: Consolidate to 2 variants:
1. `WorkerDisconnected` → merge into `IoError(String)` (3 usages, EIO)
2. `RetryLimitExceeded` → merge into `NotReady` (EAGAIN, transient)
3. `SerializationError` + `ParseError` → merge into single `ParseError(String)` (both EINVAL)

---

### 9. Directory Errors (2 variants) → **Keep both**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `IsDirectory` | 2 | EISDIR | Operation on directory |
| `NotDirectory` | 2 | ENOTDIR | Expected directory |

**Recommendation**: Keep both - they map to different errno values and are semantically distinct for FUSE operations.

---

### 10. Filesystem Errors (1 variant) → **Merge**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `ReadOnlyFilesystem` | 1 | EROFS | Write attempted on read-only FS |

**Recommendation**: Merge into `PermissionDenied(String)` with context.
- Only 1 usage
- Can be expressed as permission error with message

---

### 11. Data Errors (1 variant) → **Keep or merge**

| Variant | Usage Count | errno | Notes |
|---------|-------------|-------|-------|
| `DataUnavailable { torrent_id, reason }` | 1 | EIO | Piece data not available |

**Recommendation**: Merge into `IoError(String)` with descriptive message.
- Only 1 usage
- The context can be included in the error message

---

## Proposed Simplified Error Enum (8 variants)

```rust
pub enum RqbitFuseError {
    // 1. Not Found (was 3 variants)
    #[error("Not found: {0}")]
    NotFound(String),

    // 2. Permission Denied (was 2 variants)
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    // 3. Timeout (was 3 variants)
    #[error("Operation timed out: {0}")]
    TimedOut(String),

    // 4. Network Error (was 8 variants)
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    // 5. I/O Error (was 2 + merged variants)
    #[error("I/O error: {0}")]
    IoError(String),

    // 6. Invalid Input (was 4 variants)
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    
    #[error("Validation failed: {}", .0.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    ValidationError(Vec<ValidationIssue>),

    // 7. Resource Not Ready (was 3 variants)
    #[error("Resource temporarily unavailable: {0}")]
    NotReady(String),

    // 8. Directory Errors (keep both - different errno)
    #[error("Is a directory")]
    IsDirectory,
    
    #[error("Not a directory")]
    NotDirectory,
}
```

---

## errno Mapping Summary

| Error Variant | errno | Notes |
|---------------|-------|-------|
| `NotFound` | ENOENT | All not-found cases |
| `PermissionDenied` | EACCES | Permission/auth failures |
| `TimedOut` | ETIMEDOUT | Timeout cases |
| `NetworkError` | ENETUNREACH / EAGAIN | Network/server issues |
| `ApiError { status, .. }` | status-dependent | Map HTTP status codes |
| `IoError` | EIO | I/O failures |
| `InvalidArgument` | EINVAL | Invalid inputs |
| `ValidationError` | EINVAL | Config validation |
| `NotReady` | EAGAIN | Resource unavailable |
| `IsDirectory` | EISDIR | Directory operation error |
| `NotDirectory` | ENOTDIR | Non-directory error |

---

## Benefits of Consolidation

1. **Reduced complexity**: 32 → 8 variants (75% reduction)
2. **Simpler errno mapping**: 11 groups → 11 clear mappings
3. **Easier maintenance**: Fewer variants to maintain and document
4. **Clearer semantics**: Each variant has distinct purpose
5. **Better error messages**: Context strings provide detail instead of separate variants

---

## Migration Strategy

1. Update `RqbitFuseError` enum definition
2. Update `to_errno()` method for new variants
3. Update `is_transient()` and `is_server_unavailable()` methods
4. Update all `From<>` implementations
5. Replace all usages of removed variants with consolidated ones
6. Update tests
7. Run full test suite

---

## Files to Update

- `src/error.rs` - Main error definition
- `src/config/mod.rs` - Config validation errors
- `src/api/client.rs` - API error handling
- `src/api/streaming.rs` - Streaming errors
- `src/fs/filesystem.rs` - FUSE error handling
- `src/fs/async_bridge.rs` - Async operation errors
- Test files throughout codebase

---

*Analysis complete. Ready to proceed with Task 3.2.1: Create simplified error enum.*
