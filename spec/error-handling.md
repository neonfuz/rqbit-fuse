# Error Handling Specification

## Overview

This document specifies the comprehensive error handling strategy for torrent-fuse, addressing current issues and providing a typed, context-rich error system that properly maps to FUSE error codes.

## Current Error Handling Issues

### 1. String-Based Error Detection (ERROR-001)

**Location**: `src/fs/filesystem.rs:1008-1018`

**Current Code**:
```rust
// Try to extract ApiError from anyhow error and use proper mapping
let error_code =
    if let Some(api_err) = e.downcast_ref::<crate::api::types::ApiError>() {
        api_err.to_fuse_error()
    } else if e.to_string().contains("not found") {
        libc::ENOENT
    } else if e.to_string().contains("range") {
        libc::EINVAL
    } else {
        libc::EIO
    };
```

**Problems**:
- Fragile string matching for error classification
- Case sensitivity issues
- Locale-dependent error messages may not match
- No compile-time checking of error types
- Cannot distinguish between different "not found" scenarios

**Impact**: Errors may be misclassified, leading to incorrect FUSE error codes returned to the kernel.

### 2. Silent Failures in list_torrents() (ERROR-002)

**Location**: `src/api/client.rs:320-338`

**Current Code**:
```rust
// Fetch full details for each torrent since /torrents doesn't include files
let mut full_torrents = Vec::with_capacity(data.torrents.len());
for basic_info in data.torrents {
    match self.get_torrent(basic_info.id).await {
        Ok(full_info) => {
            full_torrents.push(full_info);
        }
        Err(e) => {
            warn!(
                api_op = "list_torrents",
                id = basic_info.id,
                name = %basic_info.name,
                error = %e,
                "Failed to get full details for torrent"
            );
            // Continue without this torrent rather than failing entirely
        }
    }
}
```

**Problems**:
- Errors are logged but silently dropped
- Caller has no visibility into partial failures
- Cannot implement retry logic for specific torrents
- No way to inform user which torrents failed to load

**Impact**: Users see incomplete torrent lists without knowing some torrents failed to load.

### 3. Lost Error Context with unwrap_or_else() (ERROR-003)

**Location**: `src/api/client.rs:289-292`

**Current Code**:
```rust
let message = response
    .text()
    .await
    .unwrap_or_else(|_| "Unknown error".to_string());
```

**Problems**:
- Original error from `response.text()` is discarded
- "Unknown error" provides no diagnostic value
- Cannot distinguish between:
  - Connection reset while reading body
  - Invalid UTF-8 in response
  - Response body too large

**Impact**: Debugging API issues is difficult when error context is lost.

### 4. Panics from .expect() in API Client (ERROR-004)

**Locations**:
- `src/api/client.rs:142` - HTTP client construction
- `src/api/client.rs:170` - HTTP client construction
- `src/api/client.rs:797` - Health check client construction

**Current Code**:
```rust
let client = Client::builder()
    .timeout(Duration::from_secs(60))
    .pool_max_idle_per_host(10)
    .build()
    .expect("Failed to build HTTP client");
```

**Problems**:
- Panics on initialization failure
- No way to handle initialization errors gracefully
- In tests, this can crash the entire test suite
- No context about *why* client construction failed

**Impact**: Application crashes on startup if HTTP client cannot be built.

### 5. Request Clone Failure Not Handled

**Location**: `src/api/client.rs:541`

**Current Code**:
```rust
let response = self
    .execute_with_retry(&endpoint, || request.try_clone().unwrap().send())
    .await?;
```

**Problems**:
- `unwrap()` on `try_clone()` which can fail for non-cloneable bodies
- Panics instead of returning an error

## Typed Error Design

### Error Enum Hierarchy

