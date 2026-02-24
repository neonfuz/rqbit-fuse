# Error Handling Specification

## Overview

This document specifies the simplified error handling strategy for rqbit-fuse, providing a minimal, typed error system that maps to FUSE error codes.

## Error Enum Hierarchy

```rust
/// The top-level error type for rqbit-fuse operations
#[derive(Error, Debug)]
pub enum TorrentFuseError {
    /// Errors from the rqbit HTTP API
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    
    /// Filesystem-level errors
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] FilesystemError),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    /// Network/IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors specific to API operations (8 essential variants)
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    /// Resource not found (torrent or file)
    #[error("Not found: {resource}")]
    NotFound { resource: String },
    
    /// API service temporarily unavailable (503, timeouts, circuit breaker)
    #[error("Service unavailable: {details}")]
    Unavailable { details: String },
    
    /// Network/connection failures
    #[error("Network error: {details}")]
    Network { details: String },
    
    /// Invalid request parameters
    #[error("Invalid argument: {details}")]
    InvalidArgument { details: String },
    
    /// Authentication/authorization failures
    #[error("Access denied: {details}")]
    AccessDenied { details: String },
    
    /// Resource already exists (conflict)
    #[error("Already exists: {resource}")]
    AlreadyExists { resource: String },
    
    /// HTTP client construction or internal error
    #[error("Internal error: {details}")]
    Internal { details: String },
    
    /// Request retry limit exceeded
    #[error("Retry limit exceeded after {attempts} attempts")]
    RetryLimitExceeded { attempts: u32 },
}

/// Errors specific to filesystem operations (8 essential variants)
#[derive(Error, Debug, Clone)]
pub enum FilesystemError {
    /// Inode or path not found
    #[error("Not found: {path}")]
    NotFound { path: String },
    
    /// Expected directory, got file
    #[error("Not a directory: {ino}")]
    NotADirectory { ino: u64 },
    
    /// Expected file, got directory
    #[error("Is a directory: {ino}")]
    IsADirectory { ino: u64 },
    
    /// Invalid path component or argument
    #[error("Invalid argument: {details}")]
    InvalidArgument { details: String },
    
    /// Permission denied or path traversal
    #[error("Permission denied: {details}")]
    PermissionDenied { details: String },
    
    /// File or directory already exists
    #[error("Already exists: {path}")]
    AlreadyExists { path: String },
    
    /// Directory not empty
    #[error("Directory not empty: {ino}")]
    DirectoryNotEmpty { ino: u64 },
    
    /// Read-only filesystem
    #[error("Read-only filesystem")]
    ReadOnly,
}

/// Errors specific to configuration (6 essential variants)
#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    /// Invalid configuration value
    #[error("Invalid configuration: {field} - {reason}")]
    InvalidValue { field: String, reason: String },
    
    /// Mount point does not exist or is not accessible
    #[error("Mount point error: {path} - {reason}")]
    MountPoint { path: String, reason: String },
    
    /// Configuration file not found or unreadable
    #[error("Configuration file: {path} - {reason}")]
    File { path: String, reason: String },
    
    /// URL parsing error
    #[error("Invalid URL: {url} - {reason}")]
    InvalidUrl { url: String, reason: String },
    
    /// Missing required configuration
    #[error("Missing required field: {field}")]
    MissingField { field: String },
    
    /// Timeout value out of range
    #[error("Invalid timeout: {value} - {reason}")]
    InvalidTimeout { value: u64, reason: String },
}
```

## FUSE Error Code Mapping

### Mapping Table (8 Essential Error Codes)

| Error Variant | FUSE Code | Description |
|--------------|-----------|-------------|
| `NotFound` | ENOENT | No such file or directory |
| `NotADirectory` | ENOTDIR | Not a directory |
| `IsADirectory` | EISDIR | Is a directory |
| `InvalidArgument` | EINVAL | Invalid argument |
| `PermissionDenied` / `AccessDenied` | EACCES | Permission denied |
| `AlreadyExists` | EEXIST | File exists |
| `DirectoryNotEmpty` | ENOTEMPTY | Directory not empty |
| `ReadOnly` / `Filesystem` | EROFS | Read-only filesystem |
| `Unavailable` / `RetryLimitExceeded` | EAGAIN | Resource temporarily unavailable |
| `Network` | ENETUNREACH | Network is unreachable |
| `Internal` / `Io` | EIO | I/O error |

