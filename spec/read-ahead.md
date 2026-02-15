# Read-Ahead/Prefetching Specification

## Overview

This document describes the read-ahead (prefetching) strategy for rqbit-fuse to optimize sequential file reads. The goal is to detect sequential access patterns and prefetch data ahead of time, reducing latency and improving throughput for streaming operations.

## Current Prefetch Behavior (Issues)

### Current Implementation

The current prefetch implementation exists in `src/fs/filesystem.rs` but has several critical issues:

```rust
// Current implementation (lines 608-674 in filesystem.rs)
fn track_and_prefetch(
    &self,
    ino: u64,
    offset: u64,
    size: u32,
    file_size: u64,
    torrent_id: u64,
    file_index: usize,
) {
    let mut read_states = self.read_states.lock().unwrap();
    
    // Get or create read state for this file
    let state = read_states
        .entry(ino)
        .or_insert_with(|| ReadState::new(offset, size));
    
    // Check if this is a sequential read
    let is_sequential = state.is_sequential(offset);
    state.update(offset, size);
    
    // Trigger prefetch after 2 consecutive sequential reads and not already prefetching
    if is_sequential && state.sequential_count >= 2 && !state.is_prefetching {
        let next_offset = offset + size as u64;
        
        if next_offset < file_size {
            let prefetch_size = std::cmp::min(
                self.config.performance.readahead_size,
                file_size - next_offset,
            ) as usize;
            
            if prefetch_size > 0 {
                state.is_prefetching = true;
                drop(read_states); // Release lock before async operation
                
                let api_client = Arc::clone(&self.api_client);
                let read_states = Arc::clone(&self.read_states);
                let readahead_size = self.config.performance.readahead_size;
                
                // Spawn prefetch in background
                tokio::spawn(async move {
                    let prefetch_end =
                        std::cmp::min(next_offset + readahead_size - 1, file_size - 1);
                    
                    match api_client
                        .read_file(torrent_id, file_index, Some((next_offset, prefetch_end)))
                        .await
                    {
                        Ok(_data) => {
                            // Could store in cache here
                        }
                        Err(_e) => {}
                    }
                    
                    // Mark prefetch as complete
                    if let Ok(mut states) = read_states.lock() {
                        if let Some(s) = states.get_mut(&ino) {
                            s.is_prefetching = false;
                        }
                    }
                });
            }
        }
    }
}
```

### Current Issues

#### 1. Data Fetched But Immediately Dropped

**Problem**: The prefetched data is read from the API but never stored or used:

```rust
Ok(_data) => {
    // Could store in cache here  <-- Data is discarded!
}
```

The `_data` variable is prefixed with underscore, indicating it's intentionally unused. The prefetched data is:
1. Fetched from the rqbit API
2. Held in memory temporarily
3. Dropped when the task completes
4. Never cached for future reads

This results in wasted network bandwidth and API calls without any performance benefit.

#### 2. No Cache Integration

The current implementation has no integration with the cache system. Prefetched data should be stored in the cache so subsequent reads can be served from memory instead of making another HTTP request.

#### 3. Limited Sequential Detection

The current sequential detection only tracks:
- Whether the current read follows immediately after the previous read
- A count of consecutive sequential reads

It doesn't account for:
- Variable read sizes
- Slightly out-of-order sequential reads (e.g., due to FUSE read coalescing)
- Random access patterns that might look sequential but aren't

#### 4. No Configurability

While there's a `readahead_size` config option, there's no way to:
- Disable prefetching entirely
- Configure the sequential read threshold
- Set prefetch limits per file or globally
- Adjust based on file type or access patterns

#### 5. Race Conditions

The current implementation uses `std::sync::Mutex` in async context, which can block the runtime:

```rust
let mut read_states = self.read_states.lock().unwrap();  // Can block async runtime
```

#### 6. No Prefetch Cancellation

If a file is closed or seek occurs, ongoing prefetch operations are not cancelled. The background task continues running, wasting resources.

#### 7. Double Read Risk

If a prefetch is in progress and the application requests the same data, both operations will proceed independently, potentially causing:
- Duplicate API calls
- Wasted bandwidth
- Cache inconsistencies

## Read-Ahead Strategies

### Sequential Read Detection

#### Definition of Sequential Access

A read is considered sequential if:

1. **Strict Sequential**: `current_offset == previous_offset + previous_size`
   - Exact byte-after-byte continuity
   - Most reliable indicator

2. **Near-Sequential**: `abs(current_offset - (previous_offset + previous_size)) <= threshold`
   - Allows for small gaps or overlaps
   - Accounts for FUSE read coalescing behavior
   - Threshold: 4KB (typical page size)

3. **Reverse Sequential**: `current_offset + current_size == previous_offset`
   - Backward sequential access (video seeking backward)
   - Less common but should be supported