```rust
/// The top-level error type for torrent-fuse operations
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
    
    /// Cache errors
    #[error("Cache error: {0}")]
    Cache(String),
}

/// Errors specific to API operations
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(String),
    
    /// API returned error status
    #[error("API returned error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    /// Torrent not found
    #[error("Torrent not found: {torrent_id}")]
    TorrentNotFound { torrent_id: u64 },
    
    /// File not found in torrent
    #[error("File not found in torrent {torrent_id}: file_idx={file_idx}")]
    FileNotFound { torrent_id: u64, file_idx: usize },
    
    /// Invalid range request
    #[error("Invalid range request: {details}")]
    InvalidRange { details: String },
    
    /// Retry limit exceeded
    #[error("Retry limit exceeded after {attempts} attempts")]
    RetryLimitExceeded { attempts: u32 },
    
    /// Serialization error
    #[error("Serialization error: {source}")]
    SerializationError { source: String },
    
    /// Connection timeout
    #[error("Connection timeout - rqbit server not responding")]
    ConnectionTimeout,
    
    /// Read timeout
    #[error("Read timeout - request took too long")]
    ReadTimeout,
    
    /// Server disconnected
    #[error("rqbit server disconnected")]
    ServerDisconnected,
    
    /// Circuit breaker open
    #[error("Circuit breaker open - too many failures")]
    CircuitBreakerOpen,
    
    /// Network error
    #[error("Network error: {details}")]
    NetworkError { details: String },
    
    /// Service unavailable
    #[error("Service unavailable: {details}")]
    ServiceUnavailable { details: String },
    
    /// Authentication failed
    #[error("Authentication failed: {details}")]
    AuthenticationFailed { details: String },
    
    /// Request body not cloneable
    #[error("Request body cannot be cloned for retry")]
    RequestNotCloneable,
    
    /// HTTP client construction failed
    #[error("Failed to build HTTP client: {details}")]
    ClientConstructionFailed { details: String },
}

/// Errors specific to filesystem operations
#[derive(Error, Debug, Clone)]
pub enum FilesystemError {
    /// Inode not found
    #[error("Inode not found: {ino}")]
    InodeNotFound { ino: u64 },
    
    /// Path not found
    #[error("Path not found: {path}")]
    PathNotFound { path: String },
    
    /// Not a directory
    #[error("Not a directory: {ino}")]
    NotADirectory { ino: u64 },
    
    /// Is a directory
    #[error("Is a directory: {ino}")]
    IsADirectory { ino: u64 },
    
    /// Invalid path component
    #[error("Invalid path component: {component}")]
    InvalidPathComponent { component: String },
    
    /// Path traversal attempt detected
    #[error("Path traversal attempt detected: {path}")]
    PathTraversalAttempt { path: String },
    
    /// File already exists
    #[error("File already exists: {path}")]
    FileExists { path: String },
    
    /// Directory not empty
    #[error("Directory not empty: {ino}")]
    DirectoryNotEmpty { ino: u64 },
    
    /// Read-only filesystem
    #[error("Read-only filesystem")]
    ReadOnlyFilesystem,
    
    /// Invalid file handle
    #[error("Invalid file handle: {fh}")]
    InvalidFileHandle { fh: u64 },
    
    /// File is busy (has open handles)
    #[error("File is busy: {ino}")]
    FileBusy { ino: u64 },
}

/// Errors specific to configuration
#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    /// Invalid URL format
    #[error("Invalid URL: {url} - {reason}")]
    InvalidUrl { url: String, reason: String },
    
    /// Invalid timeout value
    #[error("Invalid timeout: {value} - {reason}")]
    InvalidTimeout { value: u64, reason: String },
    
    /// Mount point does not exist
    #[error("Mount point does not exist: {path}")]
    MountPointNotFound { path: String },
    
    /// Mount point is not a directory
    #[error("Mount point is not a directory: {path}")]
    MountPointNotDirectory { path: String },
    
    /// No permission to access mount point
    #[error("No permission to access mount point: {path}")]
    MountPointNoPermission { path: String },
    
    /// Configuration file not found
    #[error("Configuration file not found: {path}")]
    ConfigFileNotFound { path: String },
    
    /// Configuration parse error
    #[error("Configuration parse error: {source}")]
    ConfigParseError { source: String },
    
    /// Missing required field
    #[error("Missing required configuration field: {field}")]
    MissingField { field: String },
}
```

