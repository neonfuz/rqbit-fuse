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

    fn create_test_manager() -> InodeManager {
        InodeManager::new()
    }

    #[test]
    fn test_inode_manager_creation() {
        let manager = create_test_manager();

        let root = manager.get(1).expect("Root inode should exist");
        assert!(root.is_directory());
        assert_eq!(root.ino(), 1);
        assert_eq!(root.parent(), 1);

        assert_eq!(manager.next_inode(), 2);
    }

    #[test]
    fn test_allocate_directory() {
        let manager = create_test_manager();

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
        let manager = create_test_manager();

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
        let manager = create_test_manager();

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
        let manager = create_test_manager();

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
        let manager = create_test_manager();

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
        let manager = create_test_manager();

        let torrent_inode = manager.allocate_torrent_directory(1, "torrent".to_string(), 1);
        let file = manager.allocate_file("file.txt".to_string(), torrent_inode, 1, 0, 100);

        assert!(manager.get(torrent_inode).is_some());
        assert!(manager.get(file).is_some());

        assert!(manager.remove_inode(torrent_inode));

        assert!(manager.get(torrent_inode).is_none());
        assert!(manager.get(file).is_none());
        assert!(manager.lookup_torrent(1).is_none());
    }

    #[test]
    fn test_cannot_remove_root() {
        let manager = create_test_manager();
        assert!(!manager.remove_inode(1));
        assert!(manager.get(1).is_some());
    }

    #[test]
    fn test_clear_torrents() {
        let manager = create_test_manager();

        manager.allocate_torrent_directory(1, "torrent1".to_string(), 1);
        manager.allocate_torrent_directory(2, "torrent2".to_string(), 1);

        assert_eq!(manager.inode_count(), 2);

        manager.clear_torrents();

        assert!(manager.get(1).is_some());
        assert_eq!(manager.inode_count(), 0);
        assert!(manager.lookup_torrent(1).is_none());
        assert!(manager.lookup_torrent(2).is_none());
        assert_eq!(manager.next_inode(), 2);
    }

    #[test]
    fn test_allocate_symlink() {
        let manager = create_test_manager();

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
        let manager = create_test_manager();

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
    fn test_inode_0_handling() {
        let manager = create_test_manager();
        let entry = InodeEntry::File {
            ino: 0,
            name: "invalid.txt".to_string(),
            parent: 1,
            torrent_id: 1,
            file_index: 0,
            size: 100,
            canonical_path: "/invalid.txt".to_string(),
        };
        if let dashmap::mapref::entry::Entry::Vacant(e) = manager.entries.entry(0) {
            e.insert(entry);
        }
        assert!(manager.contains(0));
        assert_eq!(manager.next_inode(), 2);

        let inode2 = manager.allocate_file("normal.txt".to_string(), 1, 1, 0, 100);
        assert_eq!(inode2, 2);

        for i in 0..5 {
            let inode = manager.allocate_file(format!("file{}.txt", i), 1, 1, i as u64, 100);
            assert!(inode >= 2);
        }

        let inodes: Vec<u64> = manager
            .entries
            .iter()
            .map(|e| e.ino())
            .filter(|&ino| ino != 0 && ino != 1)
            .collect();
        assert!(!inodes.contains(&0));
        let unique_count = inodes
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(unique_count, inodes.len());
    }

    #[rstest::rstest]
    #[case(4, 10)]
    #[case(2, 5)]
    fn test_concurrent_allocation_stress(
        #[case] num_threads: usize,
        #[case] inodes_per_thread: usize,
    ) {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(InodeManager::new());
        let mut handles = vec![];
        let total_inodes = num_threads * inodes_per_thread;

        for thread_id in 0..num_threads {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let mut allocated = Vec::with_capacity(inodes_per_thread);
                for i in 0..inodes_per_thread {
                    let inode = manager_clone.allocate_file(
                        format!("t{}_f{}", thread_id, i),
                        1,
                        thread_id as u64,
                        i as u64,
                        100,
                    );
                    assert!(inode >= 2);
                    allocated.push(inode);
                }
                allocated
            });
            handles.push(handle);
        }

        let mut all_inodes: Vec<u64> = Vec::with_capacity(total_inodes);
        for handle in handles {
            all_inodes.extend(handle.join().unwrap());
        }

        assert_eq!(manager.inode_count(), total_inodes);
        let mut unique = all_inodes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), total_inodes);
        assert_eq!(manager.next_inode(), (total_inodes + 2) as u64);
    }

    #[rstest::rstest]
    #[case(100, 99)]
    #[case(10, 9)]
    fn test_inode_limit_exhaustion(#[case] max_inodes: usize, #[case] expected_allocations: usize) {
        let manager = InodeManager::with_max_inodes(max_inodes);
        assert_eq!(manager.len(), 1);
        assert!(manager.can_allocate());

        let mut allocated = Vec::new();
        for i in 0..expected_allocations {
            let inode = manager.allocate_torrent_directory(i as u64 + 1, format!("t{}", i), 1);
            assert!(inode >= 2);
            allocated.push(inode);
        }

        assert_eq!(manager.len(), max_inodes);
        assert!(!manager.can_allocate());

        assert_eq!(
            manager.allocate_torrent_directory(999, "overflow".to_string(), 1),
            0
        );
        assert_eq!(manager.allocate_file("f".to_string(), 1, 1, 0, 100), 0);
        assert_eq!(
            manager.allocate_symlink("l".to_string(), 1, "/t".to_string()),
            0
        );

        let first_inode = allocated[0];
        assert!(manager.remove_inode(first_inode));
        assert!(manager.can_allocate());

        let new_inode = manager.allocate_torrent_directory(999, "replacement".to_string(), 1);
        assert_ne!(new_inode, 0);
    }

    #[test]
    fn test_allocation_after_clear_torrents() {
        let manager = create_test_manager();

        let torrent1 = manager.allocate_torrent_directory(1, "torrent1".to_string(), 1);
        let file1 = manager.allocate_file("file1.txt".to_string(), torrent1, 1, 0, 100);
        let file2 = manager.allocate_file("file2.txt".to_string(), torrent1, 1, 1, 200);
        let torrent2 = manager.allocate_torrent_directory(2, "torrent2".to_string(), 1);
        let file3 = manager.allocate_file("file3.txt".to_string(), torrent2, 2, 0, 300);
        let symlink1 =
            manager.allocate_symlink("link1".to_string(), torrent2, "/target".to_string());

        assert_eq!(manager.inode_count(), 6);
        assert_eq!(manager.next_inode(), 8);

        let initial_inodes = vec![torrent1, file1, file2, torrent2, file3, symlink1];

        manager.clear_torrents();

        assert_eq!(manager.inode_count(), 0);
        assert_eq!(manager.next_inode(), 2);
        assert!(manager.get(1).is_some());

        for inode in &initial_inodes {
            assert!(manager.get(*inode).is_none());
        }

        assert!(manager.lookup_torrent(1).is_none());
        assert!(manager.lookup_torrent(2).is_none());

        let new_torrent1 = manager.allocate_torrent_directory(10, "new_torrent1".to_string(), 1);
        let new_file1 =
            manager.allocate_file("new_file1.txt".to_string(), new_torrent1, 10, 0, 1000);
        let new_torrent2 = manager.allocate_torrent_directory(11, "new_torrent2".to_string(), 1);
        let new_file2 =
            manager.allocate_file("new_file2.txt".to_string(), new_torrent2, 11, 0, 2000);

        assert_eq!(new_torrent1, 2);
        assert_eq!(new_file1, 3);
        assert_eq!(new_torrent2, 4);
        assert_eq!(new_file2, 5);
        assert_eq!(manager.next_inode(), 6);

        let all_inodes: Vec<u64> = manager.entries.iter().map(|e| e.ino()).collect();
        let mut unique_inodes = all_inodes.clone();
        unique_inodes.sort();
        unique_inodes.dedup();
        assert_eq!(unique_inodes.len(), all_inodes.len());

        assert_eq!(manager.lookup_torrent(10), Some(2));
        assert_eq!(manager.lookup_torrent(11), Some(4));
        assert_eq!(manager.lookup_by_path("/"), Some(1));
        assert_eq!(manager.lookup_by_path("/new_torrent1"), Some(2));
        assert_eq!(manager.lookup_by_path("/new_torrent2"), Some(4));

        manager.clear_torrents();

        let cycle2_torrent = manager.allocate_torrent_directory(20, "cycle2".to_string(), 1);
        assert_eq!(cycle2_torrent, 2);
        assert_eq!(manager.next_inode(), 3);
        assert!(manager.get(cycle2_torrent).is_some());
        assert!(manager.lookup_torrent(20).is_some());
    }
}