#### Sequential Confidence Score

Instead of a simple count, use a weighted confidence score:

```rust
struct SequentialState {
    /// Current confidence level (0.0 - 1.0)
    confidence: f32,
    /// Last read position
    last_offset: u64,
    /// Last read size
    last_size: u32,
    /// Time of last access
    last_access: Instant,
    /// Direction of sequential access (forward/backward)
    direction: AccessDirection,
}

enum AccessDirection {
    Forward,
    Backward,
    Unknown,
}

impl SequentialState {
    /// Update confidence based on new read
    fn update(&mut self, offset: u64, size: u32) {
        let expected_offset = match self.direction {
            AccessDirection::Forward => self.last_offset + self.last_size as u64,
            AccessDirection::Backward => self.last_offset.saturating_sub(size as u64),
            AccessDirection::Unknown => self.last_offset + self.last_size as u64,
        };
        
        if offset == expected_offset {
            // Sequential read - increase confidence
            self.confidence = (self.confidence + 0.2).min(1.0);
            self.last_offset = offset;
            self.last_size = size;
        } else if offset > expected_offset && offset - expected_offset <= 4096 {
            // Near-sequential (small gap) - slight confidence increase
            self.confidence = (self.confidence + 0.1).min(1.0);
            self.last_offset = offset;
            self.last_size = size;
        } else {
            // Non-sequential - reset confidence
            self.confidence = 0.0;
            self.direction = AccessDirection::Unknown;
            self.last_offset = offset;
            self.last_size = size;
        }
        
        self.last_access = Instant::now();
    }
}
```

### Prefetch Window Sizing

#### Dynamic Window Sizing

The prefetch window should adapt based on:

1. **Sequential Confidence**: Higher confidence = larger window
2. **Available Bandwidth**: Monitor actual throughput
3. **File Size**: Larger files can have larger windows
4. **Memory Pressure**: Reduce window when memory is constrained

```rust
struct PrefetchWindow {
    /// Base window size from configuration
    base_size: u64,
    /// Current multiplier based on confidence
    multiplier: f32,
    /// Minimum window size
    min_size: u64,
    /// Maximum window size
    max_size: u64,
}

impl PrefetchWindow {
    fn calculate(&self, confidence: f32) -> u64 {
        let size = (self.base_size as f32 * (1.0 + confidence)) as u64;
        size.clamp(self.min_size, self.max_size)
    }
}
```

#### Default Window Sizes

| Confidence Level | Window Size | Description |
|-----------------|-------------|-------------|
| 0.0 - 0.3 | 1 MB | Low confidence, minimal prefetch |
| 0.3 - 0.6 | 4 MB | Medium confidence, moderate prefetch |
| 0.6 - 0.8 | 16 MB | High confidence, aggressive prefetch |
| 0.8 - 1.0 | 32 MB | Very high confidence, maximum prefetch |

### Configurable Read-Ahead Size

#### Configuration Options

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ReadaheadConfig {
    /// Enable/disable prefetching entirely
    pub enabled: bool,
    
    /// Base prefetch size in bytes (default: 32MB)
    pub base_size: u64,
    
    /// Minimum prefetch size (default: 1MB)
    pub min_size: u64,
    
    /// Maximum prefetch size (default: 128MB)
    pub max_size: u64,
    
    /// Sequential read threshold before prefetching (0.0 - 1.0, default: 0.4)
    pub sequential_threshold: f32,
    
    /// Maximum number of concurrent prefetches (default: 5)
    pub max_concurrent_prefetches: usize,
    
    /// Prefetch only for files larger than this (default: 10MB)
    pub min_file_size: u64,
    
    /// Disable prefetch for file extensions (e.g., [".txt", ".nfo"])
    pub excluded_extensions: Vec<String>,
    
    /// Time to wait before starting prefetch after sequential detection (default: 0ms)
    pub prefetch_delay_ms: u64,
}

impl Default for ReadaheadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_size: 32 * 1024 * 1024,      // 32 MB
            min_size: 1 * 1024 * 1024,        // 1 MB
            max_size: 128 * 1024 * 1024,      // 128 MB
            sequential_threshold: 0.4,
            max_concurrent_prefetches: 5,
            min_file_size: 10 * 1024 * 1024,  // 10 MB
            excluded_extensions: vec![],
            prefetch_delay_ms: 0,
        }
    }
}
```

### Smart Prefetching Based on Access Patterns

#### Pattern Detection

Track access patterns to optimize prefetch behavior:

```rust
enum AccessPattern {
    /// Sequential forward reads (streaming)
    SequentialForward,
    /// Sequential backward reads (reverse playback)
    SequentialBackward,
    /// Strided reads (e.g., video keyframes)
    Strided { stride: u64 },
    /// Random access with hotspots
    RandomHotspots { hotspots: Vec<u64> },
    /// Truly random access
    Random,
}