### FUSE Error Code Mapping Strategy

```rust
impl TorrentFuseError {
    /// Convert to FUSE error code
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            // API errors
            TorrentFuseError::Api(api_err) => api_err.to_fuse_error(),
            
            // Filesystem errors
            TorrentFuseError::Filesystem(fs_err) => fs_err.to_fuse_error(),
            
            // Config errors - typically EINVAL for bad configuration
            TorrentFuseError::Config(_) => libc::EINVAL,
            
            // IO errors - map from io::ErrorKind
            TorrentFuseError::Io(io_err) => match io_err.kind() {
                std::io::ErrorKind::NotFound => libc::ENOENT,
                std::io::ErrorKind::PermissionDenied => libc::EACCES,
                std::io::ErrorKind::AlreadyExists => libc::EEXIST,
                std::io::ErrorKind::WouldBlock => libc::EAGAIN,
                std::io::ErrorKind::InvalidInput => libc::EINVAL,
                std::io::ErrorKind::TimedOut => libc::ETIMEDOUT,
                _ => libc::EIO,
            },
            
            // Cache errors - internal error
            TorrentFuseError::Cache(_) => libc::EIO,
        }
    }
}

impl ApiError {
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            // Not found errors -> ENOENT
            ApiError::TorrentNotFound { .. } => libc::ENOENT,
            ApiError::FileNotFound { .. } => libc::ENOENT,
            
            // HTTP status code mappings
            ApiError::ApiError { status, .. } => match status {
                400 => libc::EINVAL,  // Bad request
                401 => libc::EACCES,  // Unauthorized
                403 => libc::EACCES,  // Forbidden
                404 => libc::ENOENT,  // Not found
                408 => libc::EAGAIN,  // Request timeout
                409 => libc::EEXIST,  // Conflict
                413 => libc::EFBIG,   // Payload too large
                416 => libc::EINVAL,  // Range not satisfiable
                423 => libc::EAGAIN,  // Locked
                429 => libc::EAGAIN,  // Too many requests
                500 => libc::EIO,     // Internal server error
                502 => libc::EIO,     // Bad gateway
                503 => libc::EAGAIN,  // Service unavailable
                504 => libc::EAGAIN,  // Gateway timeout
                _ => libc::EIO,
            },
            
            // Invalid input -> EINVAL
            ApiError::InvalidRange { .. } => libc::EINVAL,
            ApiError::SerializationError { .. } => libc::EIO,
            
            // Timeout -> EAGAIN (suggest retry)
            ApiError::ConnectionTimeout => libc::EAGAIN,
            ApiError::ReadTimeout => libc::EAGAIN,
            
            // Server unavailable -> ENOTCONN (endpoint not connected)
            ApiError::ServerDisconnected => libc::ENOTCONN,
            ApiError::NetworkError { .. } => libc::ENETUNREACH,
            ApiError::ServiceUnavailable { .. } => libc::EAGAIN,
            
            // Circuit breaker -> EAGAIN (temporary unavailable)
            ApiError::CircuitBreakerOpen => libc::EAGAIN,
            
            // Retry limit -> EAGAIN (caller may want to retry)
            ApiError::RetryLimitExceeded { .. } => libc::EAGAIN,
            
            // Client construction -> EIO (initialization failure)
            ApiError::ClientConstructionFailed { .. } => libc::EIO,
            
            // Request not cloneable -> EIO (internal error)
            ApiError::RequestNotCloneable => libc::EIO,
            
            // Generic HTTP errors
            ApiError::HttpError(_) => libc::EIO,
            
            // Auth failed -> EACCES
            ApiError::AuthenticationFailed { .. } => libc::EACCES,
        }
    }
}

impl FilesystemError {
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            FilesystemError::InodeNotFound { .. } => libc::ENOENT,
            FilesystemError::PathNotFound { .. } => libc::ENOENT,
            FilesystemError::NotADirectory { .. } => libc::ENOTDIR,
            FilesystemError::IsADirectory { .. } => libc::EISDIR,
            FilesystemError::InvalidPathComponent { .. } => libc::EINVAL,
            FilesystemError::PathTraversalAttempt { .. } => libc::EACCES,
            FilesystemError::FileExists { .. } => libc::EEXIST,
            FilesystemError::DirectoryNotEmpty { .. } => libc::ENOTEMPTY,
            FilesystemError::ReadOnlyFilesystem => libc::EROFS,
            FilesystemError::InvalidFileHandle { .. } => libc::EBADF,
            FilesystemError::FileBusy { .. } => libc::EBUSY,
        }
    }
}
```

