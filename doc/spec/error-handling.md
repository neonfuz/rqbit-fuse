# Error Handling Specification

## Overview

This document specifies the unified error handling strategy for rqbit-fuse, providing a simplified, typed error system that maps directly to FUSE error codes.

## Design Philosophy

The error handling system uses a **single unified error type** (`RqbitFuseError`) rather than a hierarchical structure. This simplifies error handling throughout the codebase while maintaining comprehensive coverage of all error scenarios.

Key principles:
- **Single enum**: One error type for all operations (API, filesystem, configuration)
- **Direct FUSE mapping**: Each error variant maps to a specific libc error code
- **Automatic conversions**: `From` implementations for common external error types
- **Retry classification**: Built-in methods to identify transient vs. permanent errors

## Error Type

### RqbitFuseError Enum

```rust
use thiserror::Error;

/// Unified error type for rqbit-fuse with 11 essential variants.
#[derive(Error, Debug, Clone)]
pub enum RqbitFuseError {
    /// Entity not found (ENOENT)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Permission denied (EACCES)
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Operation timed out (ETIMEDOUT)
    #[error("Operation timed out: {0}")]
    TimedOut(String),

    /// Network error - covers server disconnected, connection failures
    #[error("Network error: {0}")]
    NetworkError(String),

    /// API returned error with HTTP status code
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),

    /// Invalid argument (EINVAL)
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Validation error with multiple issues
    #[error("Validation error: {}", .0.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    ValidationError(Vec<ValidationIssue>),

    /// Resource temporarily unavailable (EAGAIN)
    #[error("Resource temporarily unavailable: {0}")]
    NotReady(String),

    /// Parse/serialization error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Is a directory (EISDIR)
    #[error("Is a directory")]
    IsDirectory,

    /// Not a directory (ENOTDIR)
    #[error("Not a directory")]
    NotDirectory,
}

/// Represents a single validation error in the configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
}

/// Result type alias for operations that can fail with RqbitFuseError.
pub type RqbitFuseResult<T> = Result<T, RqbitFuseError>;
```

## FUSE Error Code Mapping

### Mapping Table

| Error Variant | FUSE Code | Description |
|--------------|-----------|-------------|
| `NotFound` | ENOENT | No such file or directory |
| `PermissionDenied` | EACCES | Permission denied |
| `TimedOut` | ETIMEDOUT | Operation timed out |
| `NetworkError` | ENETUNREACH | Network is unreachable |
| `ApiError { status: 400/416, .. }` | EINVAL | Invalid argument |
| `ApiError { status: 401/403, .. }` | EACCES | Permission denied |
| `ApiError { status: 404, .. }` | ENOENT | Not found |
| `ApiError { status: 408/423/429/503/504, .. }` | EAGAIN | Resource temporarily unavailable |
| `ApiError { status: 409, .. }` | EEXIST | File exists |
| `ApiError { status: 413, .. }` | EFBIG | File too large |
| `ApiError { status: 500/502, .. }` | EIO | I/O error |
| `IoError` | EIO | I/O error |
| `InvalidArgument` | EINVAL | Invalid argument |
| `ValidationError` | EINVAL | Invalid argument |
| `NotReady` | EAGAIN | Resource temporarily unavailable |
| `ParseError` | EINVAL | Invalid argument |
| `IsDirectory` | EISDIR | Is a directory |
| `NotDirectory` | ENOTDIR | Not a directory |

### Implementation

```rust
impl RqbitFuseError {
    /// Convert the error to a libc error code suitable for FUSE replies.
    pub fn to_errno(&self) -> i32 {
        match self {
            RqbitFuseError::NotFound(_) => libc::ENOENT,
            RqbitFuseError::PermissionDenied(_) => libc::EACCES,
            RqbitFuseError::TimedOut(_) => libc::ETIMEDOUT,
            RqbitFuseError::NetworkError(_) => libc::ENETUNREACH,
            RqbitFuseError::ApiError { status, .. } => match status {
                400 | 416 => libc::EINVAL,
                401 | 403 => libc::EACCES,
                404 => libc::ENOENT,
                408 | 423 | 429 | 503 | 504 => libc::EAGAIN,
                409 => libc::EEXIST,
                413 => libc::EFBIG,
                500 | 502 => libc::EIO,
                _ => libc::EIO,
            },
            RqbitFuseError::IoError(_) => libc::EIO,
            RqbitFuseError::InvalidArgument(_) => libc::EINVAL,
            RqbitFuseError::ValidationError(_) => libc::EINVAL,
            RqbitFuseError::NotReady(_) => libc::EAGAIN,
            RqbitFuseError::ParseError(_) => libc::EINVAL,
            RqbitFuseError::IsDirectory => libc::EISDIR,
            RqbitFuseError::NotDirectory => libc::ENOTDIR,
        }
    }
}
```

## From Implementations for External Errors