struct PatternDetector {
    /// Recent read offsets (circular buffer)
    history: VecDeque<u64>,
    /// Detected pattern
    pattern: AccessPattern,
    /// Pattern confidence
    confidence: f32,
}

impl PatternDetector {
    fn detect(&mut self, offset: u64, size: u32) {
        self.history.push_back(offset);
        if self.history.len() > 16 {
            self.history.pop_front();
        }
        
        if self.history.len() < 4 {
            return; // Not enough data
        }
        
        // Calculate deltas between consecutive reads
        let deltas: Vec<i64> = self.history
            .windows(2)
            .map(|w| w[1] as i64 - w[0] as i64)
            .collect();
        
        // Check for strided pattern
        if let Some(stride) = self.detect_stride(&deltas) {
            self.pattern = AccessPattern::Strided { stride };
            self.confidence = 0.8;
            return;
        }
        
        // Check for sequential forward
        if deltas.iter().all(|&d| d > 0 && d <= size as i64 * 2) {
            self.pattern = AccessPattern::SequentialForward;
            self.confidence = 1.0;
            return;
        }
        
        // Check for sequential backward
        if deltas.iter().all(|&d| d < 0 && d >= -(size as i64 * 2)) {
            self.pattern = AccessPattern::SequentialBackward;
            self.confidence = 1.0;
            return;
        }
        
        // Check for hotspots
        if let Some(hotspots) = self.detect_hotspots() {
            self.pattern = AccessPattern::RandomHotspots { hotspots };
            self.confidence = 0.6;
            return;
        }
        
        self.pattern = AccessPattern::Random;
        self.confidence = 0.0;
    }
}
```

## Implementation Approach

### Track Read Positions Per File Handle

#### File Handle State

```rust
use tokio::sync::RwLock;
use dashmap::DashMap;

/// State tracked for each open file handle
struct FileHandleState {
    /// Inode number
    ino: u64,
    /// Torrent ID
    torrent_id: u64,
    /// File index within torrent
    file_index: usize,
    /// File size
    file_size: u64,
    /// Sequential access detection
    sequential: SequentialState,
    /// Pattern detection
    pattern: PatternDetector,
    /// Currently active prefetch
    active_prefetch: Option<PrefetchHandle>,
    /// Last access time
    last_access: Instant,
}

/// Handle to an active prefetch operation
struct PrefetchHandle {
    /// Offset being prefetched
    offset: u64,
    /// Size being prefetched
    size: u64,
    /// Cancellation token
    cancel: tokio_util::sync::CancellationToken,
}

/// Global read-ahead manager
pub struct ReadaheadManager {
    /// Configuration
    config: ReadaheadConfig,
    /// File handle states (use DashMap for concurrent access)
    states: DashMap<u64, RwLock<FileHandleState>>,
    /// Cache for prefetched data
    cache: Arc<ChunkCache>,
    /// Semaphore to limit concurrent prefetches
    prefetch_semaphore: Arc<tokio::sync::Semaphore>,
}

impl ReadaheadManager {
    pub fn new(config: ReadaheadConfig, cache: Arc<ChunkCache>) -> Self {
        let prefetch_semaphore = Arc::new(tokio::sync::Semaphore::new(
            config.max_concurrent_prefetches
        ));
        
        Self {
            config,
            states: DashMap::new(),
            cache,
            prefetch_semaphore,
        }
    }
    
    /// Register a new file handle for tracking
    pub async fn register_handle(
        &self,
        fh: u64,
        ino: u64,
        torrent_id: u64,
        file_index: usize,
        file_size: u64,
    ) {
        let state = FileHandleState {
            ino,
            torrent_id,
            file_index,
            file_size,
            sequential: SequentialState::new(),
            pattern: PatternDetector::new(),
            active_prefetch: None,
            last_access: Instant::now(),
        };
        
        self.states.insert(fh, RwLock::new(state));
    }
    