### Library vs Application Error Separation

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

### Error Chaining with Context

```rust
use anyhow::Context;

// Preserve error chain with context
async fn list_torrents(&self) -> Result<Vec<TorrentInfo>, TorrentFuseError> {
    let url = format!("{}/torrents", self.base_url);
    
    let response = self
        .execute_with_retry("/torrents", || self.client.get(&url).send())
        .await
        .with_context(|| format!("Failed to fetch torrent list from {}", url))?;
    
    let data: TorrentListResponse = response
        .json()
        .await
        .map_err(|e| ApiError::SerializationError {
            source: e.to_string(),
        })
        .with_context(|| "Failed to parse torrent list response")?;
    
    // Handle partial failures
    let mut full_torrents = Vec::with_capacity(data.torrents.len());
    let mut errors = Vec::new();
    
    for basic_info in data.torrents {
        match self.get_torrent(basic_info.id).await {
            Ok(full_info) => full_torrents.push(full_info),
            Err(e) => {
                errors.push((basic_info.id, basic_info.name, e));
            }
        }
    }
    
    // Return partial success with error information
    if !errors.is_empty() {
        return Err(TorrentFuseError::PartialListFailure {
            successful: full_torrents,
            failed: errors,
        });
    }
    
    Ok(full_torrents)
}
```

## Implementation

### From Implementations for External Errors

```rust
// src/error.rs

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            // Distinguish between connection and read timeouts
            if err.to_string().contains("connect") {
                ApiError::ConnectionTimeout
            } else {
                ApiError::ReadTimeout
            }
        } else if err.is_connect() {
            ApiError::ServerDisconnected
        } else if err.is_request() {
            ApiError::NetworkError {
                details: err.to_string(),
            }
        } else if err.is_body() {
            ApiError::NetworkError {
                details: format!("Failed to read response body: {}", err),
            }
        } else {
            ApiError::HttpError(err.to_string())
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::SerializationError {
            source: err.to_string(),
        }
    }
}

impl From<std::io::Error> for TorrentFuseError {
    fn from(err: std::io::Error) -> Self {
        TorrentFuseError::Io(err)
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

### Map to FUSE Error Function

```rust
// src/fs/error_mapping.rs

/// Trait for types that can be converted to FUSE error codes
pub trait ToFuseError {
    fn to_fuse_error(&self) -> libc::c_int;
}

/// Helper function to map any error to a FUSE error code
pub fn map_to_fuse_error<E>(err: &E) -> libc::c_int
where
    E: ToFuseError,
{
    err.to_fuse_error()
}

/// Map an anyhow error to FUSE error code by attempting to downcast
pub fn map_anyhow_to_fuse(err: &anyhow::Error) -> libc::c_int {
    // Try to downcast to our typed errors
    if let Some(api_err) = err.downcast_ref::<ApiError>() {
        return api_err.to_fuse_error();
    }
    
    if let Some(fs_err) = err.downcast_ref::<FilesystemError>() {
        return fs_err.to_fuse_error();
    }
    
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        return match io_err.kind() {
            std::io::ErrorKind::NotFound => libc::ENOENT,
            std::io::ErrorKind::PermissionDenied => libc::EACCES,
            std::io::ErrorKind::AlreadyExists => libc::EEXIST,
            _ => libc::EIO,
        };
    }
    
    // Default to EIO for unknown errors
    libc::EIO
}

