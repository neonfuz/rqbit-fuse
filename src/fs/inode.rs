use crate::types::inode::InodeEntry;
use dashmap::DashMap;
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
}

impl InodeManager {
    /// Creates a new InodeManager with root inode (inode 1) pre-allocated.
    pub fn new() -> Self {
        let entries = DashMap::new();
        let path_to_inode = DashMap::new();
        let torrent_to_inode = DashMap::new();

        // Root inode is always 1
        let root = InodeEntry::Directory {
            ino: 1,
            name: String::new(), // Root has no name
            parent: 1,           // Root is its own parent
            children: Vec::new(),
        };

        entries.insert(1, root);
        path_to_inode.insert("/".to_string(), 1);

        Self {
            next_inode: AtomicU64::new(2),
            entries,
            path_to_inode,
            torrent_to_inode,
        }
    }

    /// Allocates an inode for the given entry and registers it.
    fn allocate_entry(&self, entry: InodeEntry, torrent_id: Option<u64>) -> u64 {
        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);
        let entry = entry.with_ino(inode);
        let path = self.build_path(&entry);

        if let Some(id) = torrent_id {
            self.torrent_to_inode.insert(id, inode);
        }

        self.path_to_inode.insert(path, inode);
        self.entries.insert(inode, entry);

        inode
    }

    /// Allocates a new inode for the given entry.
    pub fn allocate(&self, entry: InodeEntry) -> u64 {
        self.allocate_entry(entry, None)
    }

    /// Allocates a directory inode for a torrent.
    pub fn allocate_torrent_directory(&self, torrent_id: u64, name: String, parent: u64) -> u64 {
        let entry = InodeEntry::Directory {
            ino: 0, // Will be assigned
            name,
            parent,
            children: Vec::new(),
        };
        self.allocate_entry(entry, Some(torrent_id))
    }

    /// Allocates a file inode within a torrent.
    pub fn allocate_file(
        &self,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: usize,
        size: u64,
    ) -> u64 {
        let entry = InodeEntry::File {
            ino: 0, // Will be assigned
            name,
            parent,
            torrent_id,
            file_index,
            size,
        };
        self.allocate_entry(entry, None)
    }

    /// Allocates a symbolic link inode.
    pub fn allocate_symlink(&self, name: String, parent: u64, target: String) -> u64 {
        let entry = InodeEntry::Symlink {
            ino: 0, // Will be assigned
            name,
            parent,
            target,
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

    /// Gets all entries in the inode manager.
    pub fn entries(&self) -> &DashMap<u64, InodeEntry> {
        &self.entries
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
        // First check if it's a directory and use its children list
        if let Some(parent_entry) = self.entries.get(&parent_inode) {
            if let InodeEntry::Directory { children, .. } = &*parent_entry {
                tracing::debug!(
                    parent = parent_inode,
                    children_count = children.len(),
                    "get_children: found directory"
                );
                // If children list is populated, use it
                if !children.is_empty() {
                    let result: Vec<_> = children
                        .iter()
                        .filter_map(|&child_ino| {
                            self.entries.get(&child_ino).map(|e| (child_ino, e.clone()))
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

        // Fallback: filter by parent field for entries not yet in children list
        // This handles the case where DashMap writes haven't propagated yet
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

    /// Removes an inode and all its descendants (for torrent removal).
    pub fn remove_inode(&self, inode: u64) -> bool {
        if inode == 1 {
            return false; // Can't remove root
        }

        // First, recursively remove all children
        let children: Vec<u64> = self
            .entries
            .iter()
            .filter(|entry| entry.parent() == inode)
            .map(|entry| entry.ino())
            .collect();

        for child in children {
            self.remove_inode(child);
        }

        // Remove from path mapping
        if let Some(entry) = self.entries.get(&inode) {
            let path = self.build_path(&entry);
            self.path_to_inode.remove(&path);

            // Remove from torrent mapping if it's a torrent directory
            if entry.is_directory() && entry.parent() == 1 {
                // Find the torrent_id by looking at which torrent maps to this inode
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
        }

        // Remove the inode itself
        self.entries.remove(&inode).is_some()
    }

    /// Clears all torrent entries but keeps the root inode.
    pub fn clear_torrents(&self) {
        // Remove all entries except root
        let to_remove: Vec<u64> = self
            .entries
            .iter()
            .filter(|entry| entry.ino() != 1)
            .map(|entry| entry.ino())
            .collect();

        for inode in to_remove {
            self.entries.remove(&inode);
        }

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
                if !children.contains(&child) {
                    children.push(child);
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
                children.retain(|&c| c != child);
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
            ino: 0, // Will be assigned
            name: "test_dir".to_string(),
            parent: 1,
            children: vec![],
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
            .entries
            .iter()
            .map(|e| e.ino())
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
                children: Vec::new(),
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
}