    /// Unregister a file handle and cancel any active prefetches
    pub async fn unregister_handle(&self, fh: u64) {
        if let Some((_, state)) = self.states.remove(&fh) {
            let state = state.read().await;
            if let Some(prefetch) = &state.active_prefetch {
                prefetch.cancel.cancel();
            }
        }
    }
}
```

### Detect Sequential Access

```rust
impl ReadaheadManager {
    /// Record a read operation and return whether to trigger prefetch
    pub async fn record_read(
        &self,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> Option<PrefetchRequest> {
        let state = self.states.get(&fh)?;
        let mut state = state.write().await;
        
        // Update sequential detection
        state.sequential.update(offset, size);
        state.pattern.detect(offset, size);
        state.last_access = Instant::now();
        
        // Check if we should prefetch
        if !self.config.enabled {
            return None;
        }
        
        // Skip small files
        if state.file_size < self.config.min_file_size {
            return None;
        }
        
        // Check sequential threshold
        if state.sequential.confidence < self.config.sequential_threshold {
            return None;
        }
        
        // Cancel any conflicting prefetch
        if let Some(prefetch) = &state.active_prefetch {
            let prefetch_end = prefetch.offset + prefetch.size;
            let next_offset = offset + size as u64;
            
            // If the prefetch doesn't cover what we'll need next, cancel it
            if prefetch.offset > next_offset || prefetch_end < next_offset {
                prefetch.cancel.cancel();
                state.active_prefetch = None;
            } else {
                // Prefetch is already covering the right area
                return None;
            }
        }
        
        // Calculate prefetch window
        let window_size = self.calculate_window(&state);
        let prefetch_offset = offset + size as u64;
        
        // Don't prefetch past EOF
        if prefetch_offset >= state.file_size {
            return None;
        }
        
        let prefetch_size = window_size.min(state.file_size - prefetch_offset);
        
        // Check if already cached
        if self.cache.is_range_cached(fh, prefetch_offset, prefetch_size).await {
            return None;
        }
        
        Some(PrefetchRequest {
            fh,
            torrent_id: state.torrent_id,
            file_index: state.file_index,
            offset: prefetch_offset,
            size: prefetch_size,
        })
    }
    
    fn calculate_window(&self, state: &FileHandleState) -> u64 {
        let multiplier = 1.0 + state.sequential.confidence;
        let size = (self.config.base_size as f32 * multiplier) as u64;
        size.clamp(self.config.min_size, self.config.max_size)
    }
}

struct PrefetchRequest {
    fh: u64,
    torrent_id: u64,
    file_index: usize,
    offset: u64,
    size: u64,
}
```

### Prefetch Next Chunks

```rust
impl ReadaheadManager {
    /// Execute a prefetch request
    pub async fn execute_prefetch(
        &self,
        request: PrefetchRequest,
        api_client: Arc<RqbitClient>,
    ) -> Result<(), anyhow::Error> {
        let cancel_token = tokio_util::sync::CancellationToken::new();
        
        // Update state with active prefetch
        if let Some(state) = self.states.get(&request.fh) {
            let mut state = state.write().await;
            state.active_prefetch = Some(PrefetchHandle {
                offset: request.offset,
                size: request.size,
                cancel: cancel_token.clone(),
            });
        }
        
        // Acquire semaphore permit
        let permit = self
            .prefetch_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to acquire prefetch permit: {}", e))?;
        
        let offset = request.offset;
        let size = request.size;
        let fh = request.fh;
        let cache = Arc::clone(&self.cache);
        
        // Spawn prefetch task
        tokio::spawn(async move {
            let _permit = permit; // Hold permit until task completes
            
            // Check for cancellation before starting
            if cancel_token.is_cancelled() {
                return;
            }
            
            // Make API request
            let result = api_client
                .read_file(request.torrent_id, request.file_index, Some((offset, offset + size - 1)))
                .await;
            
            match result {
                Ok(data) => {
                    // Store in cache
                    cache.store(fh, offset, data).await;
                }
                Err(e) => {
                    trace!("Prefetch failed for fh={}, offset={}: {}", fh, offset, e);
                }
            }
        });
        
        Ok(())
    }
}
```

### Cache Prefetched Data

```rust
use bytes::Bytes;
use moka::future::Cache;

/// Cache for prefetched file chunks
pub struct ChunkCache {
    /// Underlying cache: (fh, chunk_index) -> Bytes
    inner: Cache<(u64, u64), Bytes>,
    /// Chunk size (default: 1MB)
    chunk_size: u64,
}

impl ChunkCache {
    pub fn new(max_entries: u64, chunk_size: u64) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(max_entries)
                .time_to_idle(Duration::from_secs(30))  // Evict if not accessed
                .build(),
            chunk_size,
        }
    }
    
    /// Store data in the cache
    pub async fn store(&self, fh: u64, offset: u64, data: Vec<u8>) {
        let bytes = Bytes::from(data);
        let start_chunk = offset / self.chunk_size;
        let end_chunk = (offset + bytes.len() as u64 - 1) / self.chunk_size;
        
        // Split data into chunks and store
        for chunk_idx in start_chunk..=end_chunk {
            let chunk_offset = chunk_idx * self.chunk_size;
            let chunk_start = if chunk_offset > offset {
                chunk_offset - offset
            } else {
                0
            } as usize;
            let chunk_end = ((chunk_offset + self.chunk_size).min(offset + bytes.len() as u64) - offset) as usize;
            
            if chunk_start < chunk_end {
                let chunk_data = bytes.slice(chunk_start..chunk_end);
                self.inner.insert((fh, chunk_idx), chunk_data).await;
            }
        }
    }
    
    /// Check if a range is already cached
    pub async fn is_range_cached(&self, fh: u64, offset: u64, size: u64) -> bool {
        let start_chunk = offset / self.chunk_size;
        let end_chunk = (offset + size - 1) / self.chunk_size;
        
        // Check if all chunks in the range are cached
        for chunk_idx in start_chunk..=end_chunk {
            if self.inner.get(&(fh, chunk_idx)).await.is_none() {
                return false;
            }
        }
        
        true
    }
    
    /// Get data from cache, returning how much was found
    pub async fn get(&self, fh: u64, offset: u64, size: u64, buf: &mut [u8]) -> usize {
        let mut bytes_read = 0;
        let start_chunk = offset / self.chunk_size;
        let end_chunk = (offset + size - 1) / self.chunk_size;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        
        for chunk_idx in start_chunk..=end_chunk {
            if let Some(chunk) = self.inner.get(&(fh, chunk_idx)).await {
                let chunk_start_in_chunk = current_offset - chunk_idx * self.chunk_size;
                let available = (chunk.len() as u64 - chunk_start_in_chunk) as usize;
                let to_copy = available.min(buf.len() - buf_offset);
                
                buf[buf_offset..buf_offset + to_copy].copy_from_slice(
                    &chunk[chunk_start_in_chunk as usize..chunk_start_in_chunk as usize + to_copy]
                );
                
                bytes_read += to_copy;
                buf_offset += to_copy;
                current_offset += to_copy as u64;
                
                if buf_offset >= buf.len() {
                    break;
                }
            } else {
                // Missing chunk, stop here
                break;
            }
        }
        
        bytes_read
    }
}
```

### Avoid Double Reads

```rust
impl ReadaheadManager {
    /// Read data, using cache if available, otherwise fetch from API
    pub async fn read(
        &self,
        fh: u64,
        offset: u64,
        size: u32,
        api_client: Arc<RqbitClient>,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let mut buf = vec![0u8; size as usize];
        let mut total_read = 0;
        
        // Try to read from cache first
        let cached = self.cache.get(fh, offset, size as u64, &mut buf).await;
        total_read += cached;
        
        // If we got everything from cache, we're done
        if total_read == size as usize {
            return Ok(buf);
        }
        
        // Need to fetch remaining data from API
        let fetch_offset = offset + total_read as u64;
        let fetch_size = size as usize - total_read;
        
        // Check if there's an active prefetch for this range
        let prefetch_data = self.check_active_prefetch(fh, fetch_offset, fetch_size).await;
        
        let data = if let Some(prefetched) = prefetch_data {
            prefetched
        } else {
            // Fetch from API
            let state = self.states.get(&fh).ok_or_else(|| anyhow::anyhow!("Invalid file handle"))?;
            let state = state.read().await;
            
            api_client
                .read_file(state.torrent_id, state.file_index, Some((fetch_offset, fetch_offset + fetch_size as u64 - 1)))
                .await?
        };
        
        // Copy data to buffer
        buf[total_read..total_read + data.len()].copy_from_slice(&data);
        
        // Store in cache for future reads
        self.cache.store(fh, fetch_offset, data).await;
        
        Ok(buf)
    }
    
