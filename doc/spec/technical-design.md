# Technical Design Document

## Data Structures

### API Types (src/api/types.rs)

The actual implementation uses `TorrentInfo` from the rqbit API for metadata:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub id: u64,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
    pub file_count: Option<usize>,
    pub files: Vec<FileInfo>,
    pub piece_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub length: u64,
    pub components: Vec<String>,
}
```

**Note:** The spec previously referenced `src/types/torrent.rs` and `src/types/file.rs` which do not exist. All torrent metadata comes from the API's `TorrentInfo` struct.

### InodeEntry (src/fs/inode_entry.rs)

```rust
#[derive(Debug, Clone)]
pub enum InodeEntry {
    Directory {
        ino: u64,
        name: String,
        parent: u64,
        children: DashSet<u64>,     // Uses DashSet for thread-safe concurrent access
        canonical_path: String,     // Full path for reverse lookups
    },
    File {
        ino: u64,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: u64,            // NOTE: is u64, not usize
        size: u64,
        canonical_path: String,     // Full path for reverse lookups
    },
    Symlink {
        ino: u64,
        name: String,
        parent: u64,
        target: String,
        canonical_path: String,     // Full path for reverse lookups
    },
}

impl InodeEntry {
    pub fn ino(&self) -> u64;
    pub fn name(&self) -> &str;
    pub fn parent(&self) -> u64;
    pub fn canonical_path(&self) -> &str;
    pub fn torrent_id(&self) -> Option<u64>;
    pub fn file_size(&self) -> u64;
    pub fn is_directory(&self) -> bool;
    pub fn is_file(&self) -> bool;
    pub fn is_symlink(&self) -> bool;
    pub fn with_ino(&self, ino: u64) -> Self;  // Creates copy with new inode
}
```

**Key Implementation Details:**
- Root is a `Directory` with `ino: 1` (not a separate `Root` variant)
- Torrent directories use `Directory` variant (not separate `Torrent` variant)
- Each entry tracks its own inode number (`ino` field)
- Parent-child relationships are explicit with `parent` field
- Directories maintain a `children: DashSet<u64>` for thread safety
- **Symlinks are supported** (not in original spec)
- All entries store `canonical_path` for efficient path lookups
- Implements `Serialize` and `Deserialize` for persistence

### FileAttr (src/types/attr.rs)

Uses `fuser::FileAttr` directly with helper functions:

```rust
fn base_attr(ino: u64, size: u64) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(512),  // Uses div_ceil for ceiling division
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: FileType::RegularFile,
        perm: 0o444,                 // Read-only
        nlink: 1,
        uid: 1000,                   // Hardcoded to 1000 (not libc::getuid())
        gid: 1000,                   // Hardcoded to 1000 (not libc::getgid())
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn default_file_attr(ino: u64, size: u64) -> FileAttr {
    base_attr(ino, size)
}

pub fn default_dir_attr(ino: u64) -> FileAttr {
    let mut attr = base_attr(ino, 0);
    attr.kind = FileType::Directory;
    attr.perm = 0o755;           // NOTE: 0o755, not 0o555
    attr.nlink = 2;
    attr.blocks = 0;
    attr
}
```

**Note:** The implementation uses hardcoded uid/gid of 1000 rather than calling libc functions.

### FileHandle (src/types/handle.rs)

```rust
#[derive(Debug, Clone)]
pub struct FileHandle {
    pub fh: u64,
    pub inode: u64,
    pub torrent_id: u64,
    pub flags: i32,
}

pub struct FileHandleManager {
    next_handle: AtomicU64,
    handles: Arc<Mutex<HashMap<u64, FileHandle>>>,
    max_handles: usize,              // 0 = unlimited
}

