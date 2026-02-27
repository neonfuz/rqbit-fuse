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

    /// Get the number of open handles.
    pub fn len(&self) -> usize {
        let handles = self.handles.lock().unwrap();
        handles.len()
    }

    /// Check if there are no open handles.
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

    fn create_manager() -> FileHandleManager {
        FileHandleManager::new()
    }

    #[test]
    fn test_handle_allocation_and_lookup() {
        let manager = create_manager();

        let fh1 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh2 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh3 = manager.allocate(200, 1, libc::O_RDONLY);

        assert_eq!(fh1, 1);
        assert_eq!(fh2, 2);
        assert_eq!(fh3, 3);
        assert_ne!(fh1, fh2);
        assert_ne!(fh1, fh3);

        let handle = manager.get(fh1).unwrap();
        assert_eq!(handle.fh, fh1);
        assert_eq!(handle.inode, 100);
        assert_eq!(handle.torrent_id, 1);

        assert!(manager.get(9999).is_none());
    }

    #[test]
    fn test_handle_removal() {
        let manager = create_manager();
        let fh = manager.allocate(100, 1, libc::O_RDONLY);

        assert!(manager.remove(fh).is_some());
        assert!(manager.remove(fh).is_none());
        assert!(manager.get(fh).is_none());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_get_handles_for_inode() {
        let manager = create_manager();

        let fh1 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh2 = manager.allocate(100, 1, libc::O_RDONLY);
        let fh3 = manager.allocate(200, 1, libc::O_RDONLY);

        let handles_for_100 = manager.get_handles_for_inode(100);
        assert_eq!(handles_for_100.len(), 2);
        assert!(handles_for_100.contains(&fh1));
        assert!(handles_for_100.contains(&fh2));

        let handles_for_200 = manager.get_handles_for_inode(200);
        assert_eq!(handles_for_200.len(), 1);
        assert!(handles_for_200.contains(&fh3));
    }

    #[test]
    fn test_handle_exhaustion() {
        let manager = FileHandleManager::with_max_handles(5);

        let mut handles = Vec::new();
        for i in 0..5 {
            let fh = manager.allocate(100 + i as u64, 1, libc::O_RDONLY);
            assert!(fh > 0);
            handles.push(fh);
        }

        assert_eq!(manager.len(), 5);
        assert_eq!(manager.allocate(200, 1, libc::O_RDONLY), 0);

        manager.remove(handles[0]);
        assert_eq!(manager.len(), 4);

        let new_fh = manager.allocate(300, 1, libc::O_RDONLY);
        assert!(new_fh > 0);
        assert_eq!(manager.len(), 5);
    }

    #[test]
    fn test_unlimited_handles() {
        let manager = FileHandleManager::with_max_handles(0);

        for i in 0..100 {
            let fh = manager.allocate(100 + i as u64, 1, libc::O_RDONLY);
            assert!(fh > 0);
        }

        assert_eq!(manager.len(), 100);
    }
}