    async fn check_active_prefetch(
        &self,
        fh: u64,
        offset: u64,
        size: usize,
    ) -> Option<Vec<u8>> {
        // Check if we have an active prefetch that covers this range
        if let Some(state) = self.states.get(&fh) {
            let state = state.read().await;
            
            if let Some(prefetch) = &state.active_prefetch {
                let prefetch_end = prefetch.offset + prefetch.size;
                let request_end = offset + size as u64;
                
                // Check if prefetch covers our range
                if prefetch.offset <= offset && prefetch_end >= request_end {
                    // The prefetch is fetching this data, we should wait for it
                    // In practice, we'd use a channel or notification
                    // For now, return None and let the caller fetch directly
                }
            }
        }
        
        None
    }
}
```

## Configuration

### Full Configuration Integration

Update `PerformanceConfig` in `config/mod.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PerformanceConfig {
    /// Timeout for read operations in seconds
    #[serde(default = "default_read_timeout")]
    pub read_timeout: u64,
    
    /// Maximum concurrent HTTP reads
    #[serde(default = "default_max_concurrent_reads")]
    pub max_concurrent_reads: usize,
    
    /// Read-ahead configuration
    #[serde(default)]
    pub readahead: ReadaheadConfig,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            read_timeout: default_read_timeout(),
            max_concurrent_reads: default_max_concurrent_reads(),
            readahead: ReadaheadConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadaheadConfig {
    /// Enable/disable prefetching
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Base prefetch window size in bytes
    #[serde(default = "default_readahead_size")]
    pub base_size: u64,
    
    /// Minimum prefetch size
    #[serde(default = "default_min_readahead")]
    pub min_size: u64,
    
    /// Maximum prefetch size
    #[serde(default = "default_max_readahead")]
    pub max_size: u64,
    
    /// Sequential confidence threshold (0.0 - 1.0)
    #[serde(default = "default_sequential_threshold")]
    pub sequential_threshold: f32,
    
    /// Maximum concurrent prefetch operations
    #[serde(default = "default_max_concurrent_prefetches")]
    pub max_concurrent_prefetches: usize,
    
    /// Minimum file size to enable prefetching
    #[serde(default = "default_min_file_size")]
    pub min_file_size: u64,
    
    /// File extensions to exclude from prefetching
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
}

impl Default for ReadaheadConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            base_size: default_readahead_size(),
            min_size: default_min_readahead(),
            max_size: default_max_readahead(),
            sequential_threshold: default_sequential_threshold(),
            max_concurrent_prefetches: default_max_concurrent_prefetches(),
            min_file_size: default_min_file_size(),
            excluded_extensions: vec![],
        }
    }
}

