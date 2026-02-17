use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Represents state associated with an open file handle.
/// Tracks read patterns for sequential access detection and prefetching.
#[derive(Debug, Clone)]
pub struct FileHandleState {
    /// Last read offset
    pub last_offset: u64,
    /// Last read size
    pub last_size: u32,
    /// Number of consecutive sequential reads
    pub sequential_count: u32,
    /// Last access time
    pub last_access: Instant,
    /// Whether this file is being prefetched
    pub is_prefetching: bool,
}

impl FileHandleState {
    /// Create new state for a file handle
    pub fn new(offset: u64, size: u32) -> Self {
        Self {
            last_offset: offset,
            last_size: size,
            sequential_count: 1,
            last_access: Instant::now(),
            is_prefetching: false,
        }
    }

    /// Check if the current read is sequential (immediately follows previous read)
    pub fn is_sequential(&self, offset: u64) -> bool {
        offset == self.last_offset + self.last_size as u64
    }

    /// Update state after a read
    pub fn update(&mut self, offset: u64, size: u32) {
        if self.is_sequential(offset) {
            self.sequential_count += 1;
        } else {
            self.sequential_count = 1;
        }
        self.last_offset = offset;
        self.last_size = size;
        self.last_access = Instant::now();
    }
}

/// Information stored for each open file handle.
#[derive(Debug, Clone)]
pub struct FileHandle {
    /// The file handle ID (unique per open)
    pub fh: u64,
    /// The inode number this handle refers to
    pub inode: u64,
    /// Open flags used when opening the file
    pub flags: i32,
    /// Optional state for tracking read patterns
    pub state: Option<FileHandleState>,
    /// When this handle was created (for TTL-based cleanup)
    pub created_at: Instant,
}

impl FileHandle {
    /// Create a new file handle
    pub fn new(fh: u64, inode: u64, flags: i32) -> Self {
        Self {
            fh,
            inode,
            flags,
            state: None,
            created_at: Instant::now(),
        }
    }

    /// Check if this handle has exceeded its TTL
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }

    /// Initialize read tracking state
    pub fn init_state(&mut self, offset: u64, size: u32) {
        self.state = Some(FileHandleState::new(offset, size));
    }

    /// Update read tracking state
    pub fn update_state(&mut self, offset: u64, size: u32) {
        if let Some(ref mut state) = self.state {
            state.update(offset, size);
        } else {
            self.init_state(offset, size);
        }
    }

    /// Check if current read is sequential
    pub fn is_sequential(&self, offset: u64) -> bool {
        self.state
            .as_ref()
            .map(|s| s.is_sequential(offset))
            .unwrap_or(false)
    }

    /// Get sequential read count
    pub fn sequential_count(&self) -> u32 {
        self.state.as_ref().map(|s| s.sequential_count).unwrap_or(0)
    }

    /// Mark prefetching state
    pub fn set_prefetching(&mut self, prefetching: bool) {
        if self.state.is_none() {
            self.state = Some(FileHandleState::new(0, 0));
        }
        if let Some(ref mut state) = self.state {
            state.is_prefetching = prefetching;
        }
    }

    /// Check if currently prefetching
    pub fn is_prefetching(&self) -> bool {
        self.state
            .as_ref()
            .map(|s| s.is_prefetching)
            .unwrap_or(false)
    }
}

/// Manager for file handles.
/// Allocates unique file handles and tracks open file state.
#[derive(Debug)]
pub struct FileHandleManager {
    /// Counter for generating unique handle IDs
    next_handle: AtomicU64,
    /// Map of handle IDs to handle information
    handles: Arc<Mutex<HashMap<u64, FileHandle>>>,
}

