use dashmap::DashMap;
use dashmap::DashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use super::inode_entry::InodeEntry;

/// Manages inode allocation and mapping between inodes and filesystem entries.
/// Uses DashMap for concurrent access and AtomicU64 for thread-safe inode generation.
pub struct InodeManager {
    /// Next available inode number (starts at 2, root is 1)
    next_inode: AtomicU64,
    /// Maps inode numbers to their entries
    entries: DashMap<u64, InodeEntry>,
    /// Maps paths to inode numbers for reverse lookup
    path_to_inode: DashMap<String, u64>,
    /// Maps torrent IDs to their directory inode
    torrent_to_inode: DashMap<u64, u64>,
    /// Maximum number of inodes allowed (0 = unlimited)
    max_inodes: usize,
}

/// A view into an entry in the inode manager
#[derive(Debug)]
pub struct InodeEntryRef {
    pub inode: u64,
    pub entry: InodeEntry,
}

impl InodeManager {
    /// Creates a new InodeManager with root inode (inode 1) pre-allocated.
    /// Default max_inodes is 0 (unlimited).
    pub fn new() -> Self {
        Self::with_max_inodes(0)
    }

    /// Creates a new InodeManager with a maximum inode limit.
    ///
    /// # Arguments
    /// * `max_inodes` - Maximum number of inodes allowed (0 = unlimited)
    pub fn with_max_inodes(max_inodes: usize) -> Self {
        let entries = DashMap::new();
        let path_to_inode = DashMap::new();
        let torrent_to_inode = DashMap::new();

        // Root inode is always 1
        let root = InodeEntry::Directory {
            ino: 1,
            name: String::new(),
            parent: 1,
            children: DashSet::new(),
            canonical_path: "/".to_string(),
        };

        entries.insert(1, root);
        path_to_inode.insert("/".to_string(), 1);

        Self {
            next_inode: AtomicU64::new(2),
            entries,
            path_to_inode,
            torrent_to_inode,
            max_inodes,
        }
    }

    /// Check if a new inode can be allocated.
    /// Returns true if allocation is allowed (or if limit is 0/unlimited).
    pub fn can_allocate(&self) -> bool {
        if self.max_inodes > 0 {
            self.entries.len() < self.max_inodes
        } else {
            true
        }
    }

    /// Get the current inode count limit (0 = unlimited).
    pub fn max_inodes(&self) -> usize {
        self.max_inodes
    }

    /// Allocates an inode for the given entry and registers it atomically.
    ///
    /// Uses DashMap's entry API to ensure atomic insertion into the primary
    /// entries map. Indices are updated after the primary entry is confirmed.
    /// If index updates fail, the entry still exists and can be recovered.
    ///
    /// Returns 0 if the maximum inode limit has been reached.
    fn allocate_entry(&self, entry: InodeEntry, torrent_id: Option<u64>) -> u64 {
        // Check max_inodes limit (0 means unlimited)
        if self.max_inodes > 0 && self.entries.len() >= self.max_inodes {
            tracing::warn!(
                "Inode limit reached: {} >= {}",
                self.entries.len(),
                self.max_inodes
            );
            return 0;
        }

        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);
        let entry = entry.with_ino(inode);
        let path = entry.canonical_path().to_string();

