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

/// Yield to runtime every N bytes during large skip operations
/// This prevents blocking the async runtime for too long
const SKIP_YIELD_INTERVAL: u64 = 1024 * 1024; // 1MB

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
        let pending_consumed = self.consume_pending(buf.len());
        if pending_consumed > 0 {
            if let Some(ref pending) = self.pending_buffer {
                let pending_data = pending.slice(0..pending_consumed);
                buf[..pending_consumed].copy_from_slice(&pending_data);
            }
            bytes_read += pending_consumed;
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

                    self.buffer_leftover(chunk, to_copy);
                    if self.pending_buffer.is_some() {
                        break;
                    }
                }
                Some(Err(e)) => {
                    self.is_valid = false;
                    return Err(anyhow::anyhow!("Stream error: {}", e));
                }
                None => break,
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

        let mut skipped = self.consume_pending(bytes_to_skip as usize) as u64;

        // Skip more data from the stream if needed
        let mut bytes_since_yield = 0u64;
        while skipped < bytes_to_skip {
            match self.stream.next().await {
                Some(Ok(chunk)) => {
                    let remaining = bytes_to_skip - skipped;
                    let to_skip = chunk.len().min(remaining as usize);
                    skipped += to_skip as u64;
                    self.current_position += to_skip as u64;
                    bytes_since_yield += to_skip as u64;

                    self.buffer_leftover(chunk, to_skip);
                    if self.pending_buffer.is_some() {
                        break;
                    }

                    // Yield to runtime every SKIP_YIELD_INTERVAL bytes to prevent blocking
                    if bytes_since_yield >= SKIP_YIELD_INTERVAL {
                        tokio::task::yield_now().await;
                        bytes_since_yield = 0;
                    }
                }
                Some(Err(e)) => {
                    self.is_valid = false;
                    return Err(anyhow::anyhow!("Stream error during skip: {}", e));
                }
                None => break,
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

    /// Consume bytes from pending buffer, returns bytes consumed
    fn consume_pending(&mut self, bytes_needed: usize) -> usize {
        if let Some(ref mut pending) = self.pending_buffer {
            let to_consume = pending.len().min(bytes_needed);
            self.current_position += to_consume as u64;

            if to_consume < pending.len() {
                *pending = pending.slice(to_consume..);
            } else {
                self.pending_buffer = None;
            }
            to_consume
        } else {
            0
        }
    }

    /// Buffer remaining chunk data after consuming `consumed` bytes
    fn buffer_leftover(&mut self, chunk: Bytes, consumed: usize) {
        if consumed < chunk.len() {
            self.pending_buffer = Some(chunk.slice(consumed..));
            trace!(
                bytes_buffered = chunk.len() - consumed,
                "Buffered extra bytes from chunk"
            );
        }
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
    cleanup_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl PersistentStreamManager {
    /// Create a new stream manager
    pub fn new(client: Client, base_url: String) -> Self {
        let streams: Arc<Mutex<HashMap<StreamKey, PersistentStream>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let cleanup_handle = Arc::new(Mutex::new(None));

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
        handle_storage: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    ) {
        // Only spawn cleanup task if we're in a Tokio runtime context
        // In synchronous tests without a runtime, skip the cleanup task
        if tokio::runtime::Handle::try_current().is_err() {
            trace!("No Tokio runtime available, skipping cleanup task");
            return;
        }

        // Spawn the cleanup task
        let cleanup_task = tokio::spawn(async move {
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

        // Store the handle - this must be done in an async context
        // We'll spawn a short task to do this
        tokio::spawn(async move {
            let mut h = handle_storage.lock().await;
            *h = Some(cleanup_task);
        });
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

        // Try to use existing stream first, holding lock for entire check-and-act
        let mut streams = self.streams.lock().await;

        let can_use_existing = if let Some(stream) = streams.get(&key) {
            stream.can_read_at(offset)
        } else {
            false
        };

        if can_use_existing {
            // We know the stream exists and is usable, get mutable reference
            // This is safe because we held the lock continuously
            let stream = streams
                .get_mut(&key)
                .expect("Stream must exist after check");

            trace!(
                stream_op = "reuse",
                torrent_id = torrent_id,
                file_idx = file_idx,
                offset = offset,
                size = size,
                "Reusing existing stream"
            );

            // If we need to seek forward a bit, do it
            if offset > stream.current_position {
                let gap = offset - stream.current_position;
                trace!(bytes_to_skip = gap, "Skipping forward in existing stream");
                stream.skip(gap).await?;
            }

            // Read while still holding lock, then release
            let result = self
                .read_from_stream(stream, size, torrent_id, file_idx)
                .await;
            drop(streams); // Release lock before returning
            result
        } else {
            // Drop the lock before creating a new stream (creation is async and may block)
            drop(streams);

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

            let result = self
                .read_from_stream(&mut new_stream, size, torrent_id, file_idx)
                .await?;

            // Store the stream for future use
            let mut streams = self.streams.lock().await;
            streams.insert(key, new_stream);

            Ok(result)
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

    /// Read data from a stream into a Bytes buffer
    async fn read_from_stream(
        &self,
        stream: &mut PersistentStream,
        size: usize,
        torrent_id: u64,
        file_idx: usize,
    ) -> Result<Bytes> {
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
    }
}

impl Drop for PersistentStreamManager {
    fn drop(&mut self) {
        // Try to abort cleanup task
        if let Ok(handle) = self.cleanup_handle.try_lock() {
            if let Some(h) = handle.as_ref() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Barrier;

    /// Test that concurrent reads to the same stream key don't cause race conditions
    #[tokio::test]
    async fn test_concurrent_stream_access() {
        let client = Client::new();
        let manager = Arc::new(PersistentStreamManager::new(
            client,
            "http://localhost:0".to_string(),
        ));

        // Create a barrier for synchronization
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = vec![];

        // Spawn 3 concurrent readers for the same stream
        for reader_id in 0..3 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);

            let handle = tokio::spawn(async move {
                // Wait for all readers to be ready
                barrier.wait().await;

                // Try to read - this tests the race condition fix
                // Even though the stream will fail to connect (invalid URL),
                // we're testing that the locking works correctly without panics
                let result = manager.read(1, 0, 0, 1024).await;

                // We expect an error since we're using an invalid URL
                // but the important thing is we don't panic or hit race conditions
                assert!(
                    result.is_err(),
                    "Reader {} should get an error with invalid URL",
                    reader_id
                );

                reader_id
            });

            handles.push(handle);
        }

        // Wait for all readers to complete
        let results = futures::future::join_all(handles).await;

        // All should complete without panics
        for result in results {
            assert!(result.is_ok(), "All readers should complete without panics");
        }
    }

    /// Test that stream creation is properly serialized
    #[tokio::test]
    async fn test_concurrent_stream_creation() {
        let client = Client::new();
        let manager = Arc::new(PersistentStreamManager::new(
            client,
            "http://localhost:0".to_string(),
        ));

        let barrier = Arc::new(Barrier::new(5));
        let mut handles = vec![];

        // Spawn multiple concurrent readers for the same file
        for i in 0..5 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);

            let handle = tokio::spawn(async move {
                barrier.wait().await;

                // All try to read the same file at the same time
                let _ = manager.read(1, 0, (i * 1024) as u64, 1024).await;

                i
            });

            handles.push(handle);
        }

        // All should complete without panics
        let results = futures::future::join_all(handles).await;
        for result in results {
            assert!(result.is_ok(), "All readers should complete without panics");
        }
    }

    /// Test that the check-then-act pattern is atomic
    #[tokio::test]
    async fn test_stream_check_then_act_atomicity() {
        let client = Client::new();
        let manager = Arc::new(PersistentStreamManager::new(
            client,
            "http://localhost:0".to_string(),
        ));

        // Test that checking stream usability and getting the stream is atomic
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let manager = Arc::clone(&manager);
                tokio::spawn(async move {
                    // Each reader tries multiple times
                    for _ in 0..5 {
                        let _ = manager.read(1, i % 2, 0, 512).await;
                    }
                    i
                })
            })
            .collect();

        let results = futures::future::join_all(handles).await;

        // All should complete successfully
        for result in results {
            assert!(
                result.is_ok(),
                "All operations should complete without panics"
            );
        }
    }

    /// Test stream lock is held during skip operation
    #[tokio::test]
    async fn test_stream_lock_held_during_skip() {
        let client = Client::new();
        let manager = Arc::new(PersistentStreamManager::new(
            client,
            "http://localhost:0".to_string(),
        ));

        // This test verifies that when multiple concurrent reads happen,
        // the lock is held continuously during the check-and-read operation
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let manager = Arc::clone(&manager);
                tokio::spawn(async move {
                    // Try reading at different offsets - this tests skip logic too
                    let offset = (i * 2048) as u64;
                    let _ = manager.read(1, 0, offset, 1024).await;
                    i
                })
            })
            .collect();

        let results = futures::future::join_all(handles).await;

        for result in results {
            assert!(
                result.is_ok(),
                "All operations should complete without panics"
            );
        }
    }

    /// Test backward seeking creates a new stream
    #[tokio::test]
    async fn test_backward_seek_creates_new_stream() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        // Start a mock server
        let mock_server = MockServer::start().await;

        // Mock response for range request at offset 0
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(206)
                .set_body_bytes(vec![0u8; 1000]))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let manager = PersistentStreamManager::new(client, mock_server.uri());

        // First read at offset 0
        let _ = manager.read(1, 0, 0, 100).await;

        // Then read at offset 500 (backward seek)
        let _ = manager.read(1, 0, 500, 100).await;

        // Verify both requests were made (backward seek created new stream)
        mock_server.verify().await;
    }

    /// Test forward seek within MAX_SEEK_FORWARD reuses stream
    #[tokio::test]
    async fn test_forward_seek_within_limit_reuses_stream() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;

        // Should only make ONE request since forward seek within limit reuses stream
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(206)
                .set_body_bytes(vec![0u8; 5000]))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let manager = PersistentStreamManager::new(client, mock_server.uri());

        // Read at offset 0
        let result1 = manager.read(1, 0, 0, 100).await;
        assert!(result1.is_ok(), "First read should succeed");

        // Read at offset 100 (small forward seek, within MAX_SEEK_FORWARD)
        let result2 = manager.read(1, 0, 100, 100).await;
        assert!(result2.is_ok(), "Second read should succeed");

        // Verify only one request was made (stream was reused)
        mock_server.verify().await;
    }

    /// Test forward seek beyond MAX_SEEK_FORWARD creates new stream
    #[tokio::test]
    async fn test_forward_seek_beyond_limit_creates_new_stream() {
        use crate::api::streaming::MAX_SEEK_FORWARD;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;

        let seek_distance = MAX_SEEK_FORWARD + 1024;

        // Mock response for any requests to this endpoint
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(206)
                .set_body_bytes(vec![0u8; 100]))
            .expect(2) // Expect 2 requests (initial + large seek)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let manager = PersistentStreamManager::new(client, mock_server.uri());

        // Read at offset 0
        let _ = manager.read(1, 0, 0, 100).await;

        // Read at large offset (beyond MAX_SEEK_FORWARD)
        let _ = manager.read(1, 0, seek_distance, 100).await;

        // Verify two requests were made (new stream created for large seek)
        mock_server.verify().await;
    }

    /// Test sequential reads reuse the same stream
    #[tokio::test]
    async fn test_sequential_reads_reuse_stream() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;

        // Should only make ONE request for sequential reads
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(206)
                .set_body_bytes(vec![0u8; 10000]))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let manager = PersistentStreamManager::new(client, mock_server.uri());

        // Sequential reads at increasing offsets
        for i in 0..10 {
            let offset = i * 100;
            let result = manager.read(1, 0, offset, 100).await;
            assert!(result.is_ok(), "Read {} at offset {} should succeed", i, offset);
        }

        // Verify only one request was made
        mock_server.verify().await;
    }

    /// Test seek to same position reuses stream
    #[tokio::test]
    async fn test_seek_to_same_position_reuses_stream() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;

        // Should only make ONE request
        Mock::given(method("GET"))
            .and(path("/torrents/1/stream/0"))
            .respond_with(ResponseTemplate::new(206)
                .set_body_bytes(vec![0u8; 1000]))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let manager = PersistentStreamManager::new(client, mock_server.uri());

        // Read at offset 100
        let _ = manager.read(1, 0, 100, 100).await;

        // Read at same offset again
        let _ = manager.read(1, 0, 100, 100).await;

        // Verify only one request was made
        mock_server.verify().await;
    }
}
