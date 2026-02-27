use crate::api::client::RqbitClient;
use crate::error::{anyhow_to_errno, RqbitFuseError, RqbitFuseResult};
use crate::metrics::Metrics;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, trace};

/// Request sent from FUSE callback to async worker.
#[derive(Debug)]
pub enum FuseRequest {
    ReadFile {
        torrent_id: u64,
        file_index: u64,
        offset: u64,
        size: usize,
        timeout: Duration,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    CheckPiecesAvailable {
        torrent_id: u64,
        offset: u64,
        size: u64,
        timeout: Duration,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
    ForgetTorrent {
        torrent_id: u64,
        response_tx: std::sync::mpsc::Sender<FuseResponse>,
    },
}

/// Response from async worker to FUSE callback.
#[derive(Debug, Clone)]
pub enum FuseResponse {
    Success { data: Option<Vec<u8>> },
    Error { error_code: i32, message: String },
    PiecesAvailable,
    PiecesNotAvailable { reason: String },
}

/// Async worker that handles FUSE requests in an async context.
pub struct AsyncFuseWorker {
    request_tx: mpsc::Sender<FuseRequest>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AsyncFuseWorker {
    /// Create a new async worker with the given API client and metrics.
    pub fn new(
        api_client: Arc<RqbitClient>,
        metrics: Arc<Metrics>,
        channel_capacity: usize,
    ) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<FuseRequest>(channel_capacity);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
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
                trace!("ReadFile: t={} f={} off={} sz={}", torrent_id, file_index, offset, size);

                let start = std::time::Instant::now();

                let result = tokio::time::timeout(
                    timeout,
                    api_client.read_file_streaming(torrent_id, file_index as usize, offset, size),
                )
                .await;

                let _latency = start.elapsed();

                let response = match result {
                    Ok(Ok(data)) => {
                        metrics.record_read(data.len() as u64);
                        FuseResponse::Success { data: Some(data.to_vec()) }
                    }
                    Ok(Err(e)) => {
                        metrics.record_error();
                        FuseResponse::Error { error_code: anyhow_to_errno(&e), message: e.to_string() }
                    }
                    Err(_) => {
                        metrics.record_error();
                        FuseResponse::Error { error_code: libc::ETIMEDOUT, message: "Operation timed out".to_string() }
                    }
                };
                let _ = response_tx.send(response);
            }

            FuseRequest::CheckPiecesAvailable {
                torrent_id,
                offset,
                size,
                timeout,
                response_tx,
            } => {
                trace!("CheckPieces: t={} off={} sz={}", torrent_id, offset, size);

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
                    Ok(Ok(false)) => FuseResponse::PiecesNotAvailable { reason: "Pieces not available".to_string() },
                    Ok(Err(e)) => FuseResponse::Error { error_code: anyhow_to_errno(&e), message: e.to_string() },
                    Err(_) => FuseResponse::Error { error_code: libc::ETIMEDOUT, message: "Check timed out".to_string() },
                };
                let _ = response_tx.send(response);
            }

            FuseRequest::ForgetTorrent {
                torrent_id,
                response_tx,
            } => {
                trace!("ForgetTorrent: t={}", torrent_id);

                let response = match api_client.forget_torrent(torrent_id).await {
                    Ok(_) => FuseResponse::Success { data: None },
                    Err(e) => FuseResponse::Error { error_code: anyhow_to_errno(&e), message: e.to_string() },
                };
                let _ = response_tx.send(response);
            }
        }
    }

    /// Send a request to the async worker and wait for a response.
    pub fn send_request<F>(
        &self,
        request_builder: F,
        timeout: Duration,
    ) -> RqbitFuseResult<FuseResponse>
    where
        F: FnOnce(std::sync::mpsc::Sender<FuseResponse>) -> FuseRequest,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let request = request_builder(tx);

        match self.request_tx.try_send(request) {
            Ok(_) => match rx.recv_timeout(timeout) {
                Ok(response) => Ok(response),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(RqbitFuseError::TimedOut("request timed out".to_string())),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(RqbitFuseError::IoError("worker disconnected".to_string())),
            },
            Err(mpsc::error::TrySendError::Full(_)) => Err(RqbitFuseError::IoError("channel full".to_string())),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(RqbitFuseError::IoError("worker disconnected".to_string())),
        }
    }

    /// Read a file from a torrent.
    pub fn read_file(&self, torrent_id: u64, file_index: u64, offset: u64, size: usize, timeout: Duration) -> RqbitFuseResult<Vec<u8>> {
        match self.send_request(|tx| FuseRequest::ReadFile { torrent_id, file_index, offset, size, timeout, response_tx: tx }, timeout + Duration::from_secs(5))? {
            FuseResponse::Success { data: Some(data) } => Ok(data),
            FuseResponse::Error { error_code, message } => Err(RqbitFuseError::IoError(format!("Read failed (code {}): {}", error_code, message))),
            _ => Err(RqbitFuseError::IoError("Unexpected response".to_string())),
        }
    }

    /// Check if pieces are available for a byte range.
    pub fn check_pieces_available(&self, torrent_id: u64, offset: u64, size: u64, timeout: Duration) -> RqbitFuseResult<bool> {
        match self.send_request(|tx| FuseRequest::CheckPiecesAvailable { torrent_id, offset, size, timeout, response_tx: tx }, timeout + Duration::from_secs(5))? {
            FuseResponse::PiecesAvailable => Ok(true),
            FuseResponse::PiecesNotAvailable { .. } => Ok(false),
            FuseResponse::Error { error_code, message } => Err(RqbitFuseError::IoError(format!("Check failed (code {}): {}", error_code, message))),
            _ => Err(RqbitFuseError::IoError("Unexpected response".to_string())),
        }
    }

    /// Forget/remove a torrent.
    pub fn forget_torrent(&self, torrent_id: u64, timeout: Duration) -> RqbitFuseResult<()> {
        match self.send_request(|tx| FuseRequest::ForgetTorrent { torrent_id, response_tx: tx }, timeout)? {
            FuseResponse::Success { .. } => Ok(()),
            FuseResponse::Error { error_code, message } => Err(RqbitFuseError::IoError(format!("Forget failed (code {}): {}", error_code, message))),
            _ => Err(RqbitFuseError::IoError("Unexpected response".to_string())),
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
        let request = FuseRequest::ReadFile { torrent_id: 1, file_index: 0, offset: 0, size: 1024, timeout: Duration::from_secs(5), response_tx: tx };
        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("ReadFile"));
    }

    #[test]
    fn test_fuse_response_debug() {
        let response = FuseResponse::Success { data: Some(vec![1, 2, 3]) };
        assert!(format!("{:?}", response).contains("Success"));
    }
}