```rust
impl From<std::io::Error> for RqbitFuseError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => RqbitFuseError::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                RqbitFuseError::PermissionDenied(err.to_string())
            }
            std::io::ErrorKind::TimedOut => RqbitFuseError::TimedOut(err.to_string()),
            std::io::ErrorKind::InvalidInput => RqbitFuseError::InvalidArgument(err.to_string()),
            _ => RqbitFuseError::IoError(err.to_string()),
        }
    }
}

impl From<reqwest::Error> for RqbitFuseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            RqbitFuseError::TimedOut(err.to_string())
        } else if err.is_connect() {
            RqbitFuseError::NetworkError(format!("Server disconnected: {}", err))
        } else if err.is_request() {
            RqbitFuseError::NetworkError(err.to_string())
        } else {
            RqbitFuseError::IoError(format!("HTTP error: {}", err))
        }
    }
}

impl From<serde_json::Error> for RqbitFuseError {
    fn from(err: serde_json::Error) -> Self {
        RqbitFuseError::ParseError(err.to_string())
    }
}

impl From<toml::de::Error> for RqbitFuseError {
    fn from(err: toml::de::Error) -> Self {
        RqbitFuseError::ParseError(err.to_string())
    }
}
```

## ToFuseError Trait for anyhow::Error

```rust
/// Trait for converting errors to FUSE error codes.
pub trait ToFuseError {
    /// Convert the error to a FUSE error code.
    fn to_fuse_error(&self) -> i32;
}

impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types through downcasting
        if let Some(rqbit_err) = self.downcast_ref::<RqbitFuseError>() {
            return rqbit_err.to_errno();
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

## Retry Classification

```rust
impl RqbitFuseError {
    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            RqbitFuseError::TimedOut(_)
                | RqbitFuseError::NetworkError(_)
                | RqbitFuseError::NotReady(_)
                | RqbitFuseError::ApiError {
                    status: 408 | 429 | 502 | 503 | 504,
                    ..
                }
        )
    }

    /// Check if this error indicates the server is unavailable
    pub fn is_server_unavailable(&self) -> bool {
        matches!(
            self,
            RqbitFuseError::TimedOut(_) | RqbitFuseError::NetworkError(_)
        )
    }
}
```

## Usage in Filesystem

### Direct Error Usage

```rust
fn read(&mut self, ..., reply: fuser::ReplyData) {
    match self.do_read(ino, offset, size).await {
        Ok(data) => reply.data(&data),
        Err(e) => {
            let error_code = e.to_errno();
            reply.error(error_code);
        }
    }
}
```

### Creating Errors

```rust
// Not found error
return Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into());

// Permission denied
return Err(RqbitFuseError::PermissionDenied(format!(
    "Authentication failed: {}", message
)).into());

// Invalid argument
return Err(RqbitFuseError::InvalidArgument(format!(
    "Invalid range: start ({}) > end ({})", start, end
)).into());

// API error with status code
return Err(RqbitFuseError::ApiError {
    status: status.as_u16(),
    message,
}.into());
```

### Converting from anyhow::Error

```rust
// In async_bridge.rs - converting anyhow errors from API calls
let error_code = e.to_fuse_error();

// In filesystem.rs - downcasting to check error type
if let Some(api_err) = e.downcast_ref::<RqbitFuseError>() {
    if matches!(api_err, RqbitFuseError::ApiError { status: 404, .. }) {
        return Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into());
    }
}
```

## Library vs Application Error Separation

**Library Errors** (`src/error.rs`):
- Used by library code (api, fs, config modules)
- Implement `std::error::Error` trait via `thiserror`
- Provide structured error types with `RqbitFuseError`
- Can be converted to FUSE error codes via `to_errno()`

**Application Errors** (`src/main.rs`):
- Used by CLI and main application
- Use `anyhow` for convenient error handling
- Convert library errors with `.context()`
- Display user-friendly error messages

```rust
// In main.rs
use anyhow::{Context, Result};