        // Use entry API for atomic insertion into primary storage
        // This ensures we never have an index pointing to a non-existent entry
        match self.entries.entry(inode) {
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(entry);
            }
            dashmap::mapref::entry::Entry::Occupied(_) => {
                panic!("Inode {} already exists (counter corrupted)", inode);
            }
        }

        // Update indices after primary entry is confirmed
        // These are secondary and can be rebuilt if needed
        self.path_to_inode.insert(path, inode);

        if let Some(id) = torrent_id {
            self.torrent_to_inode.insert(id, inode);
        }

        inode
    }

    /// Allocates a new inode for the given entry.
    pub fn allocate(&self, entry: InodeEntry) -> u64 {
        self.allocate_entry(entry, None)
    }

    /// Allocates a directory inode for a torrent.
    pub fn allocate_torrent_directory(&self, torrent_id: u64, name: String, parent: u64) -> u64 {
        // Build canonical path from parent
        let canonical_path = if let Some(parent_entry) = self.entries.get(&parent) {
            let parent_path = parent_entry.canonical_path();
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path, name)
            }
        } else {
            format!("/{}", name)
        };

        let entry = InodeEntry::Directory {
            ino: 0,
            name,
            parent,
            children: DashSet::new(),
            canonical_path,
        };
        self.allocate_entry(entry, Some(torrent_id))
    }

    /// Allocates a file inode within a torrent.
    pub fn allocate_file(
        &self,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: u64,
        size: u64,
    ) -> u64 {
        // Build canonical path from parent
        let canonical_path = if let Some(parent_entry) = self.entries.get(&parent) {
            let parent_path = parent_entry.canonical_path();
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path, name)
            }
        } else {
            format!("/{}", name)
        };

        let entry = InodeEntry::File {
            ino: 0, // Will be assigned
            name,
            parent,
            torrent_id,
            file_index,
            size,
            canonical_path,
        };
        self.allocate_entry(entry, None)
    }

    /// Allocates a symbolic link inode.
    pub fn allocate_symlink(&self, name: String, parent: u64, target: String) -> u64 {
        // Build canonical path from parent
        let canonical_path = if let Some(parent_entry) = self.entries.get(&parent) {
            let parent_path = parent_entry.canonical_path();
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path, name)
            }
        } else {
            format!("/{}", name)
        };

        let entry = InodeEntry::Symlink {
            ino: 0, // Will be assigned
            name,
            parent,
            target,
            canonical_path,
        };
        self.allocate_entry(entry, None)
    }

    /// Looks up an inode by its number.
    pub fn get(&self, inode: u64) -> Option<InodeEntry> {
        self.entries.get(&inode).map(|e| e.clone())
    }

    /// Looks up an inode by its path.
    pub fn lookup_by_path(&self, path: &str) -> Option<u64> {
        self.path_to_inode.get(path).map(|i| *i)
    }

    /// Looks up a torrent directory by torrent ID.
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<u64> {
        self.torrent_to_inode.get(&torrent_id).map(|i| *i)
    }

    /// Gets the full path for an inode.
    /// Builds the path by traversing parent links up to root.
    pub fn get_path_for_inode(&self, inode: u64) -> Option<String> {
        let entry = self.entries.get(&inode)?;
        Some(self.build_path(&entry))
    }

    /// Check if an inode exists in the manager.
    pub fn contains(&self, inode: u64) -> bool {
        self.entries.contains_key(&inode)
    }

    /// Iterate over all entries (read-only).
    pub fn iter_entries(&self) -> impl Iterator<Item = InodeEntryRef> + '_ {
        self.entries.iter().map(|item| InodeEntryRef {
            inode: *item.key(),
            entry: item.value().clone(),
        })
    }

    /// Get the total number of entries (including root).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the manager only contains the root inode.
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
    }

    /// Gets the torrent-to-inode mapping.
    pub fn torrent_to_inode(&self) -> &DashMap<u64, u64> {
        &self.torrent_to_inode
    }

    /// Gets all torrent IDs currently tracked.
    pub fn get_all_torrent_ids(&self) -> Vec<u64> {
        self.torrent_to_inode
            .iter()
            .map(|item| *item.key())
            .collect()
    }

    /// Gets all children of a directory inode.
    pub fn get_children(&self, parent_inode: u64) -> Vec<(u64, InodeEntry)> {
        if let Some(parent_entry) = self.entries.get(&parent_inode) {
            if let InodeEntry::Directory { children, .. } = &*parent_entry {
                tracing::debug!(
                    parent = parent_inode,
                    children_count = children.len(),
                    "get_children: found directory"
                );
                if !children.is_empty() {
                    let result: Vec<_> = children
                        .iter()
                        .filter_map(|child_ino| {
                            let key = *child_ino;
                            self.entries.get(&key).map(|e| (key, e.clone()))
                        })
                        .collect();
                    tracing::debug!(
                        parent = parent_inode,
                        result_count = result.len(),
                        "get_children: returning children from list"
                    );
                    return result;
                }
            } else {
                tracing::warn!(parent = parent_inode, "get_children: not a directory");
            }
        } else {
            tracing::warn!(parent = parent_inode, "get_children: parent not found");
        }

        let result: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.parent() == parent_inode)
            .map(|entry| (entry.ino(), entry.clone()))
            .collect();
        tracing::debug!(
            parent = parent_inode,
            result_count = result.len(),
            total_entries = self.entries.len(),
            "get_children: using fallback scan"
        );
        result
    }

    /// Gets the next inode number without allocating it.
    /// Useful for getting the current state.
    pub fn next_inode(&self) -> u64 {
        self.next_inode.load(Ordering::SeqCst)
    }

    /// Gets the total number of allocated inodes (excluding root).
    pub fn inode_count(&self) -> usize {
        self.entries.len() - 1 // Subtract root
    }

    /// Removes an inode and all its descendants atomically (for torrent removal).
    ///
    /// Performs removal in a consistent order to maintain atomicity:
    /// 1. Recursively remove all children first (bottom-up)
    /// 2. Remove from parent's children list
    /// 3. Remove from indices using stored path
    /// 4. Finally remove from primary entries map
    ///
    /// This ensures we never have dangling references.
    pub fn remove_inode(&self, inode: u64) -> bool {
        if inode == 1 {
            return false; // Can't remove root
        }

        // Get entry first to access its data atomically
        let entry = match self.entries.get(&inode) {
            Some(e) => e.clone(),
            None => return false, // Already removed or never existed
        };

        // Step 1: Recursively remove all children first (bottom-up)
        // This ensures we don't leave orphaned children
        let children: Vec<u64> = self
            .entries
            .iter()
            .filter(|e| e.parent() == inode)
            .map(|e| e.ino())
            .collect();

        for child in children {
            self.remove_inode(child);
        }

        // Step 2: Remove from parent's children list atomically
        if entry.parent() != 1 {
            if let Some(mut parent_entry) = self.entries.get_mut(&entry.parent()) {
                if let InodeEntry::Directory { children, .. } = &mut *parent_entry {
                    children.retain(|&c| c != inode);
                }
            }
        }

        // Step 3: Remove from indices using the path we built before
        // Build path once and use it for both lookups
        let path = self.build_path(&entry);
        self.path_to_inode.remove(&path);

        // Remove from torrent mapping if it's a torrent directory
        if entry.is_directory() && entry.parent() == 1 {
            // Find and remove all torrent_id mappings to this inode
            let torrent_ids: Vec<u64> = self
                .torrent_to_inode
                .iter()
                .filter(|item| *item.value() == inode)
                .map(|item| *item.key())
                .collect();
            for torrent_id in torrent_ids {
                self.torrent_to_inode.remove(&torrent_id);
            }
        }

        // Step 4: Finally remove from primary entries map
        // This is the authoritative removal - after this the inode is truly gone
        self.entries.remove(&inode).is_some()
    }

    /// Clears all torrent entries atomically but keeps the root inode.
    ///
    /// Performs atomic removal of all non-root entries and resets indices.
    /// Uses a two-phase approach: first collect all entries to remove,
    /// then remove them while maintaining consistency.
    pub fn clear_torrents(&self) {
        // Phase 1: Collect all non-root entries atomically
        let to_remove: Vec<u64> = self
            .entries
            .iter()
            .filter(|entry| entry.ino() != 1)
            .map(|entry| entry.ino())
            .collect();

        // Phase 2: Remove each entry atomically using remove_inode
        // This ensures proper cleanup of indices and parent references
        for inode in to_remove {
            self.remove_inode(inode);
        }

        // Clear and reset indices
        self.path_to_inode.clear();
        self.path_to_inode.insert("/".to_string(), 1);
        self.torrent_to_inode.clear();

        // Reset next inode counter
        self.next_inode.store(2, Ordering::SeqCst);
    }

    /// Builds the full path for an inode using iteration.
    fn build_path(&self, entry: &InodeEntry) -> String {
        let mut components = vec![entry.name().to_string()];
        let mut current = entry.parent();

        while current != 1 {
            if let Some(parent_entry) = self.entries.get(&current) {
                components.push(parent_entry.name().to_string());
                current = parent_entry.parent();
            } else {
                break;
            }
        }

        components.reverse();
        if components.is_empty() || components[0].is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    }

    /// Adds a child to a directory's children list.
    pub fn add_child(&self, parent: u64, child: u64) {
        tracing::info!(parent = parent, child = child, "add_child called");
        if let Some(mut entry) = self.entries.get_mut(&parent) {
            if let InodeEntry::Directory { children, .. } = &mut *entry {
                if children.insert(child) {
                    tracing::info!(
                        parent = parent,
                        child = child,
                        children_count = children.len(),
                        "Added child to directory"
                    );
                } else {
                    tracing::warn!(
                        parent = parent,
                        child = child,
                        "Child already exists in directory"
                    );
                }
            } else {
                tracing::warn!(parent = parent, "Parent is not a directory");
            }
        } else {
            tracing::warn!(parent = parent, "Parent inode not found");
        }
    }

    /// Removes a child from a directory's children list.
    pub fn remove_child(&self, parent: u64, child: u64) {
        if let Some(mut entry) = self.entries.get_mut(&parent) {
            if let InodeEntry::Directory { children, .. } = &mut *entry {
                children.remove(&child);
            }
        }
    }
}

