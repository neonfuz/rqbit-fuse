use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Information stored for each open file handle.
#[derive(Debug, Clone)]
pub struct FileHandle {
    /// The file handle ID (unique per open)
    pub fh: u64,
    /// The inode number this handle refers to
    pub inode: u64,
    /// The torrent ID this file belongs to
    pub torrent_id: u64,
    /// Open flags used when opening the file
    pub flags: i32,
}

impl FileHandle {
    /// Create a new file handle
    pub fn new(fh: u64, inode: u64, torrent_id: u64, flags: i32) -> Self {
        Self {
            fh,
            inode,
            torrent_id,
            flags,
        }
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
    /// Maximum number of file handles allowed (0 = unlimited)
    max_handles: usize,
}

impl FileHandleManager {
    /// Create a new file handle manager with unlimited handles
    pub fn new() -> Self {
        Self::with_max_handles(0)
    }

    /// Create a new file handle manager with a maximum handle limit
    ///
    /// # Arguments
    /// * `max_handles` - Maximum number of handles (0 = unlimited)
    pub fn with_max_handles(max_handles: usize) -> Self {
        Self {
            next_handle: AtomicU64::new(1), // Start at 1, 0 is reserved/invalid
            handles: Arc::new(Mutex::new(HashMap::new())),
            max_handles,
        }
    }