impl FileHandleManager {
    pub fn new() -> Self;
    pub fn with_max_handles(max_handles: usize) -> Self;
    pub fn allocate(&self, inode: u64, torrent_id: u64, flags: i32) -> u64;  // Returns 0 if limit reached
    pub fn get(&self, fh: u64) -> Option<FileHandle>;
    pub fn remove(&self, fh: u64) -> Option<FileHandle>;
    pub fn get_inode(&self, fh: u64) -> Option<u64>;
    pub fn contains(&self, fh: u64) -> bool;
    pub fn get_handles_for_inode(&self, inode: u64) -> Vec<u64>;
    pub fn remove_by_torrent(&self, torrent_id: u64) -> usize;  // Returns count removed
}
```

## Inode Management

### InodeManager (src/fs/inode_manager.rs)

Manages mapping between inodes and filesystem entries with thread-safe concurrent access.

```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    path_to_inode: DashMap<String, u64>,
    torrent_to_inode: DashMap<u64, u64>,
    max_inodes: usize,               // 0 = unlimited
}

pub struct InodeEntryRef {
    pub inode: u64,
    pub entry: InodeEntry,
}

impl InodeManager {
    pub fn new() -> Self;
    pub fn with_max_inodes(max_inodes: usize) -> Self;
    
    // Allocation methods
    pub fn allocate(&self, entry: InodeEntry) -> u64;  // Generic allocation
    pub fn allocate_torrent_directory(&self, torrent_id: u64, name: String, parent: u64) -> u64;
    pub fn allocate_file(&self, name: String, parent: u64, torrent_id: u64, file_index: u64, size: u64) -> u64;
    pub fn allocate_symlink(&self, name: String, parent: u64, target: String) -> u64;
    
    // Lookup methods
    pub fn get(&self, inode: u64) -> Option<InodeEntry>;
    pub fn lookup_by_path(&self, path: &str) -> Option<u64>;
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<u64>;
    pub fn get_children(&self, parent_inode: u64) -> Vec<(u64, InodeEntry)>;
    pub fn iter_entries(&self) -> impl Iterator<Item = InodeEntryRef> + '_;
    pub fn torrent_to_inode(&self) -> &DashMap<u64, u64>;
    pub fn get_path_for_inode(&self, inode: u64) -> Option<String>;
    pub fn contains(&self, inode: u64) -> bool;
    
    // Management methods
    pub fn can_allocate(&self) -> bool;
    pub fn max_inodes(&self) -> usize;
    pub fn len(&self) -> usize;
    pub fn inode_count(&self) -> usize;  // Excludes root
    pub fn is_empty(&self) -> bool;
    pub fn next_inode(&self) -> u64;
    pub fn add_child(&self, parent: u64, child: u64);
    pub fn remove_child(&self, parent: u64, child: u64);
    pub fn remove_inode(&self, inode: u64) -> bool;
    pub fn clear_torrents(&self);
    pub fn get_all_torrent_ids(&self) -> Vec<u64>;
}
```

**Key Implementation Details:**
- Uses `DashMap` and `DashSet` for thread-safe concurrent access
- Supports optional `max_inodes` limit (0 = unlimited)
- Allocation returns 0 if limit is reached
- Maintains `path_to_inode` for efficient reverse path lookups
- All entries store `canonical_path` for building full paths
- Provides atomic `remove_inode` that cleans up children and indices

## FUSE Callbacks Implementation

### Overview (src/fs/filesystem.rs)

The main `TorrentFS` struct implements the `fuser::Filesystem` trait:

```rust
pub struct TorrentFS {
    config: Config,
    api_client: Arc<RqbitClient>,
    inode_manager: Arc<InodeManager>,
    initialized: bool,
    file_handles: Arc<FileHandleManager>,
    known_torrents: Arc<DashSet<u64>>,
    discovery_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    metrics: Arc<Metrics>,
    last_discovery: Arc<AtomicU64>,       // Milliseconds since Unix epoch
    async_worker: Arc<AsyncFuseWorker>,
    read_semaphore: Arc<Semaphore>,       // Limits concurrent reads
}

impl TorrentFS {
    pub fn new(config: Config, metrics: Arc<Metrics>, async_worker: Arc<AsyncFuseWorker>) -> Result<Self>;
    pub fn mount(self) -> Result<()>;
    pub fn shutdown(&self);
    pub fn refresh_torrents(&self, force: bool) -> impl Future<Output = bool>;
    pub fn build_file_attr(&self, entry: &InodeEntry) -> FileAttr;
    pub fn concurrency_stats(&self) -> ConcurrencyStats;
    
