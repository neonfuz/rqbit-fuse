use crate::api::types::ApiError;
use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::{Client, StatusCode};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, trace};

/// Maximum bytes to skip in an existing stream before creating a new connection
/// If we need to seek forward less than this, we'll read and discard bytes
/// If we need to seek more, we'll create a new HTTP connection
const MAX_SEEK_FORWARD: u64 = 10 * 1024 * 1024; // 10MB

/// Idle timeout for persistent streams before they're closed
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Cleanup interval for checking idle streams
const CLEANUP_INTERVAL: Duration = Duration::from_secs(10);

/// Unique identifier for a file stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StreamKey {
    torrent_id: u64,
    file_idx: usize,
}

/// Type alias for the byte stream from reqwest
type ByteStream = Pin<Box<dyn futures::Stream<Item = reqwest::Result<Bytes>> + Send>>;

/// State of a persistent HTTP stream for reading torrent file data
struct PersistentStream {
    /// HTTP response body stream
    stream: ByteStream,
    /// Current byte position in the stream
    current_position: u64,
    /// Last access time for idle detection
    last_access: Instant,
    /// Whether the stream is still valid
    is_valid: bool,
    /// Buffer for partial chunk data
    pending_buffer: Option<Bytes>,
}

impl PersistentStream {
    /// Create a new persistent stream starting at the given offset
    async fn new(
        client: &Client,
        base_url: &str,
        torrent_id: u64,
        file_idx: usize,
        start_offset: u64,
    ) -> Result<Self> {
        let url = format!("{}/torrents/{}/stream/{}", base_url, torrent_id, file_idx);

        trace!(
            stream_op = "create",
            torrent_id = torrent_id,
            file_idx = file_idx,
            start_offset = start_offset,
            "Creating new persistent stream"
        );

        // Request from the start offset to get a stream we can read sequentially
        let range_header = format!("bytes={}-", start_offset);
        let response = client
            .get(&url)
            .header("Range", range_header)
            .send()
            .await
            .context("Failed to create persistent stream")?;

        let status = response.status();

        // Check if we got a successful response
        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(
                ApiError::HttpError(format!("Failed to create stream: HTTP {}", status)).into(),
            );
        }

        // Check if server returned 200 OK for a range request (rqbit bug workaround)
        let is_full_response = status == StatusCode::OK && start_offset > 0;

        if is_full_response {
            debug!(
                stream_op = "created",
                torrent_id = torrent_id,
                file_idx = file_idx,
                start_offset = start_offset,
                status = %status,
                "Server returned full file, will skip to offset"
            );
        } else {
            debug!(
                stream_op = "created",
                torrent_id = torrent_id,
                file_idx = file_idx,
                start_offset = start_offset,
                status = %status,
                "Persistent stream created"
            );
        }

        // Convert response to byte stream
        let stream: ByteStream = Box::pin(response.bytes_stream());

        let mut persistent_stream = Self {
            stream,
            current_position: 0, // Will be updated after potential skip
            last_access: Instant::now(),
            is_valid: true,
            pending_buffer: None,
        };

        // If server returned full file, skip to the requested offset
        if is_full_response {
            persistent_stream.skip(start_offset).await?;
        }