// Default functions
fn default_true() -> bool { true }
fn default_readahead_size() -> u64 { 32 * 1024 * 1024 } // 32 MB
fn default_min_readahead() -> u64 { 1 * 1024 * 1024 }   // 1 MB
fn default_max_readahead() -> u64 { 128 * 1024 * 1024 } // 128 MB
fn default_sequential_threshold() -> f32 { 0.4 }
fn default_max_concurrent_prefetches() -> usize { 5 }
fn default_min_file_size() -> u64 { 10 * 1024 * 1024 }  // 10 MB
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TORRENT_FUSE_READAHEAD_ENABLED` | Enable/disable prefetching | `true` |
| `TORRENT_FUSE_READAHEAD_BASE_SIZE` | Base prefetch window | `33554432` (32MB) |
| `TORRENT_FUSE_READAHEAD_MIN_SIZE` | Minimum prefetch size | `1048576` (1MB) |
| `TORRENT_FUSE_READAHEAD_MAX_SIZE` | Maximum prefetch size | `134217728` (128MB) |
| `TORRENT_FUSE_SEQUENTIAL_THRESHOLD` | Confidence threshold | `0.4` |
| `TORRENT_FUSE_MAX_CONCURRENT_PREFETCHES` | Concurrent limit | `5` |
| `TORRENT_FUSE_MIN_FILE_SIZE` | Minimum file size | `10485760` (10MB) |

### Example Configuration File

```toml
[performance]
read_timeout = 30
max_concurrent_reads = 10

[performance.readahead]
enabled = true
base_size = 33554432       # 32 MB
min_size = 1048576         # 1 MB
max_size = 134217728       # 128 MB
sequential_threshold = 0.4
max_concurrent_prefetches = 5
min_file_size = 10485760   # 10 MB
excluded_extensions = [".txt", ".nfo", ".log"]
```

## Integration with Cache

### How Read-Ahead Data Fits in Cache

The read-ahead cache is separate from the metadata cache:

```
┌─────────────────────────────────────────────────────────────┐
│                      Cache System                            │
├─────────────────────────────────────────────────────────────┤
│  Metadata Cache (moka)        │  Chunk Cache (moka)          │
│  - Torrent metadata           │  - Prefetched file chunks    │
│  - File attributes            │  - Recently read data        │
│  - Directory listings         │  - Small reads (< 64KB)      │
│  - TTL: 30-60 seconds         │  - TTL: Idle-based eviction  │
│  - Size: ~1000 entries        │  - Size: Configurable        │
└─────────────────────────────────────────────────────────────┘
```

### Cache Key Structure

```rust
/// Key for chunk cache entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkKey {
    /// File handle
    fh: u64,
    /// Chunk index (offset / chunk_size)
    chunk_idx: u64,
}