    // Internal methods
    fn start_torrent_discovery(&self);
    fn stop_torrent_discovery(&self);
    async fn discover_torrents(api_client: &Arc<RqbitClient>, inode_manager: &Arc<InodeManager>) -> Result<Vec<u64>>;
    fn create_torrent_structure_static(inode_manager: &Arc<InodeManager>, torrent_info: &TorrentInfo) -> Result<()>;
}
```

**Key Implementation Details:**
- Uses `AsyncFuseWorker` to bridge sync FUSE callbacks to async operations
- Background torrent discovery with 30-second polling interval
- Semaphore-based limit on concurrent reads (configured via `max_concurrent_reads`)
- Cooldown protection (5 seconds) for on-demand discovery during readdir

### init()

```rust
fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> Result<(), c_int> {
    // 1. Validates mount point exists and is accessible
    // 2. Checks root inode (1) exists and is a directory
    // 3. Starts background torrent discovery task
    // Does NOT load torrents immediately (done lazily)
}
```

### lookup()

```rust
fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    // Handle special entries "." and ".."
    // Build full path from parent and name
    // Lookup by path via inode_manager.lookup_by_path()
    // Return entry attributes via build_file_attr()
}
```

### getattr()

```rust
fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
    // Get inode entry from manager
    // Build attributes via build_file_attr()
    // Return with 1-second TTL
}
```

### readdir()

```rust
fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
    // If ino == 1 (root), trigger torrent discovery with cooldown
    // Get directory entry and iterate children
    // Add "." and ".." entries first
    // Add all children with their file types
}
```

### open()

```rust
fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
    // Verify inode exists and is a file (not directory or symlink)
    // Check O_RDONLY access mode (filesystem is read-only)
    // Allocate file handle via file_handles.allocate()
    // Return file handle (fh) or EMFILE if handle limit reached
}
```

### read()

```rust
fn read(&mut self, _req: &Request, _ino: u64, fh: u64, offset: i64, size: u32, 
        _flags: i32, _lock_owner: Option<u64>, reply: ReplyData) {
    // Validate offset is non-negative
    // Look up inode from file handle (returns EBADF if invalid handle)
    // Get file entry (returns EISDIR if not a file)
    // Clamp read size to FUSE_MAX_READ (64KB)
    // Acquire read semaphore permit for concurrency control
    // Call async_worker.read_file() to perform async read
    // Record metrics and handle errors
}
```

**Key Implementation Details:**
- Uses file handle (fh) not inode for read operations
- Acquires semaphore permit to limit concurrent reads
- All async operations go through AsyncFuseWorker
- Clamps read size to 64KB (FUSE_MAX_READ constant)

### release()

```rust
fn release(&mut self, _req: &Request, _ino: u64, fh: u64, _flags: i32, 
           _lock_owner: Option<u64>, _flush: bool, reply: ReplyEmpty) {
    // Remove file handle from manager
    // Reply OK even if handle not found
}
```

### readlink()

```rust
fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
    // Get inode entry
    // If symlink, reply with target bytes
    // Otherwise return EINVAL
}
```

## HTTP Read Implementation

### Persistent Streaming (src/api/streaming.rs)

The actual implementation uses `PersistentStreamManager` for efficient sequential reads:

```rust
pub struct PersistentStreamManager {
    client: Client,
    base_url: String,
    streams: Arc<Mutex<HashMap<StreamKey, PersistentStream>>>,
    cleanup_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    auth_credentials: Option<(String, String)>,
    max_streams: usize,              // Default: 50
}

struct PersistentStream {
    stream: ByteStream,
    current_position: u64,
    last_access: Instant,
    is_valid: bool,
    pending_buffer: Option<Bytes>,
}

pub struct StreamManagerStats {
    pub active_streams: usize,
    pub max_streams: usize,
    pub total_bytes_streaming: u64,
}

impl PersistentStreamManager {
    pub fn new(client: Client, base_url: String, auth_credentials: Option<(String, String)>) -> Self;
    pub fn with_max_streams(client: Client, base_url: String, auth_credentials: Option<(String, String)>, max_streams: usize) -> Self;
    
