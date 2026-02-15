//! # torrent-fuse
//!
//! A read-only FUSE filesystem that mounts BitTorrent torrents as virtual directories,
//! enabling seamless access to torrent content without waiting for full downloads.
//!
//! ## Overview
//!
//! This crate provides a FUSE filesystem that integrates with [rqbit](https://github.com/ikatson/rqbit)
//! to expose torrent files and directories through standard filesystem operations. Files are
//! downloaded on-demand when accessed, allowing you to stream videos or copy files while
//! the torrent is still downloading.
//!
//! ## Key Features
//!
//! - **On-demand downloading**: Files are downloaded only when accessed
//! - **Video streaming**: Watch videos while they download with seeking support
//! - **Smart caching**: LRU cache with TTL for metadata and file pieces
//! - **Resilient HTTP client**: Circuit breaker, exponential backoff, automatic retries
//! - **Unicode support**: Handles filenames in any language
//! - **Large file support**: Full 64-bit file sizes
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     User Filesystem                          │
//! │  /mnt/torrents/                                            │
//! │  ├── ubuntu-24.04.iso/                                      │
//! │  └── big-buck-bunny/                                        │
//! └─────────────────────────────────────────────────────────────┘
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  torrent-fuse FUSE Client                    │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │ FUSE Handler │  │ HTTP Client │  │ Cache Mgr   │       │
//! │  │ (fuser)      │  │ (reqwest)   │  │ (moka)      │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//!                               │
//!                         HTTP API
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    rqbit Server                              │
//! │  Exposes torrent files via HTTP on port 3030               │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! This crate can be used as a library or via the CLI binary. For library usage:
//!
//! ```ignore
//! use torrent_fuse::{run, Config};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::from_args()?;
//!     run(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Modules
//!
//! - [`api`] - HTTP client for rqbit API, streaming, and torrent management
//! - [`cache`] - LRU cache with TTL for metadata and pieces
//! - [`config`] - Configuration management via CLI, env vars, and config files
//! - [`fs`] - FUSE filesystem implementation
//! - [`metrics`] - Performance metrics collection
//! - [`types`] - Core data types (inodes, handles, attributes)
//!
//! ## Feature Flags
//!
//! Currently no feature flags are defined. This crate uses all features by default.
//!
//! ## Error Handling
//!
//! This crate uses [`anyhow`] for error handling, allowing flexible error propagation
//! throughout the application. FUSE-specific errors are mapped to appropriate error
//! codes in the [`fs::error`] module.
//!
//! ## Blocking Behavior
//!
//! The FUSE kernel interface is synchronous, but this crate uses async I/O internally.
//! The [`fs::async_bridge::AsyncFuseWorker`] bridges sync FUSE callbacks to async operations
//! by spawning tasks on a Tokio runtime.
//!
//! ### Blocking Operations
//!
//! The following operations may block the calling thread:
//! - **File reads**: HTTP requests to rqbit wait for data to download from peers
//! - **Torrent discovery**: API calls to list torrents and get metadata
//! - **Stream creation**: Establishing HTTP connections to rqbit
//!
//! ### Deadlock Warnings
//!
//! ⚠️ **Warning**: Do not call blocking operations from within FUSE callbacks while holding
//! a async mutex lock. This can cause deadlocks. The crate handles this by using
//! [`AsyncFuseWorker`] to move async operations to a separate task, avoiding the need for
//! `block_in_place` + `block_on` patterns which can deadlock with the Tokio runtime.
//!
//! ### Thread Safety
//!
//! All shared state is protected by either DashMap (lock-free), Tokio async mutexes, or
//! standard library mutexes as appropriate. The cache uses the `moka` crate which provides
//! thread-safe, lock-free reads with atomic eviction.
//!
//! ## Example: Reading a File
//!
//! When you read from a mounted torrent file:
//! 1. FUSE receives a read request from the kernel
//! 2. The offset is translated to an HTTP Range request
//! 3. rqbit downloads the required pieces from peers
//! 4. Data streams back through FUSE to the application
//!
//! ## Troubleshooting
//!
//! ### Common Issues
//!
//! **"Transport endpoint is not connected"**
//! The FUSE filesystem crashed or was killed. Unmount and remount:
//! ```bash
//! fusermount -u ~/torrents
//! torrent-fuse mount ~/torrents
//! ```
//!
//! **"Connection refused" to API**
//! rqbit server is not running. Start it:
//! ```bash
//! rqbit server start
//! ```
//!
//! **Permission denied errors**
//! The filesystem is read-only. Writing operations will fail.
//!
//! ### Performance Tips
//!
//! - **Use media players with buffering**: mpv, vlc, and other players buffer ahead, which triggers rqbit's readahead
//! - **Read sequentially**: Sequential reads enable read-ahead optimization (32MB default)
//! - **Wait for initial pieces**: First access to a file may be slow while pieces download
//! - **Tune cache settings**: Increase cache size for better metadata caching
//!
//! ### Debugging
//!
//! Run with verbose logging to debug issues:
//! ```bash
//! torrent-fuse mount ~/torrents -vv  # DEBUG level
//! torrent-fuse mount ~/torrents -vvv # TRACE level
//! ```
//!
//! Enable metrics logging to monitor performance:
//! ```toml
//! [logging]
//! metrics_enabled = true
//! metrics_interval = 60
//! ```
//!
//! ## Security Considerations
//!
//! ### Read-Only Filesystem
//!
//! This filesystem is intentionally read-only. All write operations (create, write, unlink, etc.)
//! return `EROFS` (Read-only file system). This prevents malicious or accidental modifications
//! to the filesystem.
//!
//! ### Path Traversal Prevention
//!
//! The filesystem sanitizes all filenames to prevent path traversal attacks:
//! - Special characters (`..`, `/`, `\0`) are stripped or rejected
//! - Control characters are removed
//! - Leading dots are preserved for hidden files but `..` is blocked
//! - Symlinks are validated to ensure they don't escape the torrent directory
//!
//! ### Resource Limits
//!
//! Several limits prevent resource exhaustion:
//! - **Cache size**: Configurable maximum cache entries (default: 1000)
//! - **File handles**: TTL-based eviction (1 hour) for orphaned handles
//! - **Mount point**: Validated to be an absolute path
//! - **Concurrent reads**: Configurable limit (default: 10)
//!
//! ### Error Information Leakage
//!
//! Error messages are designed to be informative without leaking sensitive information:
//! - API errors are logged at debug level with full details
//! - User-facing errors use generic messages (e.g., "Permission denied", "Not found")
//! - Internal error details (stack traces, file paths) are not exposed to FUSE
//!
//! ### TOCTOU Vulnerabilities
//!
//! The codebase uses atomic operations to minimize Time-Of-Check-Time-Of-Use (TOCTOU) races:
//! - Cache operations use atomic check-and-act patterns via `moka`
//! - Inode operations use `DashMap::entry()` API for atomic insertion
//! - Torrent discovery uses atomic compare-and-swap to prevent duplicate discovery
//!
//! ## See Also
//!
//! - [rqbit](https://github.com/ikatson/rqbit) - The BitTorrent client
//! - [fuser](https://github.com/cberner/fuser) - Rust FUSE bindings
//! - [FUSE kernel documentation](https://www.kernel.org/doc/Documentation/filesystems/fuse.txt)