async fn run_mount(...) -> Result<()> {
    let config = load_config(...)?;
    
    // Create API client with context
    let api_client = Arc::new(
        create_api_client(&config.api, Some(Arc::clone(&metrics)))
            .context("Failed to create API client")?,
    );
    
    // Create filesystem with context
    let fs = TorrentFS::new(config, Arc::clone(&metrics), async_worker)
        .context("Failed to create torrent filesystem")?;
    
    Ok(())
}
```

## Configuration Validation

```rust
impl Config {
    pub fn validate(&self) -> Result<(), RqbitFuseError> {
        let mut issues = Vec::new();
        
        // Validate API URL
        if self.api.url.is_empty() {
            issues.push(ValidationIssue {
                field: "api.url".to_string(),
                message: "API URL cannot be empty".to_string(),
            });
        }
        
        // Validate mount point
        if self.mount.mount_point.as_os_str().is_empty() {
            issues.push(ValidationIssue {
                field: "mount.mount_point".to_string(),
                message: "Mount point cannot be empty".to_string(),
            });
        }
        
        // Return validation error if there are issues
        if !issues.is_empty() {
            return Err(RqbitFuseError::ValidationError(issues));
        }
        
        Ok(())
    }
}
```

## Testing

### Unit Tests for Error Mapping

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_to_errno() {
        // Not found errors
        assert_eq!(
            RqbitFuseError::NotFound("test".to_string()).to_errno(),
            libc::ENOENT
        );

        // Permission errors
        assert_eq!(
            RqbitFuseError::PermissionDenied("test".to_string()).to_errno(),
            libc::EACCES
        );

        // Timeout errors
        assert_eq!(
            RqbitFuseError::TimedOut("test".to_string()).to_errno(),
            libc::ETIMEDOUT
        );

        // Network errors
        assert_eq!(
            RqbitFuseError::NetworkError("test".to_string()).to_errno(),
            libc::ENETUNREACH
        );

        // Directory errors
        assert_eq!(RqbitFuseError::IsDirectory.to_errno(), libc::EISDIR);
        assert_eq!(RqbitFuseError::NotDirectory.to_errno(), libc::ENOTDIR);

        // Resource errors
        assert_eq!(
            RqbitFuseError::NotReady("test".to_string()).to_errno(),
            libc::EAGAIN
        );
    }

    #[test]
    fn test_api_error_to_errno() {
        // API error status code mappings
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 400,
                message: "test".to_string()
            }
            .to_errno(),
            libc::EINVAL
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 404,
                message: "test".to_string()
            }
            .to_errno(),
            libc::ENOENT
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 429,
                message: "test".to_string()
            }
            .to_errno(),
            libc::EAGAIN
        );
    }

    #[test]
    fn test_is_transient() {
        assert!(RqbitFuseError::TimedOut("test".to_string()).is_transient());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_transient());
        assert!(RqbitFuseError::NotReady("test".to_string()).is_transient());
        assert!(RqbitFuseError::ApiError {
            status: 503,
            message: "test".to_string()
        }
        .is_transient());

        // Non-transient errors
        assert!(!RqbitFuseError::NotFound("test".to_string()).is_transient());
        assert!(!RqbitFuseError::InvalidArgument("test".to_string()).is_transient());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let rqbit_err: RqbitFuseError = io_err.into();
        assert!(matches!(rqbit_err, RqbitFuseError::NotFound(_)));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let rqbit_err: RqbitFuseError = io_err.into();
        assert!(matches!(rqbit_err, RqbitFuseError::PermissionDenied(_)));
    }

    #[test]
    fn test_anyhow_to_fuse_error() {
        let err = anyhow::Error::new(RqbitFuseError::NotFound("test".to_string()));
        assert_eq!(err.to_fuse_error(), libc::ENOENT);

        let err = anyhow::Error::new(RqbitFuseError::PermissionDenied("test".to_string()));
        assert_eq!(err.to_fuse_error(), libc::EACCES);
    }
}
```

## Error Handling Patterns

### Pattern 1: API Error Conversion

```rust
async fn get_torrent(&self, id: u64) -> Result<TorrentInfo> {
    let url = format!("{}/torrents/{}", self.base_url, id);
    
    match self.get_json::<TorrentInfo>(&endpoint, &url).await {
        Ok(torrent) => Ok(torrent),
        Err(e) => {
            // Check if it's a 404 error from the API
            if let Some(api_err) = e.downcast_ref::<RqbitFuseError>() {
                if matches!(api_err, RqbitFuseError::ApiError { status: 404, .. }) {
                    return Err(RqbitFuseError::NotFound(format!("torrent {}", id)).into());
                }
            }
            Err(e)
        }
    }
}
```

### Pattern 2: Retry Logic with Transient Errors

```rust
async fn execute_with_retry<F, Fut>(...) -> Result<reqwest::Response> {
    for attempt in 0..=self.max_retries {
        match operation().await {
            Ok(response) => { ... }
            Err(e) => {
                let api_error: RqbitFuseError = e.into();
                
                // Check if error is transient and we should retry
                if api_error.is_transient() && attempt < self.max_retries {
                    warn!("Transient error, retrying: {}", api_error);
                    sleep(self.retry_delay * (attempt + 1)).await;
                } else {
                    return Err(api_error.into());
                }
            }
        }
    }
}
```

### Pattern 3: FUSE Callback Error Handling

```rust
fn read(&mut self, ..., reply: fuser::ReplyData) {
    // ... validation ...
    
    let result = self.async_worker.read_file(torrent_id, file_index, offset, size, timeout);

    match result {
        Ok(data) => reply.data(&data),
        Err(e) => {
            let error_code = e.to_errno();
            error!("Failed to read file: {}", e);
            reply.error(error_code);
        }
    }
}
```

## References

- [Rust Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [FUSE Error Codes](https://github.com/libfuse/libfuse/blob/master/include/fuse_kernel.h)
- [anyhow Documentation](https://docs.rs/anyhow/latest/anyhow/)
- [thiserror Documentation](https://docs.rs/thiserror/latest/thiserror/)
- [libc Error Constants](https://docs.rs/libc/latest/libc/)