    pub async fn read(&self, torrent_id: u64, file_idx: usize, offset: u64, size: usize) -> Result<Bytes>;
    pub async fn close_stream(&self, torrent_id: u64, file_idx: usize);
    pub async fn close_torrent_streams(&self, torrent_id: u64);
    pub async fn stats(&self) -> StreamManagerStats;
}
```

**Key Features:**
- Connection reuse for sequential reads (keyed by torrent_id + file_idx)
- Automatic cleanup of idle streams (30-second timeout)
- Handles rqbit bug: server returns 200 OK instead of 206 Partial Content
- Supports forward seeks up to 10MB without creating new connection
- Configurable maximum concurrent streams (default 50)
- HTTP Basic Auth support via auth_credentials
- Background cleanup task removes idle streams every 10 seconds

### AsyncFuseWorker (src/fs/async_bridge.rs)

Bridges synchronous FUSE callbacks to async operations:

```rust
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

pub enum FuseResponse {
    ReadSuccess { data: Vec<u8> },
    ReadError { error_code: i32, message: String },
    PiecesAvailable,
    PiecesNotAvailable { reason: String },
    ForgetSuccess,
    ForgetError { error_code: i32, message: String },
}

pub struct AsyncFuseWorker {
    request_tx: mpsc::Sender<FuseRequest>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AsyncFuseWorker {
    pub fn new(api_client: Arc<RqbitClient>, metrics: Arc<Metrics>, channel_capacity: usize) -> Self;
    
    // Synchronous methods callable from FUSE callbacks
    pub fn read_file(&self, torrent_id: u64, file_index: u64, offset: u64, size: usize, timeout: Duration) -> RqbitFuseResult<Vec<u8>>;
    pub fn check_pieces_available(&self, torrent_id: u64, offset: u64, size: u64, timeout: Duration) -> RqbitFuseResult<bool>;
    pub fn forget_torrent(&self, torrent_id: u64, timeout: Duration) -> RqbitFuseResult<()>;
    pub fn shutdown(&mut self);
}
```

**Channel Architecture:**
- Request channel: `tokio::sync::mpsc` (async sender, async receiver in worker)
- Response channel: `std::sync::mpsc` (sync sender in worker, blocking recv in FUSE callback)
- This hybrid approach allows FUSE callbacks to block waiting for async operations

## Error Types

### RqbitFuseError (src/error.rs)

Unified error type with 11 variants:

```rust
#[derive(Error, Debug, Clone)]
pub enum RqbitFuseError {
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Operation timed out: {0}")]
    TimedOut(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    #[error("I/O error: {0}")]
    IoError(String),
    
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    
    #[error("Validation error: {0}")]
    ValidationError(Vec<ValidationIssue>),
    
    #[error("Resource temporarily unavailable: {0}")]
    NotReady(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Is a directory")]
    IsDirectory,
    
    #[error("Not a directory")]
    NotDirectory,
}

pub struct ValidationIssue {
    pub field: String,
    pub message: String,
}

impl RqbitFuseError {
    pub fn to_errno(&self) -> i32;
    pub fn is_transient(&self) -> bool;
    pub fn is_server_unavailable(&self) -> bool;
}

pub trait ToFuseError {
    fn to_fuse_error(&self) -> i32;
}
```

**Error Mapping to FUSE Codes:**

| Error Variant | FUSE Code | HTTP Status |
|---------------|-----------|-------------|
| `NotFound` | ENOENT | 404 |
| `PermissionDenied` | EACCES | 401, 403 |
| `TimedOut` | ETIMEDOUT | 408 |
| `NetworkError` | ENETUNREACH | Connection errors |
| `ApiError{400,416}` | EINVAL | 400, 416 |
| `ApiError{401,403}` | EACCES | 401, 403 |
| `ApiError{404}` | ENOENT | 404 |
| `ApiError{408,423,429,503,504}` | EAGAIN | 408, 423, 429, 503, 504 |
| `ApiError{409}` | EEXIST | 409 |
| `ApiError{413}` | EFBIG | 413 |
| `ApiError{500,502}` | EIO | 500, 502 |
| `IoError` | EIO | - |
| `InvalidArgument` | EINVAL | - |
| `ValidationError` | EINVAL | - |
| `NotReady` | EAGAIN | - |
| `ParseError` | EINVAL | - |
| `IsDirectory` | EISDIR | - |
| `NotDirectory` | ENOTDIR | - |

**Transient Errors (Retryable):**
- `TimedOut`
- `NetworkError`
- `NotReady`
- `ApiError` with status 408, 429, 502, 503, 504

## API Client

### RqbitClient (src/api/client.rs)

```rust
pub struct RqbitClient {
    client: Client,
    base_url: String,
    max_retries: u32,                // Default: 3
    retry_delay: Duration,           // Default: 500ms
    stream_manager: PersistentStreamManager,
    auth_credentials: Option<(String, String)>,
    list_torrents_cache: Arc<RwLock<Option<(Instant, ListTorrentsResult)>>>,
    list_torrents_cache_ttl: Duration,  // 30 seconds
    metrics: Option<Arc<Metrics>>,
}

pub struct ListTorrentsResult {
    pub torrents: Vec<TorrentInfo>,
    pub errors: Vec<(u64, String, RqbitFuseError)>,
}

impl RqbitClient {
    pub fn new(base_url: String) -> Result<Self>;
    pub fn with_auth(base_url: String, username: String, password: String) -> Result<Self>;
    pub fn with_config(base_url: String, max_retries: u32, retry_delay: Duration, 
                       auth_credentials: Option<(String, String)>, metrics: Option<Arc<Metrics>>) -> Result<Self>;
    
    // Torrent management
    pub async fn list_torrents(&self) -> Result<ListTorrentsResult>;
    pub async fn get_torrent(&self, id: u64) -> Result<TorrentInfo>;
    pub async fn add_torrent_magnet(&self, magnet_link: &str) -> Result<AddTorrentResponse>;
    pub async fn add_torrent_url(&self, torrent_url: &str) -> Result<AddTorrentResponse>;
    pub async fn get_torrent_stats(&self, id: u64) -> Result<TorrentStats>;
    pub async fn get_piece_bitfield(&self, id: u64) -> Result<PieceBitfield>;
    
    // File operations
    pub async fn read_file(&self, torrent_id: u64, file_idx: usize, range: Option<(u64, u64)>) -> Result<Bytes>;
    pub async fn read_file_streaming(&self, torrent_id: u64, file_idx: usize, offset: u64, size: usize) -> Result<Bytes>;
    pub async fn check_range_available(&self, torrent_id: u64, offset: u64, size: u64, piece_length: u64) -> Result<bool>;
    
    // Torrent control
    pub async fn pause_torrent(&self, id: u64) -> Result<()>;
    pub async fn start_torrent(&self, id: u64) -> Result<()>;
    pub async fn forget_torrent(&self, id: u64) -> Result<()>;
    pub async fn delete_torrent(&self, id: u64) -> Result<()>;
    
    // Health checks
    pub async fn health_check(&self) -> Result<bool>;
    pub async fn wait_for_server(&self, max_wait: Duration) -> Result<()>;
}
```

**Key Implementation Details:**
- Retry logic with exponential backoff (not circuit breaker)
- `list_torrents` has built-in caching (30 second TTL)
- Handles partial failures when fetching torrent details
- Supports HTTP Basic Authentication
- Uses `PersistentStreamManager` for streaming reads

## Configuration Structure

### Config (src/config/mod.rs)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    pub cache: CacheConfig,
    pub mount: MountConfig,
    pub performance: PerformanceConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub url: String,                 // Default: "http://127.0.0.1:3030"
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub metadata_ttl: u64,           // Default: 60 seconds
    pub max_entries: usize,          // Default: 1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub mount_point: PathBuf,        // Default: "/mnt/torrents"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub read_timeout: u64,           // Default: 30 seconds
    pub max_concurrent_reads: usize, // Default: 10
    pub readahead_size: u64,         // Default: 33554432 (32MB)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,               // Default: "info"
}

#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub api_url: Option<String>,
    pub mount_point: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Config {
    pub fn new() -> Self;
    pub fn from_file(path: &PathBuf) -> Result<Self, RqbitFuseError>;
    pub fn from_default_locations() -> Result<Self, RqbitFuseError>;
    pub fn merge_from_env(mut self) -> Result<Self, RqbitFuseError>;
    pub fn merge_from_cli(mut self, cli: &CliArgs) -> Self;
    pub fn load() -> Result<Self, RqbitFuseError>;
    pub fn load_with_cli(cli: &CliArgs) -> Result<Self, RqbitFuseError>;
    pub fn validate(&self) -> Result<(), RqbitFuseError>;
}
```

**Configuration Precedence (highest to lowest):**
1. CLI arguments
2. Environment variables (TORRENT_FUSE_*)
3. Config file (JSON or TOML)
4. Default values

**Environment Variables:**
- `TORRENT_FUSE_API_URL`
- `TORRENT_FUSE_MOUNT_POINT`
- `TORRENT_FUSE_METADATA_TTL`
- `TORRENT_FUSE_MAX_ENTRIES`
- `TORRENT_FUSE_READ_TIMEOUT`
- `TORRENT_FUSE_LOG_LEVEL`
- `TORRENT_FUSE_AUTH_USERPASS` (format: "username:password")
- `TORRENT_FUSE_AUTH_USERNAME`
- `TORRENT_FUSE_AUTH_PASSWORD`

## Metrics Collection

### Metrics (src/metrics.rs)

```rust
#[derive(Debug, Default)]
pub struct Metrics {
    pub bytes_read: AtomicU64,
    pub error_count: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self;
    pub fn record_read(&self, bytes: u64);
    pub fn record_error(&self);
    pub fn record_cache_hit(&self);
    pub fn record_cache_miss(&self);
    pub fn log_summary(&self);  // Logs on shutdown with hit rate percentage
}
```

**Metrics Summary Output:**
- bytes_read: Total bytes read from torrents
- errors: Total error count
- cache_hits: Total cache hits
- cache_misses: Total cache misses
- cache_hit_rate_pct: Calculated hit rate percentage

## Module Structure

```
src/
├── main.rs                    # Application entry point
├── lib.rs                     # Library exports
├── mount.rs                   # Mount/CLI handling
├── error.rs                   # RqbitFuseError and validation
├── metrics.rs                 # Metrics collection
├── config/
│   └── mod.rs                 # Configuration management
├── api/
│   ├── mod.rs                 # API module exports
│   ├── client.rs              # RqbitClient implementation
│   ├── streaming.rs           # PersistentStreamManager
│   └── types.rs               # API data types (TorrentInfo, etc.)
├── fs/
│   ├── mod.rs                 # FS module exports
│   ├── filesystem.rs          # TorrentFS FUSE implementation
│   ├── inode.rs               # Re-exports (backward compatibility)
│   ├── inode_entry.rs         # InodeEntry enum
│   ├── inode_manager.rs       # InodeManager implementation
│   └── async_bridge.rs        # AsyncFuseWorker
└── types/
    ├── mod.rs                 # Types module exports
    ├── attr.rs                # FileAttr helpers
    └── handle.rs              # FileHandle and FileHandleManager
```

## Constants

### FUSE Limits (src/fs/filesystem.rs)

```rust
impl TorrentFS {
    /// Maximum read size for FUSE responses (64KB)
    const FUSE_MAX_READ: u32 = 64 * 1024;
}
```

### Streaming Configuration (src/api/streaming.rs)

```rust
const MAX_SEEK_FORWARD: u64 = 10 * 1024 * 1024;        // 10MB
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(10);
const SKIP_YIELD_INTERVAL: u64 = 1024 * 1024;          // 1MB
```

### Inode Manager (src/fs/inode_manager.rs)

```rust
// Default maximum inodes when not specified
const DEFAULT_MAX_INODES: usize = 100_000;
```

### Discovery Cooldown (src/fs/filesystem.rs)

```rust
const COOLDOWN_MS: u64 = 5000;  // 5 seconds between on-demand discoveries
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);  // Background polling
```

Last updated: 2026-02-24