// Re-exports
//
// The primary types and functions intended for public use.

pub mod api;
pub mod cache;
pub mod config;
pub mod fs;
pub mod metrics;
pub mod sharded_counter;
pub mod types;

/// Cache module re-exports.
///
/// See [`cache`] module for more details.
pub use cache::{Cache, CacheStats};

/// Configuration module re-exports.
///
/// See [`config`] module for more details.
pub use config::{CliArgs, Config};

/// Async worker for handling FUSE callbacks.
///
/// This worker bridges synchronous FUSE callbacks with asynchronous operations
/// by spawning tasks on a Tokio runtime. Required for mounting the filesystem.
pub use fs::async_bridge::AsyncFuseWorker;

/// The main filesystem implementation.
///
/// This is the core type that handles all FUSE operations. Use [`TorrentFS::new()`]
/// to create an instance, then call [`TorrentFS::mount()`] to mount it.
pub use fs::filesystem::TorrentFS;

/// Metrics collection for monitoring performance.
///
/// Tracks API call latency, cache hits/misses, FUSE operation counts, and other
/// useful metrics for debugging and optimization.
pub use metrics::Metrics;

/// High-performance counter for concurrent metrics.
///
/// A sharded counter that allows concurrent increment operations without locking.
/// Used internally for metrics that are updated frequently from multiple threads.
pub use sharded_counter::ShardedCounter;

use crate::api::create_api_client;
use anyhow::{Context, Result};
use std::sync::Arc;

/// Run the torrent-fuse filesystem.
///
/// This is the main entry point for using torrent-fuse as a library.
/// It sets up the metrics collection, API client, async worker, and filesystem,
/// then mounts the FUSE filesystem at the configured mount point.
///
/// # Arguments
///
/// * `config` - Configuration for the filesystem, API client, cache, and mount options
///
/// # Returns
///
/// Returns `Ok(())` on successful unmount, or an error if:
/// - API client creation fails
/// - Filesystem creation fails
/// - Mounting fails
/// - An error occurs during operation
///
/// # Example
///
/// ```ignore
/// use torrent_fuse::{run, Config};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::from_args()?;
///     run(config).await?;
///     Ok(())
/// }
/// ```
///
/// # Note
///
/// This function blocks until the filesystem is unmounted. It handles SIGINT and
/// SIGTERM gracefully, cleaning up resources on shutdown.
pub async fn run(config: Config) -> Result<()> {
    tracing::info!(operation = "startup", message = "torrent-fuse starting");
    tracing::debug!(config = ?config, "Configuration loaded");

    // Create metrics
    let metrics = Arc::new(Metrics::new());

    // Create API client for the async worker
    let api_client = Arc::new(
        create_api_client(&config.api, Arc::clone(&metrics.api))
            .context("Failed to create API client")?,
    );

    // Create async worker for FUSE callbacks
    // Channel capacity of 1000 allows for good concurrency without excessive memory use
    let async_worker = Arc::new(AsyncFuseWorker::new(api_client, Arc::clone(&metrics), 1000));

    // Create the filesystem with async worker
    let fs = TorrentFS::new(config, Arc::clone(&metrics), async_worker)
        .context("Failed to create torrent filesystem")?;

    // Discover existing torrents before mounting
    crate::fs::filesystem::discover_existing_torrents(&fs)
        .await
        .context("Failed to discover existing torrents")?;

    // Mount the filesystem (this blocks until unmounted)
    fs.mount().context("Failed to mount filesystem")?;

    // Log final metrics on shutdown
    metrics.log_full_summary();

    Ok(())
}