// Usage in filesystem.rs
fn read(&mut self, ..., reply: fuser::ReplyData) {
    match self.do_read(ino, offset, size).await {
        Ok(data) => reply.data(&data),
        Err(e) => {
            let error_code = map_anyhow_to_fuse(&e);
            reply.error(error_code);
        }
    }
}
```

### Error Propagation Patterns

**Pattern 1: Early Return with Context**
```rust
async fn get_torrent(&self, id: u64) -> Result<TorrentInfo, ApiError> {
    let url = format!("{}/torrents/{}", self.base_url, id);
    
    let response = self.client
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::from(e))
        .with_context(|| format!("Failed to request torrent {} from {}", id, url))?;
    
    match response.status() {
        StatusCode::NOT_FOUND => Err(ApiError::TorrentNotFound { torrent_id: id }),
        status if !status.is_success() => {
            let message = response
                .text()
                .await
                .map_err(|e| ApiError::NetworkError {
                    details: format!("Failed to read error response: {}", e),
                })?;
            Err(ApiError::ApiError {
                status: status.as_u16(),
                message,
            })
        }
        _ => {
            response.json().await.map_err(|e| ApiError::SerializationError {
                source: format!("Failed to parse torrent {}: {}", id, e),
            })
        }
    }
}
```

**Pattern 2: Accumulate Errors for Partial Results**
```rust
#[derive(Debug)]
pub struct ListTorrentsResult {
    pub torrents: Vec<TorrentInfo>,
    pub errors: Vec<(u64, String, ApiError)>, // (id, name, error)
}

impl ListTorrentsResult {
    pub fn is_partial(&self) -> bool {
        !self.errors.is_empty()
    }
    
    pub fn has_successes(&self) -> bool {
        !self.torrents.is_empty()
    }
}

pub async fn list_torrents(&self) -> Result<ListTorrentsResult, ApiError> {
    let basic_list = self.fetch_torrent_list().await?;
    
    let mut result = ListTorrentsResult {
        torrents: Vec::with_capacity(basic_list.len()),
        errors: Vec::new(),
    };
    
    for basic in basic_list {
        match self.get_torrent(basic.id).await {
            Ok(full) => result.torrents.push(full),
            Err(e) => result.errors.push((basic.id, basic.name, e)),
        }
    }
    
    Ok(result)
}
```

**Pattern 3: Replace Panics with Results**
```rust
// Before
pub fn new(base_url: String, metrics: Arc<ApiMetrics>) -> Self {
    let client = Client::builder()
        .build()
        .expect("Failed to build HTTP client"); // Panic!
    ...
}

// After
pub fn new(base_url: String, metrics: Arc<ApiMetrics>) -> Result<Self, ApiError> {
    let client = Client::builder()
        .build()
        .map_err(|e| ApiError::ClientConstructionFailed {
            details: e.to_string(),
        })?;
    ...
}
```

**Pattern 4: Preserve Error Context in Closures**
```rust
// Before
let message = response
    .text()
    .await
    .unwrap_or_else(|_| "Unknown error".to_string());

// After
let message = match response.text().await {
    Ok(text) => text,
    Err(e) => {
        return Err(ApiError::NetworkError {
            details: format!("Failed to read error response body: {}", e),
        })
    }
};
```

### Context Preservation

```rust
use anyhow::Context;

// Add context at each layer
async fn operation() -> Result<(), TorrentFuseError> {
    // Low-level: raw error
    let response = client.get(url).send().await
        .map_err(ApiError::from)?;
    
    // Mid-level: add context about what we were doing
    let data = response.json().await
        .map_err(|e| ApiError::SerializationError { source: e.to_string() })
        .with_context(|| format!("Failed to parse response from {}", url))?;
    
    // High-level: add context about the operation
    process_data(data)
        .with_context(|| "Failed to process API response")?;
    
    Ok(())
}

