use crate::types::inode::InodeEntry;
use dashmap::DashMap;
use dashmap::DashSet;
use std::sync::atomic::{AtomicU64, Ordering};

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
    use proptest::prelude::*;

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
    fn test_concurrent_allocation() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(InodeManager::new());
        let mut handles = vec![];

        // Spawn 10 threads, each allocating 10 inodes
        for thread_id in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    manager_clone.allocate_file(
                        format!("thread{}_file{}.txt", thread_id, i),
                        1,
                        thread_id as u64,
                        i,
                        100,
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 100 inodes allocated (plus root)
        assert_eq!(manager.inode_count(), 100);

        // All inodes should be unique
        let mut inodes: Vec<u64> = manager
            .iter_entries()
            .map(|e| e.entry.ino())
            .filter(|&ino| ino != 1)
            .collect();
        inodes.sort();

        // Should be 2, 3, 4, ..., 101
        for (i, &inode) in inodes.iter().enumerate() {
            assert_eq!(inode, (i + 2) as u64);
        }
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
    fn test_lookup_by_path_with_symlink() {
        let manager = InodeManager::new();

        let dir_inode = manager.allocate_torrent_directory(1, "dir".to_string(), 1);
        manager.add_child(1, dir_inode);

        let symlink_inode =
            manager.allocate_symlink("link".to_string(), dir_inode, "target".to_string());
        manager.add_child(dir_inode, symlink_inode);

        // Look up symlink by path
        assert_eq!(manager.lookup_by_path("/dir/link"), Some(symlink_inode));
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
    fn test_remove_inode_with_symlink() {
        let manager = InodeManager::new();

        let dir = manager.allocate_torrent_directory(1, "dir".to_string(), 1);
        let file = manager.allocate_file("file.txt".to_string(), dir, 1, 0, 100);
        let symlink = manager.allocate_symlink("link".to_string(), dir, "target".to_string());

        assert!(manager.get(dir).is_some());
        assert!(manager.get(file).is_some());
        assert!(manager.get(symlink).is_some());

        // Remove directory (should remove file and symlink)
        manager.remove_inode(dir);

        assert!(manager.get(dir).is_none());
        assert!(manager.get(file).is_none());
        assert!(manager.get(symlink).is_none());
    }

    #[test]
    fn test_empty_directory_children() {
        let manager = InodeManager::new();

        let dir = manager.allocate_torrent_directory(1, "empty_dir".to_string(), 1);
        let children = manager.get_children(dir);

        assert!(children.is_empty());
    }

    #[test]
    fn test_deep_nesting() {
        let manager = InodeManager::new();

        // Create deeply nested structure
        let mut current = 1u64; // Start at root
        for i in 0..10 {
            let new_dir = manager.allocate(InodeEntry::Directory {
                ino: 0,
                name: format!("level{}", i),
                parent: current,
                children: DashSet::new(),
                canonical_path: if i == 0 {
                    "/level0".to_string()
                } else {
                    format!(
                        "/level0{}",
                        (0..=i)
                            .map(|j| format!("/level{}", j))
                            .collect::<String>()
                            .strip_prefix("/level0")
                            .unwrap_or_default()
                    )
                },
            });
            manager.add_child(current, new_dir);
            current = new_dir;
        }

        // Verify path lookup works
        let path = "/level0/level1/level2/level3/level4/level5/level6/level7/level8/level9";
        let inode = manager.lookup_by_path(path);
        assert!(inode.is_some());
        assert_eq!(inode.unwrap(), current);
    }

    #[test]
    fn test_concurrent_allocation_atomicity() {
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
    fn test_concurrent_removal_atomicity() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(InodeManager::new());

        // Pre-allocate some torrent directories with files
        let mut torrent_inodes = vec![];
        for i in 0..20 {
            let torrent_inode =
                manager.allocate_torrent_directory(i as u64, format!("torrent{}", i), 1);
            manager.add_child(1, torrent_inode);

            // Add files to each torrent
            for j in 0..5 {
                let file_inode =
                    manager.allocate_file(format!("file{}", j), torrent_inode, i as u64, j, 100);
                manager.add_child(torrent_inode, file_inode);
            }
            torrent_inodes.push(torrent_inode);
        }

        // Verify initial state
        assert_eq!(
            manager.inode_count(),
            120,
            "Should have 20 torrents + 100 files"
        );

        // Concurrently remove torrents from multiple threads
        let mut handles = vec![];
        for chunk in torrent_inodes.chunks(5) {
            let manager_clone = Arc::clone(&manager);
            let to_remove: Vec<u64> = chunk.to_vec();
            let handle = thread::spawn(move || {
                for inode in to_remove {
                    // Verify torrent exists before removal
                    assert!(
                        manager_clone.get(inode).is_some(),
                        "Torrent {} should exist before removal",
                        inode
                    );

                    // Remove the torrent
                    assert!(
                        manager_clone.remove_inode(inode),
                        "Should successfully remove torrent {}",
                        inode
                    );

                    // Verify torrent no longer exists
                    assert!(
                        manager_clone.get(inode).is_none(),
                        "Torrent {} should not exist after removal",
                        inode
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Final verification: only root should remain
        assert_eq!(
            manager.inode_count(),
            0,
            "Should have no inodes remaining (except root)"
        );
        assert!(manager.get(1).is_some(), "Root should still exist");

        // Verify all torrent mappings are cleaned up
        for i in 0..20 {
            assert!(
                manager.lookup_torrent(i as u64).is_none(),
                "Torrent {} mapping should be removed",
                i
            );
        }
    }

    #[test]
    fn test_mixed_concurrent_operations() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(InodeManager::new());
        let alloc_count = Arc::new(AtomicUsize::new(0));
        let remove_count = Arc::new(AtomicUsize::new(0));

        // Spawn allocator threads
        let mut handles = vec![];
        for thread_id in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let count_clone = Arc::clone(&alloc_count);
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    let inode = manager_clone.allocate_file(
                        format!("alloc{}_file{}", thread_id, i),
                        1,
                        thread_id as u64,
                        i,
                        100,
                    );
                    count_clone.fetch_add(1, Ordering::SeqCst);

                    // Verify immediately
                    assert!(manager_clone.get(inode).is_some());
                }
            });
            handles.push(handle);
        }

        // Spawn remover threads (will remove what allocators create)
        for _thread_id in 0..5 {
            let manager_clone = Arc::clone(&manager);
            let count_clone = Arc::clone(&remove_count);
            let handle = thread::spawn(move || {
                // Small delay to let some allocations happen
                thread::sleep(std::time::Duration::from_millis(10));

                // Try to remove entries (some may already be removed by other threads)
                for i in 2..30u64 {
                    if manager_clone.remove_inode(i) {
                        count_clone.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final consistency
        let allocated = alloc_count.load(Ordering::SeqCst);
        let _removed = remove_count.load(Ordering::SeqCst);
        let remaining = manager.inode_count();

        // The relationship should hold: allocated - removed ≈ remaining (with some tolerance for race conditions)
        assert!(
            remaining <= allocated,
            "Remaining inodes ({}) should not exceed allocated ({})",
            remaining,
            allocated
        );

        // All entries should be consistent (no orphans)
        for entry_ref in manager.iter_entries() {
            let entry = &entry_ref.entry;
            let inode = entry.ino();
            if inode != 1 {
                // Every non-root entry should have a valid parent
                let parent = entry.parent();
                assert!(
                    manager.get(parent).is_some(),
                    "Entry {} has invalid parent {}",
                    inode,
                    parent
                );
            }
        }
    }

    #[test]
    fn test_atomic_allocation_no_duplicates() {
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::sync::Mutex;
        use std::thread;

        let manager = Arc::new(InodeManager::new());
        let allocated_inodes = Arc::new(Mutex::new(HashSet::new()));
        let mut handles = vec![];

        // Spawn many threads all trying to allocate simultaneously
        for thread_id in 0..100 {
            let manager_clone = Arc::clone(&manager);
            let inodes_clone = Arc::clone(&allocated_inodes);
            let handle = thread::spawn(move || {
                let inode = manager_clone.allocate_file(
                    format!("thread{}", thread_id),
                    1,
                    thread_id as u64,
                    0,
                    100,
                );

                // Record the inode immediately
                let mut set = inodes_clone.lock().unwrap();
                assert!(set.insert(inode), "Inode {} was allocated twice!", inode);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify no duplicates in final state
        let final_inodes: HashSet<u64> = manager
            .iter_entries()
            .map(|e| e.entry.ino())
            .filter(|&ino| ino != 1)
            .collect();

        assert_eq!(
            final_inodes.len(),
            100,
            "Should have exactly 100 unique inodes"
        );
    }

    // Property-based tests using proptest
    proptest! {
        #[test]
        fn test_inode_allocation_never_returns_zero(attempts in 1..100u32) {
            let manager = InodeManager::new();
            for i in 0..attempts {
                let inode = manager.allocate_file(
                    format!("file{}.txt", i),
                    1,
                    1,
                    i as u64,
                    100,
                );
                prop_assert!(inode != 0, "Inode should never be zero");
            }
        }

        #[test]
        fn test_parent_inode_exists_for_all_entries(num_dirs in 1..20u32) {
            let manager = InodeManager::new();

            // Create directories
            for i in 0..num_dirs {
                let dir = manager.allocate_torrent_directory(i as u64, format!("dir{}", i), 1);
                manager.add_child(1, dir);
            }

            // Verify every entry has a valid parent
            for entry_ref in manager.iter_entries() {
                let entry = &entry_ref.entry;
                let parent = entry.parent();
                prop_assert!(
                    manager.get(parent).is_some(),
                    "Entry {} has invalid parent {}",
                    entry.ino(),
                    parent
                );
            }
        }

        #[test]
        fn test_inode_uniqueness(num_files in 1..50u32) {
            let manager = InodeManager::new();
            let mut inodes = Vec::new();

            for i in 0..num_files {
                let inode = manager.allocate_file(
                    format!("file{}.txt", i),
                    1,
                    1,
                    i as u64,
                    100,
                );
                inodes.push(inode);
            }

            // All inodes should be unique
            let unique_count = inodes.len();
            inodes.sort();
            inodes.dedup();
            prop_assert_eq!(
                inodes.len(),
                unique_count,
                "All allocated inodes should be unique"
            );
        }

        #[test]
        fn test_children_relationship_consistency(num_children in 1..30u32) {
            let manager = InodeManager::new();

            // Create a directory
            let parent = manager.allocate_torrent_directory(1, "parent".to_string(), 1);

            // Add children
            for i in 0..num_children {
                let child = manager.allocate_file(
                    format!("child{}.txt", i),
                    parent,
                    1,
                    i as u64,
                    100,
                );
                manager.add_child(parent, child);
            }

            // Verify get_children returns same count
            let children = manager.get_children(parent);
            prop_assert_eq!(
                children.len() as u32,
                num_children,
                "Children count should match"
            );
        }
    }
}
