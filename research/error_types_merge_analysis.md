# FuseError and ApiError Merge Analysis

**Date:** 2026-02-22
**Task:** Evaluate merging FuseError and ApiError types

## Analysis

### FuseError (src/fs/error.rs)
- **Purpose**: FUSE filesystem operation errors
- **Variants**: NotFound, PermissionDenied, TimedOut, IoError, NotReady, ChannelFull, WorkerDisconnected, InvalidArgument, IsDirectory, NotDirectory, DeviceBusy, ReadOnlyFilesystem
- **Key method**: `to_errno()` - maps to libc error codes for FUSE replies
- **Size**: 178 lines with tests

### ApiError (src/api/types.rs)
- **Purpose**: HTTP API interaction errors  
- **Variants**: HttpError, ClientInitializationError, RequestCloneError, ApiError, TorrentNotFound, FileNotFound, InvalidRange, RetryLimitExceeded, SerializationError, ConnectionTimeout, ReadTimeout, ServerDisconnected, CircuitBreakerOpen, NetworkError, ServiceUnavailable, AuthenticationError, DataUnavailable
- **Key feature**: Uses thiserror::Error derive macro
- **Integration**: Implements ToFuseError trait for FUSE error code conversion

## Current Integration

The error types are already well-integrated via the `ToFuseError` trait:

1. **ApiError → FUSE codes**: ApiError implements ToFuseError to convert to libc error codes
2. **anyhow::Error handling**: Can downcast both FuseError and ApiError from anyhow errors
3. **Clean separation**: Each type handles its domain-specific errors

## Merge Evaluation

**RECOMMENDATION: DO NOT MERGE**

### Reasons to keep separate:

1. **Separation of Concerns**
   - FuseError: Filesystem-level errors (FUSE operations)
   - ApiError: Network/HTTP-level errors (rqbit API)
   - Merging would create a "god enum" mixing unrelated domains

2. **Different Use Cases**
   - FuseError used in filesystem.rs for FUSE callbacks
   - ApiError used in client.rs for HTTP API calls
   - Different contexts require different error information

3. **Existing Integration is Clean**
   - ToFuseError trait provides conversion without coupling
   - anyhow::Error can handle both types via downcasting
   - No duplication of error handling logic

4. **Merge Would Complicate**
   - Combined enum would have 25+ variants
   - Would need to maintain backward compatibility
   - Would require refactoring across multiple modules
   - No clear benefit over current design

## Conclusion

The current error type design follows Rust best practices:
- Domain-specific error types
- Clean trait-based integration
- No unnecessary coupling

**No action needed** - the error types are appropriately designed and well-integrated.

## References

- `src/fs/error.rs` - FuseError definition and ToFuseError trait
- `src/api/types.rs` - ApiError definition
- Integration via `impl ToFuseError for ApiError` in error.rs