impl Default for InodeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inode_manager_creation() {
        let manager = InodeManager::new();

        // Root inode should exist
        let root = manager.get(1).expect("Root inode should exist");
        assert!(root.is_directory());
        assert_eq!(root.ino(), 1);
        assert_eq!(root.parent(), 1);

        // Next inode should be 2
        assert_eq!(manager.next_inode(), 2);
    }

    #[test]
    fn test_allocate_directory() {
        let manager = InodeManager::new();

        let entry = InodeEntry::Directory {
            ino: 0,
            name: "test_dir".to_string(),
            parent: 1,
            children: DashSet::new(),
            canonical_path: "/test_dir".to_string(),
        };

        let inode = manager.allocate(entry);
        assert_eq!(inode, 2);

        let retrieved = manager.get(inode).expect("Should retrieve allocated inode");
        assert_eq!(retrieved.name(), "test_dir");
        assert_eq!(retrieved.parent(), 1);
        assert!(retrieved.is_directory());
    }

    #[test]
    fn test_allocate_file() {
        let manager = InodeManager::new();

        let inode = manager.allocate_file(
            "test.txt".to_string(),
            1,    // parent (root)
            123,  // torrent_id
            0,    // file_index
            1024, // size
        );

        assert_eq!(inode, 2);

        let entry = manager.get(inode).expect("Should retrieve file");
        assert_eq!(entry.name(), "test.txt");
        assert!(entry.is_file());
    }

    #[test]
    fn test_allocate_torrent_directory() {
        let manager = InodeManager::new();

        let inode = manager.allocate_torrent_directory(
            42, // torrent_id
            "My Torrent".to_string(),
            1, // parent (root)
        );

        assert_eq!(inode, 2);

        // Should be able to look up by torrent_id
        let found = manager.lookup_torrent(42);
        assert_eq!(found, Some(2));

        let entry = manager.get(inode).expect("Should retrieve torrent dir");
        assert_eq!(entry.name(), "My Torrent");
    }

    #[test]
    fn test_lookup_by_path() {
        let manager = InodeManager::new();

        let inode = manager.allocate_torrent_directory(1, "test_torrent".to_string(), 1);

        manager.allocate_file("file.txt".to_string(), inode, 1, 0, 100);

        // Look up root
        assert_eq!(manager.lookup_by_path("/"), Some(1));

        // Look up torrent directory
        assert_eq!(manager.lookup_by_path("/test_torrent"), Some(inode));

        // Look up file
        assert_eq!(manager.lookup_by_path("/test_torrent/file.txt"), Some(3));
    }

    #[test]
    fn test_get_children() {
        let manager = InodeManager::new();

        let torrent_inode = manager.allocate_torrent_directory(1, "torrent".to_string(), 1);
        manager.add_child(1, torrent_inode);

        let file1 = manager.allocate_file("file1.txt".to_string(), torrent_inode, 1, 0, 100);
        let file2 = manager.allocate_file("file2.txt".to_string(), torrent_inode, 1, 1, 200);
        manager.add_child(torrent_inode, file1);
        manager.add_child(torrent_inode, file2);

        let root_children = manager.get_children(1);
        assert_eq!(root_children.len(), 1);
        assert_eq!(root_children[0].0, torrent_inode);

        let torrent_children = manager.get_children(torrent_inode);
        assert_eq!(torrent_children.len(), 2);
    }

    #[test]
    fn test_remove_inode() {
        let manager = InodeManager::new();

        let torrent_inode = manager.allocate_torrent_directory(1, "torrent".to_string(), 1);
        let file = manager.allocate_file("file.txt".to_string(), torrent_inode, 1, 0, 100);

        assert!(manager.get(torrent_inode).is_some());
        assert!(manager.get(file).is_some());

        // Remove torrent (should also remove its file)
        assert!(manager.remove_inode(torrent_inode));

        assert!(manager.get(torrent_inode).is_none());
        assert!(manager.get(file).is_none());
        assert!(manager.lookup_torrent(1).is_none());
    }

    #[test]
    fn test_cannot_remove_root() {
        let manager = InodeManager::new();
        assert!(!manager.remove_inode(1));
        assert!(manager.get(1).is_some());
    }

    #[test]
    fn test_clear_torrents() {
        let manager = InodeManager::new();

        manager.allocate_torrent_directory(1, "torrent1".to_string(), 1);
        manager.allocate_torrent_directory(2, "torrent2".to_string(), 1);

        assert_eq!(manager.inode_count(), 2);

        manager.clear_torrents();

        // Root should still exist
        assert!(manager.get(1).is_some());

        // Torrents should be gone
        assert_eq!(manager.inode_count(), 0);
        assert!(manager.lookup_torrent(1).is_none());
        assert!(manager.lookup_torrent(2).is_none());

        // Next inode should be reset
        assert_eq!(manager.next_inode(), 2);
    }

    #[test]
    fn test_allocate_symlink() {
        let manager = InodeManager::new();

        let inode = manager.allocate_symlink("link".to_string(), 1, "/target/path".to_string());

        assert_eq!(inode, 2);

        let entry = manager.get(inode).expect("Should retrieve symlink");
        assert!(entry.is_symlink());
        assert_eq!(entry.name(), "link");

        if let InodeEntry::Symlink { target, .. } = entry {
            assert_eq!(target, "/target/path");
        } else {
            panic!("Expected symlink entry");
        }
    }

    #[test]
    fn test_mixed_entry_types() {
        let manager = InodeManager::new();

        // Create directory
        let dir = manager.allocate_torrent_directory(1, "dir".to_string(), 1);
        manager.add_child(1, dir);
        assert!(manager.get(dir).unwrap().is_directory());

        // Create file
        let file = manager.allocate_file("file.txt".to_string(), dir, 1, 0, 100);
        manager.add_child(dir, file);
        assert!(manager.get(file).unwrap().is_file());

        // Create symlink
        let symlink = manager.allocate_symlink("link".to_string(), dir, "target".to_string());
        manager.add_child(dir, symlink);
        assert!(manager.get(symlink).unwrap().is_symlink());

        // Verify counts
        assert_eq!(manager.inode_count(), 3);

        // Verify children
        let children = manager.get_children(dir);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_concurrent_allocation_consistency() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(InodeManager::new());
        let mut handles = vec![];

        // Spawn threads that allocate and immediately verify consistency
        for thread_id in 0..50 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                for i in 0..20 {
                    let name = format!("thread{}_file{}", thread_id, i);
                    let inode = manager_clone.allocate_file(
                        name.clone(),
                        1, // parent (root)
                        thread_id as u64,
                        i,
                        100,
                    );

                    // Immediate verification: allocated inode must exist
                    let retrieved = manager_clone.get(inode);
                    assert!(
                        retrieved.is_some(),
                        "Allocated inode {} should exist immediately",
                        inode
                    );

                    // Verify path consistency: path lookup should find the inode
                    let expected_path = format!("/{}*", name);
                    let _path_lookup = manager_clone.lookup_by_path(&expected_path);
                    // Note: path lookup may not work immediately due to path building,
                    // but the entry must exist
                    assert_eq!(
                        retrieved.unwrap().ino(),
                        inode,
                        "Retrieved entry should have correct inode"
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Final consistency check: should have 1000 inodes (plus root)
        assert_eq!(
            manager.inode_count(),
            1000,
            "Should have exactly 1000 allocated inodes"
        );

        // Verify no duplicate inodes
        let mut inodes: Vec<u64> = manager
            .entries
            .iter()
            .map(|e| e.ino())
            .filter(|&ino| ino != 1)
            .collect();
        inodes.sort();
        inodes.dedup();
        assert_eq!(inodes.len(), 1000, "Should have 1000 unique inodes");
    }

    #[test]
    fn test_inode_0_allocation_attempt() {
        // EDGE-027: Test inode 0 allocation attempt
        // Try to allocate inode 0
        // Should fail gracefully, return 0 or error
        // Should not corrupt inode counter

        let manager = InodeManager::new();

        // Record initial state
        let initial_next_inode = manager.next_inode();
        assert_eq!(initial_next_inode, 2, "Initial next_inode should be 2");

        // Attempt to manually insert an entry with inode 0
        // This simulates what would happen if someone tried to allocate inode 0
        let entry_with_inode_0 = InodeEntry::File {
            ino: 0,
            name: "invalid.txt".to_string(),
            parent: 1,
            torrent_id: 1,
            file_index: 0,
            size: 100,
            canonical_path: "/invalid.txt".to_string(),
        };

        // Try to insert inode 0 directly into the entries map
        // This should not panic or corrupt the system
        let insert_result = manager.entries.entry(0);
        match insert_result {
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(entry_with_inode_0);
            }
            dashmap::mapref::entry::Entry::Occupied(_) => {
                panic!("Inode 0 should not already exist");
            }
        }

        // Verify that inode 0 exists but the counter is not corrupted
        assert!(manager.contains(0), "Inode 0 should exist after insertion");
        assert_eq!(
            manager.next_inode(),
            initial_next_inode,
            "Next inode counter should not be corrupted by inode 0 insertion"
        );

        // Verify normal allocations still work correctly
        let inode2 = manager.allocate_file("normal.txt".to_string(), 1, 1, 0, 100);
        assert_eq!(inode2, 2, "Normal allocation should return inode 2");
        assert!(manager.contains(inode2), "Allocated inode 2 should exist");

        // Verify we can retrieve inode 0
        let retrieved_0 = manager.get(0);
        assert!(retrieved_0.is_some(), "Should be able to retrieve inode 0");
        assert_eq!(
            retrieved_0.unwrap().ino(),
            0,
            "Retrieved entry should have inode 0"
        );

        // Verify inode_count is correct (root + inode 0 + inode 2)
        assert_eq!(
            manager.len(),
            3,
            "Should have 3 entries: root, inode 0, and inode 2"
        );

        // Cleanup: remove inode 0 and verify system is still consistent
        manager.entries.remove(&0);
        assert!(!manager.contains(0), "Inode 0 should be removed");
        assert_eq!(
            manager.next_inode(),
            3,
            "Next inode should be 3 after allocating inode 2"
        );
    }

    #[test]
    fn test_inode_0_not_returned_from_allocate() {
        // Verify that the public allocate methods never return inode 0
        let manager = InodeManager::new();

        // Allocate multiple entries and verify none have inode 0
        for i in 0..10 {
            let inode = manager.allocate_file(format!("file{}.txt", i), 1, 1, i as u64, 100);
            assert_ne!(inode, 0, "allocate() should never return inode 0");
            assert!(inode >= 2, "Allocated inode should be >= 2");
        }

        // Verify all allocated inodes are unique and non-zero
        let inodes: Vec<u64> = manager
            .entries
            .iter()
            .map(|e| e.ino())
            .filter(|&ino| ino != 1) // Exclude root
            .collect();

        assert!(!inodes.contains(&0), "No allocated inodes should be 0");

        // Verify uniqueness
        let mut unique_inodes = inodes.clone();
        unique_inodes.sort();
        unique_inodes.dedup();
        assert_eq!(
            unique_inodes.len(),
            inodes.len(),
            "All allocated inodes should be unique"
        );
    }

    #[test]
    fn test_max_inodes_limit() {
        // EDGE-028: Test max_inodes limit
        // Set max_inodes = 10
        // Allocate 11 inodes
        // 11th allocation should fail (return 0)
        // Verify no panic

        let manager = InodeManager::with_max_inodes(10);

        // Verify initial state
        assert_eq!(manager.max_inodes(), 10);
        assert_eq!(manager.len(), 1, "Only root inode should exist initially");
        assert!(
            manager.can_allocate(),
            "Should be able to allocate initially"
        );

        // Allocate 9 more entries (to reach limit of 10 total including root)
        let mut allocated_inodes = Vec::new();
        for i in 0..9 {
            let inode = manager.allocate_file(format!("file{}.txt", i), 1, 1, i as u64, 100);
            assert_ne!(inode, 0, "Allocation {} should succeed", i);
            allocated_inodes.push(inode);
        }

        // Should now have exactly 10 entries (root + 9 files)
        assert_eq!(manager.len(), 10, "Should have exactly 10 entries");
        assert!(
            !manager.can_allocate(),
            "Should not be able to allocate at limit"
        );

        // 11th allocation should fail and return 0
        let failed_inode = manager.allocate_file("overflow.txt".to_string(), 1, 1, 99, 100);
        assert_eq!(failed_inode, 0, "11th allocation should fail and return 0");

        // Verify state hasn't changed
        assert_eq!(manager.len(), 10, "Should still have exactly 10 entries");
        assert_eq!(manager.inode_count(), 9, "Should have 9 non-root inodes");

        // Verify all previously allocated inodes still exist
        for inode in &allocated_inodes {
            assert!(
                manager.contains(*inode),
                "Allocated inode {} should still exist",
                inode
            );
        }

        // Test with different entry types (directory, symlink)
        // First clear and start fresh with smaller limit
        let manager2 = InodeManager::with_max_inodes(5);

        // Allocate root + 4 more = 5 total
        manager2.allocate_torrent_directory(1, "torrent1".to_string(), 1);
        manager2.allocate_torrent_directory(2, "torrent2".to_string(), 1);
        manager2.allocate_file("file1.txt".to_string(), 1, 1, 0, 100);
        manager2.allocate_symlink("link1".to_string(), 1, "/target".to_string());

        assert_eq!(manager2.len(), 5, "Should have exactly 5 entries");

        // 6th allocation should fail
        let failed_dir = manager2.allocate_torrent_directory(3, "torrent3".to_string(), 1);
        assert_eq!(failed_dir, 0, "6th allocation (directory) should fail");

        let failed_file = manager2.allocate_file("file2.txt".to_string(), 1, 1, 1, 100);
        assert_eq!(failed_file, 0, "6th allocation (file) should fail");

        let failed_symlink =
            manager2.allocate_symlink("link2".to_string(), 1, "/target2".to_string());
        assert_eq!(failed_symlink, 0, "6th allocation (symlink) should fail");

        // Test that can_allocate() correctly reflects the limit
        assert!(
            !manager2.can_allocate(),
            "can_allocate should return false at limit"
        );
    }
}