impl ChunkKey {
    fn new(fh: u64, offset: u64, chunk_size: u64) -> Self {
        Self {
            fh,
            chunk_idx: offset / chunk_size,
        }
    }
}
```

### Eviction Policy for Prefetched Data

Use **time-to-idle** eviction for prefetched data:

```rust
impl ChunkCache {
    pub fn new(max_entries: u64, chunk_size: u64) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(max_entries)
                .time_to_idle(Duration::from_secs(60))  // Evict if not accessed for 60s
                .eviction_listener(|key, value, cause| {
                    trace!("Chunk {:?} evicted: {:?}", key, cause);
                })
                .build(),
            chunk_size,
        }
    }
}
```

**Eviction priorities:**

1. **Accessed chunks**: Move to front of LRU
2. **Prefetched but unaccessed**: Lower priority, evict first
3. **Partially accessed**: Keep accessed portions, evict rest
4. **File handle closed**: Evict all chunks for that handle

### Cache Warming Strategy

```rust
impl ReadaheadManager {
    /// Warm cache with predicted future reads
    async fn warm_cache(&self, fh: u64, current_offset: u64) {
        let state = match self.states.get(&fh) {
            Some(s) => s,
            None => return,
        };
        let state = state.read().await;
        
        match &state.pattern.pattern {
            AccessPattern::SequentialForward => {
                // Prefetch ahead
                self.trigger_prefetch(fh, current_offset).await;
            }
            AccessPattern::SequentialBackward => {
                // Prefetch behind (for reverse playback)
                let prefetch_offset = current_offset.saturating_sub(self.config.base_size);
                self.trigger_prefetch_at(fh, prefetch_offset).await;
            }
            AccessPattern::Strided { stride } => {
                // Prefetch next stride positions
                for i in 1..=3 {
                    let offset = current_offset + stride * i;
                    self.trigger_prefetch_at(fh, offset).await;
                }
            }
            AccessPattern::RandomHotspots { hotspots } => {
                // Prefetch nearby hotspots
                for hotspot in hotspots {
                    if (*hotspot as i64 - current_offset as i64).abs() < self.config.base_size as i64 {
                        self.trigger_prefetch_at(fh, *hotspot).await;
                    }
                }
            }
            _ => {} // No warming for random access
        }
    }
}
```

### Cache Coordination with Metadata Cache

When a file is closed or torrent is removed:

```rust
impl ReadaheadManager {
    /// Clean up cache when file is closed
    pub async fn on_file_close(&self, fh: u64) {
        // Cancel active prefetch
        self.unregister_handle(fh).await;
        
        // Evict all chunks for this file handle
        self.cache.evict_file_handle(fh).await;
    }
    
    /// Clean up cache when torrent is removed
    pub async fn on_torrent_removed(&self, torrent_id: u64) {
        // Find all file handles for this torrent
        let handles_to_remove: Vec<u64> = self
            .states
            .iter()
            .filter(|entry| {
                let state = entry.value().blocking_read();
                state.torrent_id == torrent_id
            })
            .map(|entry| *entry.key())
            .collect();
        
        // Clean up each handle
        for fh in handles_to_remove {
            self.on_file_close(fh).await;
        }
    }
}
```

## Performance Considerations

### Memory Usage

#### Per-File-Handle Overhead

```rust
struct FileHandleState {
    ino: u64,                    // 8 bytes
    torrent_id: u64,             // 8 bytes
    file_index: usize,           // 8 bytes
    file_size: u64,              // 8 bytes
    sequential: SequentialState, // ~40 bytes
    pattern: PatternDetector,    // ~256 bytes (history buffer)
    active_prefetch: Option<PrefetchHandle>, // ~24 bytes
    last_access: Instant,        // 16 bytes
}                                // ~368 bytes per file handle
```

With 100 open files: ~36 KB overhead

#### Cache Memory Usage

```rust
// Each 1MB chunk with overhead
struct CachedChunk {
    data: Bytes,           // 1 MB + ~32 bytes overhead
    key: ChunkKey,         // 16 bytes
    metadata: CacheMetadata, // ~64 bytes
}                          // ~1.05 MB per chunk

// 1000 chunks = ~1 GB max
// 100 chunks = ~100 MB
```

**Memory limits should be configurable:**

```rust
pub struct ChunkCacheConfig {
    /// Maximum memory in bytes
    pub max_memory_bytes: u64,
    /// Chunk size
    pub chunk_size: u64,
}

impl ChunkCacheConfig {
    fn max_entries(&self) -> u64 {
        self.max_memory_bytes / (self.chunk_size + 1024) // Account for overhead
    }
}
```

### Network Efficiency

#### Bandwidth Saving Strategies

1. **Deduplication**: Don't prefetch data that's already in-flight
2. **Range Merging**: Merge adjacent prefetch requests
3. **Cancellation**: Cancel prefetches when file is closed or seek occurs
4. **Backoff**: Reduce prefetch frequency on slow connections

```rust
struct BandwidthMonitor {
    /// Recent throughput measurements (bytes/sec)
    throughput_history: VecDeque<f64>,
    /// Slow connection threshold
    slow_threshold: f64,
}

impl BandwidthMonitor {
    fn is_slow_connection(&self) -> bool {
        if self.throughput_history.len() < 3 {
            return false;
        }
        
        let avg: f64 = self.throughput_history.iter().sum::<f64>() 
            / self.throughput_history.len() as f64;
        
        avg < self.slow_threshold // e.g., 100 KB/s
    }
    