impl FileHandleManager {
    /// Create a new file handle manager
    pub fn new() -> Self {
        Self {
            next_handle: AtomicU64::new(1), // Start at 1, 0 is reserved/invalid
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Allocate a new file handle for an open file.
    /// Returns a unique handle ID.
    pub fn allocate(&self, inode: u64, flags: i32) -> u64 {
        let fh = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let handle = FileHandle::new(fh, inode, flags);

        let mut handles = self.handles.lock().unwrap();
        handles.insert(fh, handle);

        fh
    }

    /// Get file handle information by handle ID.
    pub fn get(&self, fh: u64) -> Option<FileHandle> {
        let handles = self.handles.lock().unwrap();
        handles.get(&fh).cloned()
    }

    /// Remove a file handle (called on release).
    /// Returns the removed handle information if it existed.
    pub fn remove(&self, fh: u64) -> Option<FileHandle> {
        let mut handles = self.handles.lock().unwrap();
        handles.remove(&fh)
    }

    /// Update file handle state (for read tracking).
    pub fn update_state(&self, fh: u64, offset: u64, size: u32) {
        let mut handles = self.handles.lock().unwrap();
        if let Some(handle) = handles.get_mut(&fh) {
            handle.update_state(offset, size);
        }
    }

    /// Set prefetching state for a handle.
    pub fn set_prefetching(&self, fh: u64, prefetching: bool) {
        let mut handles = self.handles.lock().unwrap();
        if let Some(handle) = handles.get_mut(&fh) {
            handle.set_prefetching(prefetching);
        }
    }

    /// Get the inode associated with a handle.
    pub fn get_inode(&self, fh: u64) -> Option<u64> {
        let handles = self.handles.lock().unwrap();
        handles.get(&fh).map(|h| h.inode)
    }

    /// Check if a handle exists.
    pub fn contains(&self, fh: u64) -> bool {
        let handles = self.handles.lock().unwrap();
        handles.contains_key(&fh)
    }

    /// Get the number of open handles.
    pub fn len(&self) -> usize {
        let handles = self.handles.lock().unwrap();
        handles.len()
    }

    /// Check if there are any open handles.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all handles for a specific inode.
    pub fn get_handles_for_inode(&self, inode: u64) -> Vec<u64> {
        let handles = self.handles.lock().unwrap();
        handles
            .iter()
            .filter(|(_, h)| h.inode == inode)
            .map(|(fh, _)| *fh)
            .collect()
    }

    /// Get all open handles (for cleanup).
    pub fn get_all_handles(&self) -> Vec<u64> {
        let handles = self.handles.lock().unwrap();
        handles.keys().copied().collect()
    }

    /// Remove handles that have exceeded the TTL (time-to-live).
    /// Returns the number of handles removed.
    pub fn remove_expired_handles(&self, ttl: Duration) -> usize {
        let mut handles = self.handles.lock().unwrap();
        let expired: Vec<u64> = handles
            .iter()
            .filter(|(_, handle)| handle.is_expired(ttl))
            .map(|(fh, _)| *fh)
            .collect();

        let count = expired.len();
        for fh in expired {
            handles.remove(&fh);
        }

        count
    }

    /// Get the total memory usage estimate for all handles in bytes.
    /// This is an approximation based on the size of FileHandle structs.
    pub fn memory_usage(&self) -> usize {
        let handles = self.handles.lock().unwrap();
        // FileHandle size: ~72 bytes (without state) + state overhead
        // This is a rough estimate for monitoring purposes
        handles.len() * std::mem::size_of::<FileHandle>()
    }

    /// Get the number of handles that would be expired with the given TTL.
    pub fn count_expired(&self, ttl: Duration) -> usize {
        let handles = self.handles.lock().unwrap();
        handles
            .values()
            .filter(|handle| handle.is_expired(ttl))
            .count()
    }
}

impl Default for FileHandleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_handle_allocation() {
        let manager = FileHandleManager::new();

        // Allocate first handle
        let fh1 = manager.allocate(100, libc::O_RDONLY);
        assert_eq!(fh1, 1);

        // Allocate second handle for same inode
        let fh2 = manager.allocate(100, libc::O_RDONLY);
        assert_eq!(fh2, 2);

        // Allocate handle for different inode
        let fh3 = manager.allocate(200, libc::O_RDONLY);
        assert_eq!(fh3, 3);

        // Verify handles are unique
        assert_ne!(fh1, fh2);
        assert_ne!(fh1, fh3);
        assert_ne!(fh2, fh3);
    }

    #[test]
    fn test_file_handle_lookup() {
        let manager = FileHandleManager::new();
        let inode = 100u64;
        let flags = libc::O_RDONLY;

        let fh = manager.allocate(inode, flags);

        // Lookup should succeed
        let handle = manager.get(fh).unwrap();
        assert_eq!(handle.fh, fh);
        assert_eq!(handle.inode, inode);
        assert_eq!(handle.flags, flags);

        // Lookup non-existent handle
        assert!(manager.get(9999).is_none());
    }

    #[test]
    fn test_file_handle_removal() {
        let manager = FileHandleManager::new();
        let fh = manager.allocate(100, libc::O_RDONLY);

        // Remove should return the handle
        let removed = manager.remove(fh).unwrap();
        assert_eq!(removed.fh, fh);

        // Second removal should fail
        assert!(manager.remove(fh).is_none());

        // Lookup should also fail
        assert!(manager.get(fh).is_none());
    }

    #[test]
    fn test_read_from_released_handle() {
        // EDGE-007: Test read from released handle
        // This simulates the scenario where a file handle is released
        // but something tries to read from it (which should return EBADF)
        let manager = FileHandleManager::new();

        // Open file, get handle
        let fh = manager.allocate(100, libc::O_RDONLY);
        assert!(manager.contains(fh));

        // Verify we can look up the handle (simulating a valid read operation)
        let handle = manager.get(fh);
        assert!(handle.is_some());
        assert_eq!(handle.unwrap().fh, fh);

        // Release handle (close the file)
        let removed = manager.remove(fh);
        assert!(removed.is_some());
        assert!(!manager.contains(fh));

        // Try to read from released handle (should return None, which translates to EBADF)
        let handle_after_release = manager.get(fh);
        assert!(
            handle_after_release.is_none(),
            "Reading from a released handle should return None (EBADF in FUSE layer)"
        );

        // Verify handle count is correct
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_file_handle_state_tracking() {
        let manager = FileHandleManager::new();
        let fh = manager.allocate(100, libc::O_RDONLY);

        // Update state
        manager.update_state(fh, 0, 1024);

        let handle = manager.get(fh).unwrap();
        assert_eq!(handle.sequential_count(), 1);

        // Sequential read
        manager.update_state(fh, 1024, 1024);
        let handle = manager.get(fh).unwrap();
        assert_eq!(handle.sequential_count(), 2);
        assert!(handle.is_sequential(2048));

        // Non-sequential read resets count
        manager.update_state(fh, 5000, 1024);
        let handle = manager.get(fh).unwrap();
        assert_eq!(handle.sequential_count(), 1);
        assert!(!handle.is_sequential(2048));
    }

    #[test]
    fn test_get_handles_for_inode() {
        let manager = FileHandleManager::new();

        // Open same file multiple times
        let fh1 = manager.allocate(100, libc::O_RDONLY);
        let fh2 = manager.allocate(100, libc::O_RDONLY);
        let fh3 = manager.allocate(200, libc::O_RDONLY);

        let handles_for_100 = manager.get_handles_for_inode(100);
        assert_eq!(handles_for_100.len(), 2);
        assert!(handles_for_100.contains(&fh1));
        assert!(handles_for_100.contains(&fh2));
        assert!(!handles_for_100.contains(&fh3));

        let handles_for_200 = manager.get_handles_for_inode(200);
        assert_eq!(handles_for_200.len(), 1);
        assert!(handles_for_200.contains(&fh3));
    }

    #[test]
    fn test_prefetching_state() {
        let manager = FileHandleManager::new();
        let fh = manager.allocate(100, libc::O_RDONLY);

        assert!(!manager.get(fh).unwrap().is_prefetching());

        manager.set_prefetching(fh, true);
        assert!(manager.get(fh).unwrap().is_prefetching());

        manager.set_prefetching(fh, false);
        assert!(!manager.get(fh).unwrap().is_prefetching());
    }
}