### Implementation

```rust
impl TorrentFuseError {
    /// Convert to FUSE error code
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            TorrentFuseError::Api(api_err) => api_err.to_fuse_error(),
            TorrentFuseError::Filesystem(fs_err) => fs_err.to_fuse_error(),
            TorrentFuseError::Config(_) => libc::EINVAL,
            TorrentFuseError::Io(io_err) => match io_err.kind() {
                std::io::ErrorKind::NotFound => libc::ENOENT,
                std::io::ErrorKind::PermissionDenied => libc::EACCES,
                std::io::ErrorKind::AlreadyExists => libc::EEXIST,
                std::io::ErrorKind::WouldBlock => libc::EAGAIN,
                std::io::ErrorKind::InvalidInput => libc::EINVAL,
                std::io::ErrorKind::TimedOut => libc::EAGAIN,
                _ => libc::EIO,
            },
        }
    }
}

impl ApiError {
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            // Not found errors
            ApiError::NotFound { .. } => libc::ENOENT,
            
            // Service unavailable - suggest retry
            ApiError::Unavailable { .. } => libc::EAGAIN,
            ApiError::RetryLimitExceeded { .. } => libc::EAGAIN,
            
            // Network errors
            ApiError::Network { .. } => libc::ENETUNREACH,
            
            // Invalid input
            ApiError::InvalidArgument { .. } => libc::EINVAL,
            
            // Access denied
            ApiError::AccessDenied { .. } => libc::EACCES,
            
            // Already exists
            ApiError::AlreadyExists { .. } => libc::EEXIST,
            
            // Internal errors
            ApiError::Internal { .. } => libc::EIO,
        }
    }
}

impl FilesystemError {
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            FilesystemError::NotFound { .. } => libc::ENOENT,
            FilesystemError::NotADirectory { .. } => libc::ENOTDIR,
            FilesystemError::IsADirectory { .. } => libc::EISDIR,
            FilesystemError::InvalidArgument { .. } => libc::EINVAL,
            FilesystemError::PermissionDenied { .. } => libc::EACCES,
            FilesystemError::AlreadyExists { .. } => libc::EEXIST,
            FilesystemError::DirectoryNotEmpty { .. } => libc::ENOTEMPTY,
            FilesystemError::ReadOnly => libc::EROFS,
        }
    }
}
```

## From Implementations for External Errors

```rust
impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            ApiError::Unavailable { 
                details: format!("Request timeout: {}", err) 
            }
        } else if err.is_connect() {
            ApiError::Network { 
                details: format!("Connection failed: {}", err) 
            }
        } else if err.is_request() {
            ApiError::InvalidArgument { 
                details: format!("Invalid request: {}", err) 
            }
        } else {
            ApiError::Internal { 
                details: format!("HTTP error: {}", err) 
            }
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::InvalidArgument {
            details: format!("JSON parse error: {}", err),
        }
    }
}

impl From<reqwest::StatusCode> for ApiError {
    fn from(status: reqwest::StatusCode) -> Self {
        match status {
            StatusCode::NOT_FOUND => ApiError::NotFound {
                resource: "resource".to_string(),
            },
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => ApiError::AccessDenied {
                details: format!("HTTP {}", status),
            },
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => ApiError::InvalidArgument {
                details: format!("HTTP {}", status),
            },
            StatusCode::CONFLICT => ApiError::AlreadyExists {
                resource: "resource".to_string(),
            },
            StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => ApiError::Unavailable {
                details: format!("HTTP {}", status),
            },
            _ => ApiError::Internal {
                details: format!("HTTP {}", status),
            },
        }
    }
}

impl From<ApiError> for TorrentFuseError {
    fn from(err: ApiError) -> Self {
        TorrentFuseError::Api(err)
    }
}

impl From<FilesystemError> for TorrentFuseError {
    fn from(err: FilesystemError) -> Self {
        TorrentFuseError::Filesystem(err)
    }
}

impl From<ConfigError> for TorrentFuseError {
    fn from(err: ConfigError) -> Self {
        TorrentFuseError::Config(err)
    }
}
```