// Error chain will show:
// Error: Failed to process API response
// 
// Caused by:
//   0: Failed to parse response from http://localhost:3030/torrents/1
//   1: Serialization error: missing field `name` at line 1 column 45
//   2: missing field `name` at line 1 column 45
```

## Required Error Types

### Core Error Types

| Error Type | Module | Description | FUSE Code |
|------------|--------|-------------|-----------|
| `TorrentNotFound` | api | Torrent ID doesn't exist | ENOENT |
| `FileNotFound` | api | File index invalid for torrent | ENOENT |
| `ApiUnavailable` | api | HTTP 503 from rqbit | EAGAIN |
| `Timeout` | api | Request timeout | EAGAIN |
| `NetworkError` | api | Connection/transport failure | ENETUNREACH |
| `InvalidArgument` | api | Bad request parameters | EINVAL |
| `CircuitBreakerOpen` | api | Too many failures, backing off | EAGAIN |
| `AuthenticationFailed` | api | Auth token/API key rejected | EACCES |
| `InodeNotFound` | fs | Inode doesn't exist in table | ENOENT |
| `PathNotFound` | fs | Path resolution failed | ENOENT |
| `NotADirectory` | fs | Expected directory, got file | ENOTDIR |
| `IsADirectory` | fs | Expected file, got directory | EISDIR |
| `ReadOnlyFilesystem` | fs | Write operation attempted | EROFS |
| `FileBusy` | fs | File has open handles | EBUSY |
| `InvalidUrl` | config | URL parsing failed | EINVAL |
| `MountPointNotFound` | config | Mount path doesn't exist | EINVAL |
| `InvalidTimeout` | config | Timeout value out of range | EINVAL |

### Retry Classification

```rust
impl ApiError {
    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::ConnectionTimeout
                | ApiError::ReadTimeout
                | ApiError::ServerDisconnected
                | ApiError::NetworkError { .. }
                | ApiError::ServiceUnavailable { .. }
                | ApiError::CircuitBreakerOpen
                | ApiError::RetryLimitExceeded { .. }
                | ApiError::ApiError { status: 408 | 429 | 502 | 503 | 504, .. }
        )
    }
    
    /// Check if this error indicates the server is unavailable
    pub fn is_server_unavailable(&self) -> bool {
        matches!(
            self,
            ApiError::ConnectionTimeout
                | ApiError::ServerDisconnected
                | ApiError::NetworkError { .. }
                | ApiError::ServiceUnavailable { .. }
                | ApiError::CircuitBreakerOpen
        )
    }
    
    /// Check if this is a client error (4xx) that won't be fixed by retry
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            ApiError::TorrentNotFound { .. }
                | ApiError::FileNotFound { .. }
                | ApiError::InvalidRange { .. }
                | ApiError::AuthenticationFailed { .. }
                | ApiError::ApiError { status: 400..=499, .. }
        )
    }
}
```

## Testing Error Handling

### Unit Tests for Error Mapping

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_torrent_not_found_mapping() {
        let err = ApiError::TorrentNotFound { torrent_id: 42 };
        assert_eq!(err.to_fuse_error(), libc::ENOENT);
        assert!(!err.is_transient());
    }
    
    #[test]
    fn test_timeout_mapping() {
        let err = ApiError::ConnectionTimeout;
        assert_eq!(err.to_fuse_error(), libc::EAGAIN);
        assert!(err.is_transient());
        assert!(err.is_server_unavailable());
    }
    
    #[test]
    fn test_circuit_breaker_mapping() {
        let err = ApiError::CircuitBreakerOpen;
        assert_eq!(err.to_fuse_error(), libc::EAGAIN);
        assert!(err.is_transient());
        assert!(err.is_server_unavailable());
    }
    
    #[test]
    fn test_http_status_mappings() {
        let test_cases = vec![
            (400, libc::EINVAL),
            (401, libc::EACCES),
            (403, libc::EACCES),
            (404, libc::ENOENT),
            (408, libc::EAGAIN),
            (429, libc::EAGAIN),
            (500, libc::EIO),
            (503, libc::EAGAIN),
        ];
        
        for (status, expected) in test_cases {
            let err = ApiError::ApiError {
                status,
                message: "test".to_string(),
            };
            assert_eq!(
                err.to_fuse_error(),
                expected,
                "Status {} should map to {}",
                status, expected
            );
        }
    }
    
    #[test]
    fn test_filesystem_error_mappings() {
        assert_eq!(
            FilesystemError::InodeNotFound { ino: 1 }.to_fuse_error(),
            libc::ENOENT
        );
        assert_eq!(
            FilesystemError::NotADirectory { ino: 1 }.to_fuse_error(),
            libc::ENOTDIR
        );
        assert_eq!(
            FilesystemError::ReadOnlyFilesystem.to_fuse_error(),
            libc::EROFS
        );
    }
}
```