        Ok(persistent_stream)
    }

    /// Read bytes from the current position
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_valid {
            return Err(anyhow::anyhow!("Stream is no longer valid"));
        }

        let mut bytes_read = 0;

        // First, use any pending buffered data
        if let Some(ref mut pending) = self.pending_buffer {
            let to_copy = pending.len().min(buf.len());
            buf[..to_copy].copy_from_slice(&pending[..to_copy]);
            bytes_read += to_copy;
            self.current_position += to_copy as u64; // Update position!

            if to_copy < pending.len() {
                // Still have data left in pending
                *pending = pending.slice(to_copy..);
            } else {
                // Used all pending data
                self.pending_buffer = None;
            }
        }

        // Read more data from the stream if needed
        while bytes_read < buf.len() {
            match self.stream.next().await {
                Some(Ok(chunk)) => {
                    let remaining = buf.len() - bytes_read;
                    let to_copy = chunk.len().min(remaining);
                    buf[bytes_read..bytes_read + to_copy].copy_from_slice(&chunk[..to_copy]);
                    bytes_read += to_copy;
                    self.current_position += to_copy as u64;

                    // If we got more data than needed, buffer the rest
                    if to_copy < chunk.len() {
                        self.pending_buffer = Some(chunk.slice(to_copy..));
                        trace!(
                            bytes_buffered = chunk.len() - to_copy,
                            "Buffered extra bytes from chunk"
                        );
                        break;
                    }
                }
                Some(Err(e)) => {
                    self.is_valid = false;
                    return Err(anyhow::anyhow!("Stream error: {}", e));
                }
                None => {
                    // End of stream
                    break;
                }
            }
        }

        self.last_access = Instant::now();
        Ok(bytes_read)
    }

    /// Skip forward in the stream by reading and discarding bytes
    async fn skip(&mut self, bytes_to_skip: u64) -> Result<u64> {
        if !self.is_valid {
            return Err(anyhow::anyhow!("Stream is no longer valid"));
        }

        let mut skipped = 0u64;

        // First, use any pending buffered data
        if let Some(ref mut pending) = self.pending_buffer {
            let to_skip = pending.len().min(bytes_to_skip as usize);
            skipped += to_skip as u64;
            self.current_position += to_skip as u64; // Update position!

            if to_skip < pending.len() {
                // Still have data left in pending
                *pending = pending.slice(to_skip..);
            } else {
                // Used all pending data
                self.pending_buffer = None;
            }
        }

        // Skip more data from the stream if needed
        while skipped < bytes_to_skip {
            match self.stream.next().await {
                Some(Ok(chunk)) => {
                    let remaining = bytes_to_skip - skipped;
                    let to_skip = chunk.len().min(remaining as usize);
                    skipped += to_skip as u64;
                    self.current_position += to_skip as u64;

                    // If we didn't use the whole chunk, buffer the rest
                    if to_skip < chunk.len() {
                        self.pending_buffer = Some(chunk.slice(to_skip..));
                        break;
                    }
                }
                Some(Err(e)) => {
                    self.is_valid = false;
                    return Err(anyhow::anyhow!("Stream error during skip: {}", e));
                }
                None => {
                    break;
                }
            }
        }

        self.last_access = Instant::now();
        Ok(skipped)
    }

    /// Check if this stream can satisfy a read at the given offset
    fn can_read_at(&self, offset: u64) -> bool {
        if !self.is_valid {
            return false;
        }

        // Can read if we're exactly at the offset (sequential)
        // or if we need to seek forward a small amount
        if offset >= self.current_position {
            let gap = offset - self.current_position;
            gap <= MAX_SEEK_FORWARD
        } else {
            // Can't seek backward
            false
        }
    }

    /// Check if the stream has been idle too long
    fn is_idle(&self) -> bool {
        self.last_access.elapsed() > STREAM_IDLE_TIMEOUT
    }
}

/// Manages persistent streams for efficient sequential reading
pub struct PersistentStreamManager {
    client: Client,
    base_url: String,
    /// Active streams keyed by (torrent_id, file_idx)
    /// Using Mutex instead of RwLock because the stream type is not Sync
    streams: Arc<Mutex<HashMap<StreamKey, PersistentStream>>>,
    /// Cleanup task handle stored in an Option<tokio::task::JoinHandle>
    cleanup_handle: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl PersistentStreamManager {
    /// Create a new stream manager
    pub fn new(client: Client, base_url: String) -> Self {
        let streams: Arc<Mutex<HashMap<StreamKey, PersistentStream>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let cleanup_handle = Arc::new(std::sync::Mutex::new(None));

        let manager = Self {
            client,
            base_url,
            streams: Arc::clone(&streams),
            cleanup_handle: Arc::clone(&cleanup_handle),
        };

        // Start cleanup task
        manager.start_cleanup_task(streams, cleanup_handle);

        manager
    }

    /// Start background task to clean up idle streams
    fn start_cleanup_task(
        &self,
        streams: Arc<Mutex<HashMap<StreamKey, PersistentStream>>>,
        handle_storage: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    ) {
        // Check if we're running in a Tokio runtime context
        // If not (e.g., in synchronous tests), skip starting the cleanup task
        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                let handle = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(CLEANUP_INTERVAL);

                    loop {
                        interval.tick().await;

                        let mut streams_guard = streams.lock().await;
                        let before_count = streams_guard.len();

                        streams_guard.retain(|key, stream| {
                            let should_keep = !stream.is_idle();
                            if !should_keep {
                                trace!(
                                    stream_op = "cleanup",
                                    torrent_id = key.torrent_id,
                                    file_idx = key.file_idx,
                                    "Removing idle stream"
                                );
                            }
                            should_keep
                        });

                        let after_count = streams_guard.len();
                        if before_count != after_count {
                            debug!(
                                stream_op = "cleanup",
                                removed = before_count - after_count,
                                remaining = after_count,
                                "Cleaned up idle streams"
                            );
                        }
                    }
                });