## Retry Classification

```rust
impl ApiError {
    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::Unavailable { .. } | ApiError::RetryLimitExceeded { .. }
        )
    }
    
    /// Check if this error indicates the server is unavailable
    pub fn is_server_unavailable(&self) -> bool {
        matches!(
            self,
            ApiError::Unavailable { .. } | ApiError::Network { .. }
        )
    }
    
    /// Check if this is a client error that won't be fixed by retry
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            ApiError::NotFound { .. } 
                | ApiError::InvalidArgument { .. }
                | ApiError::AccessDenied { .. }
                | ApiError::AlreadyExists { .. }
        )
    }
}
```

## Usage in Filesystem

```rust
fn read(&mut self, ..., reply: fuser::ReplyData) {
    match self.do_read(ino, offset, size).await {
        Ok(data) => reply.data(&data),
        Err(e) => {
            let error_code = e.to_fuse_error();
            reply.error(error_code);
        }
    }
}
```

## Library vs Application Error Separation

**Library Errors** (`src/error.rs`):
- Used by library code (api, fs, cache modules)
- Implement `std::error::Error` trait
- Provide structured error types
- Can be converted to FUSE error codes

**Application Errors** (`src/main.rs`, `src/cli.rs`):
- Used by CLI and main application
- Use `anyhow` for convenient error handling
- Convert library errors with context
- Display user-friendly error messages
- Exit with appropriate status codes

```rust
// In main.rs or CLI code
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let config = Config::load()
        .context("Failed to load configuration")?;
    
    let fs = TorrentFS::new(config)
        .context("Failed to create filesystem")?;
    
    fs.mount()
        .context("Failed to mount filesystem")?;
    
    Ok(())
}
```

## Error Context Preservation

```rust
use anyhow::Context;

async fn get_torrent(&self, id: u64) -> Result<TorrentInfo, TorrentFuseError> {
    let url = format!("{}/torrents/{}", self.base_url, id);
    
    let response = self
        .execute_with_retry(&url)
        .await
        .with_context(|| format!("Failed to request torrent {} from {}", id, url))?;
    
    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::NotFound {
            resource: format!("torrent {}", id),
        }.into()),
        status if !status.is_success() => {
            Err(ApiError::from(status).into())
        }
        _ => {
            response.json().await.map_err(|e| {
                ApiError::InvalidArgument {
                    details: format!("Failed to parse torrent {}: {}", id, e),
                }.into()
            })
        }
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
    fn test_not_found_mapping() {
        let err = ApiError::NotFound { resource: "test".to_string() };
        assert_eq!(err.to_fuse_error(), libc::ENOENT);
        assert!(!err.is_transient());
        assert!(err.is_client_error());
    }
    
    #[test]
    fn test_unavailable_mapping() {
        let err = ApiError::Unavailable { details: "down".to_string() };
        assert_eq!(err.to_fuse_error(), libc::EAGAIN);
        assert!(err.is_transient());
        assert!(err.is_server_unavailable());
    }
    
    fn test_filesystem_error_mappings() {
        assert_eq!(
            FilesystemError::NotFound { path: "/test".to_string() }.to_fuse_error(),
            libc::ENOENT
        );
        assert_eq!(
            FilesystemError::NotADirectory { ino: 1 }.to_fuse_error(),
            libc::ENOTDIR
        );
        assert_eq!(
            FilesystemError::ReadOnly.to_fuse_error(),
            libc::EROFS
        );
    }
}
```

## References

- [Rust Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [FUSE Error Codes](https://github.com/libfuse/libfuse/blob/master/include/fuse_kernel.h)
- [anyhow Documentation](https://docs.rs/anyhow/latest/anyhow/)
- [thiserror Documentation](https://docs.rs/thiserror/latest/thiserror/)