### Integration Tests for Error Propagation

```rust
#[tokio::test]
async fn test_list_torrents_partial_failure() {
    let mock_server = MockServer::start().await;
    let client = create_test_client(&mock_server);
    
    // Mock successful torrent list
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "torrents": [
                {"id": 1, "name": "Torrent 1"},
                {"id": 2, "name": "Torrent 2"},
            ]
        })))
        .mount(&mock_server)
        .await;
    
    // First torrent succeeds
    Mock::given(method("GET"))
        .and(path("/torrents/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "name": "Torrent 1", "files": []
        })))
        .mount(&mock_server)
        .await;
    
    // Second torrent fails
    Mock::given(method("GET"))
        .and(path("/torrents/2"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;
    
    let result = client.list_torrents().await.unwrap();
    
    // Should have partial results
    assert_eq!(result.torrents.len(), 1);
    assert_eq!(result.errors.len(), 1);
    assert!(matches!(
        result.errors[0].2,
        ApiError::TorrentNotFound { torrent_id: 2 }
    ));
}
```

### Mock Testing Error Scenarios

```rust
#[tokio::test]
async fn test_retry_exhaustion() {
    let mock_server = MockServer::start().await;
    let client = RqbitClient::with_config(
        mock_server.uri(),
        2, // 2 retries
        Duration::from_millis(10),
        Arc::new(ApiMetrics::new()),
    );
    
    // All requests fail with 503
    Mock::given(method("GET"))
        .and(path("/torrents"))
        .respond_with(ResponseTemplate::new(503))
        .expect(3) // Initial + 2 retries
        .mount(&mock_server)
        .await;
    
    let result = client.list_torrents().await;
    assert!(result.is_err());
    
    let err = result.unwrap_err().downcast::<ApiError>().unwrap();
    assert!(matches!(err, ApiError::RetryLimitExceeded { attempts: 3 }));
}
```

## Migration Plan

### Phase 1: Define New Error Types
1. Create `src/error.rs` with new error hierarchy
2. Implement `From` traits for external errors
3. Implement FUSE error mapping
4. Add comprehensive unit tests

### Phase 2: Update API Client
1. Replace `.expect()` calls with `Result` returns
2. Update `list_torrents()` to return partial results
3. Fix `check_response()` to preserve error context
4. Handle `try_clone()` failure gracefully

### Phase 3: Update Filesystem
1. Replace string-based error detection with typed errors
2. Update all FUSE callbacks to use new error mapping
3. Add context to error propagation
4. Test error scenarios

### Phase 4: Update Main/CLI
1. Add error context at application level
2. Implement user-friendly error display
3. Add appropriate exit codes
4. Test end-to-end error handling

## References

- [Rust Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [FUSE Error Codes](https://github.com/libfuse/libfuse/blob/master/include/fuse_kernel.h)
- [anyhow Documentation](https://docs.rs/anyhow/latest/anyhow/)
- [thiserror Documentation](https://docs.rs/thiserror/latest/thiserror/)