                if let Ok(mut h) = handle_storage.lock() {
                    *h = Some(handle);
                }
            }
            Err(_) => {
                // No runtime available (e.g., in synchronous tests)
                // Cleanup will be handled by stream reuse/creation logic
                trace!("No Tokio runtime available, skipping cleanup task");
            }
        }
    }

    /// Read data from a file, using a persistent stream if possible
    pub async fn read(
        &self,
        torrent_id: u64,
        file_idx: usize,
        offset: u64,
        size: usize,
    ) -> Result<Bytes> {
        let key = StreamKey {
            torrent_id,
            file_idx,
        };

        // Check if we have a usable stream
        let can_use_existing = {
            let streams = self.streams.lock().await;
            if let Some(stream) = streams.get(&key) {
                stream.can_read_at(offset)
            } else {
                false
            }
        };

        if can_use_existing {
            // Use existing stream
            trace!(
                stream_op = "reuse",
                torrent_id = torrent_id,
                file_idx = file_idx,
                offset = offset,
                size = size,
                "Reusing existing stream"
            );

            let mut streams = self.streams.lock().await;
            let stream = streams.get_mut(&key).unwrap();

            // If we need to seek forward a bit, do it
            if offset > stream.current_position {
                let gap = offset - stream.current_position;
                trace!(bytes_to_skip = gap, "Skipping forward in existing stream");
                stream.skip(gap).await?;
            }

            // Read the data
            let mut buffer = vec![0u8; size];
            let bytes_read = stream.read(&mut buffer).await?;
            buffer.truncate(bytes_read);

            trace!(
                stream_op = "read_complete",
                torrent_id = torrent_id,
                file_idx = file_idx,
                bytes_read = bytes_read,
                "Completed read from persistent stream"
            );

            Ok(Bytes::from(buffer))
        } else {
            // Create a new stream
            trace!(
                stream_op = "create_new",
                torrent_id = torrent_id,
                file_idx = file_idx,
                offset = offset,
                size = size,
                "Creating new stream for read"
            );

            let mut new_stream =
                PersistentStream::new(&self.client, &self.base_url, torrent_id, file_idx, offset)
                    .await?;

            // Read the data
            let mut buffer = vec![0u8; size];
            let bytes_read = new_stream.read(&mut buffer).await?;
            buffer.truncate(bytes_read);

            // Store the stream for future use
            let mut streams = self.streams.lock().await;
            streams.insert(key, new_stream);

            trace!(
                stream_op = "read_complete",
                torrent_id = torrent_id,
                file_idx = file_idx,
                bytes_read = bytes_read,
                "Completed read from new stream"
            );

            Ok(Bytes::from(buffer))
        }
    }

    /// Close a specific stream
    pub async fn close_stream(&self, torrent_id: u64, file_idx: usize) {
        let key = StreamKey {
            torrent_id,
            file_idx,
        };
        let mut streams = self.streams.lock().await;
        if streams.remove(&key).is_some() {
            trace!(
                stream_op = "close",
                torrent_id = torrent_id,
                file_idx = file_idx,
                "Stream closed"
            );
        }
    }

    /// Close all streams for a torrent
    pub async fn close_torrent_streams(&self, torrent_id: u64) {
        let mut streams = self.streams.lock().await;
        let before_count = streams.len();

        streams.retain(|key, _| key.torrent_id != torrent_id);

        let after_count = streams.len();
        if before_count != after_count {
            debug!(
                stream_op = "close_torrent",
                torrent_id = torrent_id,
                closed_count = before_count - after_count,
                "Closed all streams for torrent"
            );
        }
    }

    /// Get statistics about active streams
    pub async fn stats(&self) -> StreamManagerStats {
        let streams = self.streams.lock().await;
        StreamManagerStats {
            active_streams: streams.len(),
            total_bytes_streaming: streams.values().map(|s| s.current_position).sum(),
        }
    }
}

impl Drop for PersistentStreamManager {
    fn drop(&mut self) {
        // Try to abort cleanup task
        if let Ok(mut handle) = self.cleanup_handle.lock() {
            if let Some(h) = handle.take() {
                h.abort();
            }
        }
    }
}

/// Statistics about the stream manager
#[derive(Debug)]
pub struct StreamManagerStats {
    pub active_streams: usize,
    pub total_bytes_streaming: u64,
}
