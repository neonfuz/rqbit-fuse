use crate::api::client::RqbitClient;
use crate::error::{RqbitFuseError, RqbitFuseResult, ToFuseError};
use crate::metrics::Metrics;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, trace, warn};

/// Request sent from FUSE callback to async worker.
/// Contains all necessary information to execute an async operation
/// and a channel to send the response back.
#[derive(Debug)]
pub enum FuseRequest {
    /// Read file data from a torrent
    ReadFile {
        /// Torrent ID
        torrent_id: u64,
        /// File index within the torrent
        file_index: u64,
        /// Offset to start reading from
        offset: u64,
        /// Number of bytes to read
        size: usize,
        /// Timeout for the operation
        timeout: Duration,
        /// Channel to send the response
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    /// Check if pieces are available for a byte range
    CheckPiecesAvailable {
        /// Torrent ID
        torrent_id: u64,
        /// Starting byte offset
        offset: u64,
        /// Number of bytes to check
        size: u64,
        /// Timeout for the operation
        timeout: Duration,
        /// Channel to send the response
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    /// Remove/forget a torrent from rqbit
    ForgetTorrent {
        /// Torrent ID to remove
        torrent_id: u64,
        /// Channel to send the response
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
}

/// Response from async worker to FUSE callback.
/// Represents the result of an async operation.
#[derive(Debug, Clone)]
pub enum FuseResponse {
    /// File read was successful
    ReadSuccess { data: Vec<u8> },
    /// File read failed
    ReadError { error_code: i32, message: String },
    /// Pieces are available for the requested range
    PiecesAvailable,
    /// Pieces are not available for the requested range
    PiecesNotAvailable { reason: String },
    /// Torrent was successfully forgotten
    ForgetSuccess,
    /// Failed to forget torrent
    ForgetError { error_code: i32, message: String },
}

/// Async worker that handles FUSE requests in an async context.
/// Provides a bridge between synchronous FUSE callbacks and async I/O operations.
///
/// # Async/Sync Bridge Pattern
///
/// This struct solves the fundamental problem of calling async code from synchronous
/// FUSE callbacks. FUSE callbacks run in synchronous threads, but our HTTP operations
/// are async. Using `block_in_place()` + `block_on()` patterns causes deadlocks.
///
/// ## Channel Architecture
///
/// ### Request Channel: `tokio::sync::mpsc`
/// Used to send requests from sync FUSE callbacks to the async worker task.
/// We use tokio's async channel because:
/// - The worker task runs in an async context (tokio::spawn)
/// - Using `std::sync::mpsc` would block the async executor
/// - The `select!` macro requires async-aware channels
///
/// ### Response Channel: `std::sync::mpsc`
/// Used to send responses from the async worker back to sync FUSE callbacks.
/// We use std's sync channel because:
/// - FUSE callbacks run in synchronous threads
/// - We need `recv_timeout()` for timeout handling
/// - tokio::sync::mpsc doesn't provide blocking recv with timeout
///
/// This hybrid approach is the correct pattern for async/sync bridging.
///
/// # Example Flow
///
/// 1. FUSE callback (sync) calls `read_file()` on AsyncFuseWorker
/// 2. AsyncFuseWorker creates a `std::sync::mpsc` response channel
/// 3. AsyncFuseWorker sends request via `tokio::sync::mpsc` to worker task
/// 4. Worker task (async) receives request and spawns handling task
/// 5. Handling task performs async HTTP operations via API client
/// 6. Handling task sends response back via `std::sync::mpsc` channel
/// 7. AsyncFuseWorker blocks waiting for response with timeout
/// 8. FUSE callback receives result and returns to FUSE kernel
pub struct AsyncFuseWorker {
    /// Channel sender for submitting requests (tokio::sync::mpsc)
    ///
    /// Uses tokio's async channel because the worker runs in an async context.
    /// Std::sync::mpsc would block the tokio executor.
    request_tx: mpsc::Sender<FuseRequest>,
    /// Handle to the worker task for cleanup
    #[allow(dead_code)]
    worker_handle: Option<tokio::task::JoinHandle<()>>,
    /// Shutdown signal sender
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AsyncFuseWorker {
    /// Create a new async worker with the given API client and metrics.
    ///
    /// # Arguments
    /// * `api_client` - The HTTP client for rqbit API calls
    /// * `metrics` - Metrics collection for monitoring
    /// * `channel_capacity` - Maximum number of pending requests
    ///
    /// # Returns
    /// A new AsyncFuseWorker instance
    pub fn new(
        api_client: Arc<RqbitClient>,
        metrics: Arc<Metrics>,
        channel_capacity: usize,
    ) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<FuseRequest>(channel_capacity);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let worker_handle = tokio::spawn(async move {
            info!("AsyncFuseWorker started");

            loop {
                tokio::select! {
                    biased;

                    // Handle shutdown signal first
                    _ = &mut shutdown_rx => {
                        info!("AsyncFuseWorker received shutdown signal");
                        break;
                    }

                    // Handle incoming requests
                    Some(request) = request_rx.recv() => {
                        let api_client = Arc::clone(&api_client);
                        let metrics = Arc::clone(&metrics);

                        // Spawn a task for each request to allow concurrent processing
                        tokio::spawn(async move {
                            Self::handle_request(&api_client, &metrics, request).await;
                        });
                    }
                }
            }

            info!("AsyncFuseWorker shut down");
        });

        Self {
            request_tx,
            worker_handle: Some(worker_handle),
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Handle a single FUSE request.
    async fn handle_request(
        api_client: &Arc<RqbitClient>,
        metrics: &Arc<Metrics>,
        request: FuseRequest,
    ) {
        match request {
            FuseRequest::ReadFile {
                torrent_id,
                file_index,
                offset,
                size,
                timeout,
                response_tx,
            } => {
                trace!(
                    torrent_id = torrent_id,
                    file_index = file_index,
                    offset = offset,
                    size = size,
                    "Handling ReadFile request"
                );

                let start = std::time::Instant::now();

                let result = tokio::time::timeout(
                    timeout,
                    api_client.read_file_streaming(torrent_id, file_index as usize, offset, size),
                )
                .await;

                let latency = start.elapsed();

                let response = match result {
                    Ok(Ok(data)) => {
                        metrics.fuse.record_read(data.len() as u64, latency);
                        FuseResponse::ReadSuccess {
                            data: data.to_vec(),
                        }
                    }
                    Ok(Err(e)) => {
                        metrics.fuse.record_error();
                        let error_code = e.to_fuse_error();
                        FuseResponse::ReadError {
                            error_code,
                            message: e.to_string(),
                        }
                    }
                    Err(_) => {
                        metrics.fuse.record_error();
                        FuseResponse::ReadError {
                            error_code: libc::ETIMEDOUT,
                            message: "Operation timed out".to_string(),
                        }
                    }
                };

                // Ignore send failure (receiver dropped = FUSE timeout or cancelled)
                let _ = response_tx.send(response);
            }

            FuseRequest::CheckPiecesAvailable {
                torrent_id,
                offset,
                size,
                timeout,
                response_tx,
            } => {
                trace!(
                    torrent_id = torrent_id,
                    offset = offset,
                    size = size,
                    "Handling CheckPiecesAvailable request"
                );

                // Fetch torrent info to get piece_length
                let piece_length = match api_client.get_torrent(torrent_id).await {
                    Ok(info) => info.piece_length.unwrap_or(256 * 1024), // Default to 256KB
                    Err(_) => 256 * 1024,                                // Conservative default
                };

                let result = tokio::time::timeout(
                    timeout,
                    api_client.check_range_available(torrent_id, offset, size, piece_length),
                )
                .await;

                let response = match result {
                    Ok(Ok(true)) => FuseResponse::PiecesAvailable,
                    Ok(Ok(false)) => FuseResponse::PiecesNotAvailable {
                        reason: "Some pieces in the requested range are not available".to_string(),
                    },
                    Ok(Err(e)) => {
                        let error_code = e.to_fuse_error();
                        FuseResponse::ReadError {
                            error_code,
                            message: e.to_string(),
                        }
                    }
                    Err(_) => FuseResponse::ReadError {
                        error_code: libc::ETIMEDOUT,
                        message: "Piece availability check timed out".to_string(),
                    },
                };

                let _ = response_tx.send(response);
            }

            FuseRequest::ForgetTorrent {
                torrent_id,
                response_tx,
            } => {
                trace!(torrent_id = torrent_id, "Handling ForgetTorrent request");

                let result = api_client.forget_torrent(torrent_id).await;

                let response = match result {
                    Ok(_) => FuseResponse::ForgetSuccess,
                    Err(e) => {
                        let error_code = e.to_fuse_error();
                        FuseResponse::ForgetError {
                            error_code,
                            message: e.to_string(),
                        }
                    }
                };

                // Ignore send failure
                let _ = response_tx.send(response);
            }
        }
    }

    /// Send a request to the async worker and wait for a response.
    /// This is a synchronous method that can be called from FUSE callbacks.
    ///
    /// # Arguments
    /// * `request_builder` - A closure that builds the request with a response channel
    /// * `timeout` - Maximum time to wait for a response
    ///
    /// # Returns
    /// * `Ok(FuseResponse)` if successful
    /// * `Err(RqbitFuseError)` if the channel is full, worker disconnected, or timed out
    pub fn send_request<F>(
        &self,
        request_builder: F,
        timeout: Duration,
    ) -> RqbitFuseResult<FuseResponse>
    where
        F: FnOnce(std::sync::mpsc::Sender<FuseResponse>) -> FuseRequest,
    {
        // Use std::sync::mpsc for the response channel to get recv_timeout support
        let (tx, rx) = std::sync::mpsc::channel();
        let request = request_builder(tx);

        // Try to send the request without blocking
        match self.request_tx.try_send(request) {
            Ok(_) => {
                // Wait for response with timeout
                match rx.recv_timeout(timeout) {
                    Ok(response) => Ok(response),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        warn!("FUSE request timed out waiting for response");
                        Err(RqbitFuseError::TimedOut("request timed out".to_string()))
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        error!("Async worker disconnected while waiting for response");
                        Err(RqbitFuseError::IoError("worker disconnected".to_string()))
                    }
                }
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("FUSE request channel is full");
                Err(RqbitFuseError::IoError("channel full".to_string()))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!("Async worker channel is closed");
                Err(RqbitFuseError::IoError("worker disconnected".to_string()))
            }
        }
    }

    /// Convenience method to read a file from a torrent.
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent
    /// * `file_index` - Index of the file within the torrent
    /// * `offset` - Offset to start reading from
    /// * `size` - Number of bytes to read
    /// * `timeout` - Maximum time to wait for the operation
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` with the file data
    /// * `Err(RqbitFuseError)` if the operation failed
    pub fn read_file(
        &self,
        torrent_id: u64,
        file_index: u64,
        offset: u64,
        size: usize,
        timeout: Duration,
    ) -> RqbitFuseResult<Vec<u8>> {
        let response = self.send_request(
            |tx| FuseRequest::ReadFile {
                torrent_id,
                file_index,
                offset,
                size,
                timeout,
                response_tx: tx,
            },
            timeout + Duration::from_secs(5), // Add buffer for channel overhead
        )?;

        match response {
            FuseResponse::ReadSuccess { data } => Ok(data),
            FuseResponse::ReadError {
                error_code,
                message,
            } => Err(RqbitFuseError::IoError(format!(
                "Read failed (code {}): {}",
                error_code, message
            ))),
            _ => Err(RqbitFuseError::IoError(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Check if pieces are available for a byte range in a torrent.
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent
    /// * `offset` - Starting byte offset
    /// * `size` - Number of bytes to check
    /// * `timeout` - Maximum time to wait for the operation
    ///
    /// # Returns
    /// * `Ok(true)` if all pieces in the range are available
    /// * `Ok(false)` if any piece in the range is not available
    /// * `Err(RqbitFuseError)` if the operation failed
    pub fn check_pieces_available(
        &self,
        torrent_id: u64,
        offset: u64,
        size: u64,
        timeout: Duration,
    ) -> RqbitFuseResult<bool> {
        let response = self.send_request(
            |tx| FuseRequest::CheckPiecesAvailable {
                torrent_id,
                offset,
                size,
                timeout,
                response_tx: tx,
            },
            timeout + Duration::from_secs(5),
        )?;

        match response {
            FuseResponse::PiecesAvailable => Ok(true),
            FuseResponse::PiecesNotAvailable { .. } => Ok(false),
            FuseResponse::ReadError {
                error_code,
                message,
            } => Err(RqbitFuseError::IoError(format!(
                "Piece check failed (code {}): {}",
                error_code, message
            ))),
            _ => Err(RqbitFuseError::IoError(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Convenience method to forget/remove a torrent.
    ///
    /// # Arguments
    /// * `torrent_id` - ID of the torrent to forget
    /// * `timeout` - Maximum time to wait for the operation
    ///
    /// # Returns
    /// * `Ok(())` if successful
    /// * `Err(RqbitFuseError)` if the operation failed
    pub fn forget_torrent(&self, torrent_id: u64, timeout: Duration) -> RqbitFuseResult<()> {
        let response = self.send_request(
            |tx| FuseRequest::ForgetTorrent {
                torrent_id,
                response_tx: tx,
            },
            timeout,
        )?;

        match response {
            FuseResponse::ForgetSuccess => Ok(()),
            FuseResponse::ForgetError {
                error_code,
                message,
            } => Err(RqbitFuseError::IoError(format!(
                "Forget failed (code {}): {}",
                error_code, message
            ))),
            _ => Err(RqbitFuseError::IoError(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Shut down the async worker gracefully.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            info!("Sending shutdown signal to AsyncFuseWorker");
            let _ = tx.send(());
        }
    }
}

impl Drop for AsyncFuseWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuse_request_debug() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let request = FuseRequest::ReadFile {
            torrent_id: 1,
            file_index: 0,
            offset: 0,
            size: 1024,
            timeout: Duration::from_secs(5),
            response_tx: tx,
        };
        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("ReadFile"));
        assert!(debug_str.contains("torrent_id: 1"));
    }

    #[test]
    fn test_fuse_response_debug() {
        let response = FuseResponse::ReadSuccess {
            data: vec![1, 2, 3],
        };
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("ReadSuccess"));

        let response = FuseResponse::ForgetSuccess;
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("ForgetSuccess"));
    }
}