    /// Allocate a new file handle for an open file.
    /// Returns a unique handle ID, or 0 if handle limit is reached.
    pub fn allocate(&self, inode: u64, torrent_id: u64, flags: i32) -> u64 {
        // Check if we're at the handle limit
        if self.max_handles > 0 {
            let handles = self.handles.lock().unwrap();
            if handles.len() >= self.max_handles {
                // At limit - return 0 to indicate failure
                return 0;
            }
        }

        let fh = self.next_handle.fetch_add(1, Ordering::SeqCst);

        // Handle overflow: if we wrapped to 0, skip it and get next
        let fh = if fh == 0 {
            self.next_handle.fetch_add(1, Ordering::SeqCst)
        } else {
            fh
        };

        let handle = FileHandle::new(fh, inode, torrent_id, flags);

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

    /// Get the next handle value (for testing overflow scenarios).
    #[cfg(test)]
    pub fn set_next_handle(&self, value: u64) {
        self.next_handle.store(value, Ordering::SeqCst);
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

    /// Remove all file handles for a specific torrent.
    /// Returns the number of handles removed.
    pub fn remove_by_torrent(&self, torrent_id: u64) -> usize {
        let mut handles = self.handles.lock().unwrap();
        let handles_to_remove: Vec<u64> = handles
            .iter()
            .filter(|(_, h)| h.torrent_id == torrent_id)
            .map(|(fh, _)| *fh)
            .collect();

        let count = handles_to_remove.len();
        for fh in handles_to_remove {
            handles.remove(&fh);
        }

        count
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
        let fh1 = manager.allocate(100, 1, libc::O_RDONLY);
        assert_eq!(fh1, 1);

        // Allocate second handle for same inode
        let fh2 = manager.allocate(100, 1, libc::O_RDONLY);
        assert_eq!(fh2, 2);

        // Allocate handle for different inode
        let fh3 = manager.allocate(200, 1, libc::O_RDONLY);
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

        let torrent_id = 1u64;
        let fh = manager.allocate(inode, torrent_id, flags);

        // Lookup should succeed
        let handle = manager.get(fh).unwrap();
        assert_eq!(handle.fh, fh);
        assert_eq!(handle.inode, inode);
        assert_eq!(handle.torrent_id, torrent_id);
        assert_eq!(handle.flags, flags);

        // Lookup non-existent handle
        assert!(manager.get(9999).is_none());
    }

    #[test]
    fn test_file_handle_removal() {
        let manager = FileHandleManager::new();
        let fh = manager.allocate(100, 1, libc::O_RDONLY);

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
        let fh = manager.allocate(100, 1, libc::O_RDONLY);
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
    fn test_get_handles_for_inode() {
        let manager = FileHandleManager::new();

        // Open same file multiple times
        let fh1 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh2 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh3 = manager.allocate(200, 1, libc::O_RDONLY);

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
    fn test_handle_exhaustion() {
        // EDGE-008: Test handle exhaustion
        // Verify that allocating beyond max_handles returns 0 (indicating failure)
        const MAX_HANDLES: usize = 5;

        let manager = FileHandleManager::with_max_handles(MAX_HANDLES);

        // Allocate handles up to the limit
        let mut handles = Vec::new();
        for i in 0..MAX_HANDLES {
            let fh = manager.allocate(100 + i as u64, 1, libc::O_RDONLY);
            assert!(fh > 0, "Handle {} should be allocated successfully", i);
            handles.push(fh);
        }

        // Verify we have exactly MAX_HANDLES handles
        assert_eq!(
            manager.len(),
            MAX_HANDLES,
            "Should have {} handles",
            MAX_HANDLES
        );

        // Try to allocate one more - should return 0
        let extra_fh = manager.allocate(200, 1, libc::O_RDONLY);
        assert_eq!(extra_fh, 0, "Should return 0 when handle limit is exceeded");

        // Verify count hasn't changed
        assert_eq!(
            manager.len(),
            MAX_HANDLES,
            "Handle count should not increase after exhaustion"
        );

        // Release one handle
        let released = manager.remove(handles[0]);
        assert!(released.is_some(), "Should successfully release handle");
        assert_eq!(
            manager.len(),
            MAX_HANDLES - 1,
            "Handle count should decrease after release"
        );

        // Now we should be able to allocate again
        let new_fh = manager.allocate(300, 1, libc::O_RDONLY);
        assert!(
            new_fh > 0,
            "Should be able to allocate after releasing a handle"
        );
        assert_eq!(
            manager.len(),
            MAX_HANDLES,
            "Handle count should be back to max"
        );

        // Verify the new handle is different from the old ones
        assert!(!handles.contains(&new_fh), "New handle should be unique");
    }

    #[test]
    fn test_unlimited_handles() {
        // Test that unlimited handles (max_handles = 0) allows many allocations
        let manager = FileHandleManager::with_max_handles(0);

        // Allocate many handles
        for i in 0..100 {
            let fh = manager.allocate(100 + i as u64, 1, libc::O_RDONLY);
            assert!(fh > 0, "Handle {} should be allocated", i);
        }

        assert_eq!(manager.len(), 100, "Should have 100 handles");
    }

    #[test]
    fn test_handle_overflow() {
        // EDGE-009: Test handle allocation wrapping past u64::MAX
        // When the handle counter overflows, it should:
        // 1. Skip handle 0 (reserved/invalid)
        // 2. Maintain handle uniqueness

        let manager = FileHandleManager::new();

        // Set next_handle to u64::MAX - 2 to test overflow behavior
        manager.set_next_handle(u64::MAX - 2);

        // Allocate a few handles to trigger overflow
        let fh1 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh2 = manager.allocate(101, 1, libc::O_RDONLY);
        let fh3 = manager.allocate(102, 1, libc::O_RDONLY);
        let fh4 = manager.allocate(103, 1, libc::O_RDONLY);

        // Verify handle 0 is never allocated
        assert_ne!(fh1, 0, "Handle should never be 0");
        assert_ne!(fh2, 0, "Handle should never be 0");
        assert_ne!(fh3, 0, "Handle should never be 0");
        assert_ne!(fh4, 0, "Handle should never be 0");

        // Verify the sequence: u64::MAX-2, u64::MAX-1, u64::MAX, 1 (skipping 0)
        assert_eq!(fh1, u64::MAX - 2, "First handle should be u64::MAX - 2");
        assert_eq!(fh2, u64::MAX - 1, "Second handle should be u64::MAX - 1");
        assert_eq!(fh3, u64::MAX, "Third handle should be u64::MAX");
        assert_eq!(fh4, 1, "Fourth handle should wrap to 1 (skipping 0)");

        // Verify all handles are unique
        let handles = vec![fh1, fh2, fh3, fh4];
        let unique_handles: std::collections::HashSet<_> = handles.iter().cloned().collect();
        assert_eq!(
            unique_handles.len(),
            handles.len(),
            "All handles should be unique"
        );

        // Verify all handles are valid (can be looked up)
        for fh in &handles {
            assert!(
                manager.contains(*fh),
                "Handle {} should exist in manager",
                fh
            );
        }

        // Verify handle count
        assert_eq!(manager.len(), 4, "Should have 4 handles allocated");
    }
}