    fn adjust_prefetch_size(&self, base_size: u64) -> u64 {
        if self.is_slow_connection() {
            base_size / 4  // Reduce on slow connections
        } else {
            base_size
        }
    }
}
```

#### Prefetch Efficiency Metrics

Track these metrics to optimize prefetch behavior:

```rust
struct PrefetchMetrics {
    /// Total prefetch operations
    total_prefetches: AtomicU64,
    /// Prefetches that were used (cache hit)
    used_prefetches: AtomicU64,
    /// Prefetches cancelled before completion
    cancelled_prefetches: AtomicU64,
    /// Prefetches that failed
    failed_prefetches: AtomicU64,
    /// Average prefetch latency
    avg_prefetch_latency_ms: AtomicU64,
}

impl PrefetchMetrics {
    fn efficiency_ratio(&self) -> f64 {
        let total = self.total_prefetches.load(Ordering::Relaxed);
        let used = self.used_prefetches.load(Ordering::Relaxed);
        
        if total == 0 {
            0.0
        } else {
            used as f64 / total as f64
        }
    }
}
```

### Sequential vs Random Access

#### Detection Heuristics

```rust
enum AccessType {
    Sequential,
    Random,
    Mixed,
}

fn classify_access(pattern: &PatternDetector) -> AccessType {
    match &pattern.pattern {
        AccessPattern::SequentialForward | AccessPattern::SequentialBackward => {
            AccessType::Sequential
        }
        AccessPattern::Random => AccessType::Random,
        _ => AccessType::Mixed,
    }
}

/// Adjust behavior based on access type
fn optimize_for_access_type(
    &mut self,
    access_type: AccessType,
) {
    match access_type {
        AccessType::Sequential => {
            // Aggressive prefetching
            self.config.base_size = 32 * 1024 * 1024;
            self.config.sequential_threshold = 0.3;
        }
        AccessType::Random => {
            // Disable prefetching
            self.config.enabled = false;
        }
        AccessType::Mixed => {
            // Conservative prefetching
            self.config.base_size = 4 * 1024 * 1024;
            self.config.sequential_threshold = 0.6;
        }
    }
}
```

#### Adaptive Prefetching

```rust
impl ReadaheadManager {
    /// Adapt prefetch behavior based on observed efficiency
    async fn adapt_prefetch_behavior(&self) {
        let efficiency = self.metrics.efficiency_ratio();
        
        if efficiency < 0.2 {
            // Very inefficient - reduce aggressiveness
            warn!("Prefetch efficiency low ({:.1}%), reducing aggressiveness", efficiency * 100.0);
            self.config.base_size = self.config.base_size / 2;
            self.config.sequential_threshold = (self.config.sequential_threshold + 0.1).min(0.8);
        } else if efficiency > 0.8 {
            // Very efficient - can be more aggressive
            info!("Prefetch efficiency high ({:.1}%), increasing aggressiveness", efficiency * 100.0);
            self.config.base_size = (self.config.base_size * 2).min(self.config.max_size);
        }
    }
}
```

## Implementation Checklist

### Phase 1: Core Infrastructure

- [ ] Create `ReadaheadManager` struct with configuration
- [ ] Implement `FileHandleState` tracking
- [ ] Create `SequentialState` with confidence scoring
- [ ] Implement pattern detection (`PatternDetector`)
- [ ] Add configuration options to `PerformanceConfig`

### Phase 2: Cache Integration

- [ ] Create `ChunkCache` with moka backend
- [ ] Implement chunk-based storage and retrieval
- [ ] Add cache eviction policies
- [ ] Integrate with read path (check cache first)

### Phase 3: Prefetch Logic

- [ ] Implement `track_and_prefetch` with cache storage
- [ ] Add prefetch execution with semaphore limiting
- [ ] Implement cancellation support
- [ ] Add double-read prevention

### Phase 4: Optimization

- [ ] Add bandwidth monitoring
- [ ] Implement adaptive prefetch sizing
- [ ] Add prefetch efficiency metrics
- [ ] Implement access pattern classification

### Phase 5: Testing

- [ ] Unit tests for sequential detection
- [ ] Unit tests for pattern detection
- [ ] Integration tests for prefetch behavior
- [ ] Performance benchmarks
- [ ] Memory usage tests

## References

- Current implementation: `src/fs/filesystem.rs` lines 608-674
- Cache implementation: `src/cache.rs`
- Streaming implementation: `src/api/streaming.rs`
- Configuration: `src/config/mod.rs`
- Moka cache documentation: https://docs.rs/moka/
- FUSE read behavior: https://libfuse.github.io/doxygen/

---

*Document version: 1.0*
*Last updated: 2026-02-14*
