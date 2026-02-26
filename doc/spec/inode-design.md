# Inode Table Design Specification

## Overview

This document describes the inode management system implementation in rqbit-fuse. The design uses DashMap for concurrent access, stores canonical paths in entries, and provides atomic operations for inode allocation and removal.

## 1. Current Inode Table Issues

### 1.1 Atomic Operations

**Implementation:** The `allocate_entry()` method uses DashMap's entry API for atomic insertion into the primary storage:

```rust
match self.entries.entry(inode) {
    dashmap::mapref::entry::Entry::Vacant(e) => {
        e.insert(entry);  // Atomic insertion
    }
    dashmap::mapref::entry::Entry::Occupied(_) => {
        panic!("Inode {} already exists (counter corrupted)", inode);
    }
}

// Indices are updated after primary entry is confirmed
self.path_to_inode.insert(path, inode);
```

**Design:** The primary entry insertion is atomic. Indices (`path_to_inode`, `torrent_to_inode`) are secondary and updated after the primary entry is confirmed. If index updates fail, the entry still exists and can be recovered.

**Notes:**
- `allocate()` - Uses entry API for atomic primary insertion
- `remove_inode()` - Performs removal in consistent order (children first, then parent references, then indices)
- `clear_torrents()` - Two-phase approach: collect entries, then remove atomically

### 1.2 Encapsulation

**Design:** The `entries` field is private. Controlled accessor methods are provided:

```rust
// Read accessors
pub fn get(&self, inode: u64) -> Option<InodeEntry>
pub fn contains(&self, inode: u64) -> bool
pub fn iter_entries(&self) -> impl Iterator<Item = InodeEntryRef> + '_
pub fn len(&self) -> usize
pub fn is_empty(&self) -> bool

// Note: torrent_to_inode() returns a reference for internal use
pub fn torrent_to_inode(&self) -> &DashMap<u64, u64>
```

**Rationale:** Direct DashMap access is restricted to maintain consistency between `entries`, `path_to_inode`, and `torrent_to_inode`. Tests use public APIs or the internal `entries` field directly via `get_all_inodes()`.

### 1.4 Stale Path References in remove_inode()

**Problem:** In `remove_inode()` at lines 294-296:

```rust
if let Some(entry) = self.entries.get(&inode) {
    let path = self.build_path(inode, &entry);
    self.path_to_inode.remove(&path);
    // ...
}
```

The `build_path()` method reconstructs the path by traversing parent pointers. If the filesystem structure has changed (e.g., parent renamed, moved), the reconstructed path may not match the actual stored path, causing the removal to fail silently.

**Example Failure Scenario:**
1. Directory `/torrents/movies` exists with inode 5
2. File `/torrents/movies/file.txt` exists with inode 6
3. Directory is renamed to `/torrents/films` (inode 5)
4. Calling `remove_inode(6)` builds path `/torrents/films/file.txt`
5. But `path_to_inode` still has old path `/torrents/movies/file.txt`
6. Removal fails because paths don't match

## 2. Design Alternatives Comparison

### 2.1 Current Multi-Map Approach (DashMap)

**Structure:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<u64, InodeEntry>,
    path_to_inode: DashMap<String, u64>,
    torrent_to_inode: DashMap<u64, u64>,
}
```

**Pros:**
- Simple, straightforward implementation
- Good concurrent read performance
- Individual maps can be locked independently

**Cons:**
- No atomicity across maps (critical issue)
- Multiple lookup structures to keep in sync
- Higher memory overhead (3x storage for keys)
- No way to ensure consistency during updates

**Verdict:** Current approach has fundamental correctness issues.

### 2.2 Single DashMap with Composite Keys

**Structure:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    // Single source of truth
    entries: DashMap<u64, InodeEntry>,
    // Composite indices (not separate stores)
    path_index: DashMap<String, u64>,  // path -> inode (reference only)
    torrent_index: DashMap<u64, u64>,  // torrent_id -> inode (reference only)
}

pub struct InodeEntry {
    ino: u64,
    name: String,
    parent: u64,
    entry_type: EntryType,
    // Canonical path stored within entry
    canonical_path: String,
}
```

**Access Pattern:**
- Lookups use indices for O(1) access
- Indices are rebuilt from entries on startup if needed
- Updates use DashMap entry API for atomicity

**Pros:**
- Single source of truth (`entries`)
- Can use DashMap's entry API for atomic operations
- Indices are derived, not primary storage
- Easier to ensure consistency

**Cons:**
- Still need to update multiple structures atomically
- Path stored redundantly (in entry and as index key)
- Complex transaction-like operations needed

**Verdict:** Better than current, but still has atomicity challenges.

### 2.3 RwLock + HashMap Approach

**Structure:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    // Single write lock for all operations
    state: RwLock<InodeState>,
}

struct InodeState {
    entries: HashMap<u64, InodeEntry>,
    path_to_inode: HashMap<String, u64>,
    torrent_to_inode: HashMap<u64, u64>,
}
```

**Access Pattern:**
- Read operations: acquire read lock
- Write operations: acquire write lock
- Batch operations within single lock scope

**Pros:**
- True atomicity for multi-map operations
- Simple, easy to reason about
- No consistency issues
- Can batch operations efficiently

**Cons:**
- Write lock is exclusive (no concurrent writes)
- Read lock may block during writes
- Less concurrent than DashMap for mixed workloads
- Lock contention under high write load

**Performance Analysis:**
- Read-heavy workloads: RwLock may outperform DashMap due to less overhead
- Write-heavy workloads: DashMap wins due to fine-grained locking
- rqbit-fuse workload: Mostly reads (FUSE operations), occasional writes (torrent add/remove)

**Verdict:** Good for correctness, may need optimization for high write loads.

### 2.4 DashMap with Transaction Pattern

**Structure:**
Same as current, but with explicit transaction objects:

```rust
pub struct InodeTransaction<'a> {
    manager: &'a InodeManager,
    staged: Vec<Operation>,
}

enum Operation {
    InsertEntry { inode: u64, entry: InodeEntry },
    InsertPath { path: String, inode: u64 },
    InsertTorrent { torrent_id: u64, inode: u64 },
}

impl<'a> InodeTransaction<'a> {
    pub fn commit(self) {
        // Apply all operations atomically by holding locks
        // Or use DashMap entry API for each
    }
}
```

**Pros:**
- Maintains DashMap concurrency benefits
- Explicit atomic batches
- Can implement rollback

**Cons:**
- Complex to implement correctly
- No true atomicity in DashMap across multiple maps
- Requires careful lock ordering to avoid deadlocks

**Verdict:** Overly complex for the problem space.

## 3. Recommended Design

### 3.1 Architecture Decision

**Selected Approach:** Single DashMap with Composite Keys + Entry API

**Rationale:**
1. rqbit-fuse has read-heavy workload (FUSE file operations)
2. DashMap provides excellent concurrent read performance
3. Entry API allows atomic check-and-set operations
4. Can add RwLock wrapper later if atomic batches are needed
5. Gradual migration path from current code

### 3.2 Atomic Inode Operations

**Design Principles:**
1. Single source of truth: `entries` DashMap
2. Indices are secondary and rebuilt if inconsistent
3. Use DashMap Entry API for atomic check-modify-write
4. Store canonical path in entry to avoid reconstruction

**Key Operations:**

```rust
// Atomic allocation
pub fn allocate(&self, entry: InodeEntry) -> u64 {
    let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);
    let entry = entry.with_inode(inode);
    
    // Use entry API for atomic insertion
    match self.entries.entry(inode) {
        Entry::Vacant(e) => {
            e.insert(entry.clone());
            // Update indices atomically relative to entry
            self.path_index.insert(entry.canonical_path().clone(), inode);
            if let Some(torrent_id) = entry.torrent_id() {
                if entry.is_directory() {
                    self.torrent_index.insert(torrent_id, inode);
                }
            }
            inode
        }
        Entry::Occupied(_) => {
            panic!("Inode {} already exists (counter corrupted)", inode);
        }
    }
}
```

### 3.3 Composite Key Structure

**InodeKey Design:**
```rust
/// Unique identifier for an inode entry
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InodeKey(u64);

impl InodeKey {
    pub const ROOT: Self = Self(1);
    
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Composite key for path-based lookups
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathKey(String);

impl PathKey {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Normalize path for consistent lookups
    pub fn normalize(&self) -> Self {
        // Remove trailing slashes, resolve ".." and "."
        Self(normalize_path(&self.0))
    }
}

/// Key for torrent directory lookups
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TorrentKey(u64);

impl TorrentKey {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}
```

### 3.4 Path Resolution Fix for Nested Directories

**Problem:** Current `build_path()` traverses parent pointers recursively, which:
1. Is O(depth) for each lookup
2. Can fail if parent chain is broken
3. Reconstructs path that may be stale

**Solution:** Store canonical path in each entry

```rust
pub struct InodeEntry {
    ino: InodeKey,
    name: String,
    parent: InodeKey,
    canonical_path: String,  // Pre-computed full path
    entry_type: EntryType,
}

impl InodeEntry {
    /// Returns the canonical path without reconstruction
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
    
    /// Builds canonical path during construction
    fn build_canonical_path(parent_path: &str, name: &str) -> String {
        if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        }
    }
}
```

**Path Resolution Algorithm:**
```rust
pub fn resolve_path(&self, path: &str) -> Option<InodeKey> {
    let normalized = PathKey::new(path).normalize();
    
    // Fast path: direct lookup
    if let Some(inode) = self.path_index.get(&normalized) {
        return Some(*inode);
    }
    
    // Slow path: traverse from root
    self.resolve_path_slow(&normalized)
}

fn resolve_path_slow(&self, path: &PathKey) -> Option<InodeKey> {
    let components: Vec<&str> = path.as_str()
        .split('/')
        .filter(|c| !c.is_empty())
        .collect();
    
    let mut current = InodeKey::ROOT;
    
    for component in components {
        let children = self.get_children(current);
        let found = children.iter()
            .find(|(_, entry)| entry.name() == component)
            .map(|(inode, _)| *inode);
        
        match found {
            Some(inode) => current = inode,
            None => return None,
        }
    }
    
    Some(current)
}
```

### 3.5 Private Entries with Accessor Methods

**Encapsulation Strategy:**
```rust
pub struct InodeManager {
    next_inode: AtomicU64,
    entries: DashMap<InodeKey, InodeEntry>,
    path_index: DashMap<PathKey, InodeKey>,
    torrent_index: DashMap<TorrentKey, InodeKey>,
}

impl InodeManager {
    // Read accessors
    pub fn get(&self, inode: InodeKey) -> Option<InodeEntry> {
        self.entries.get(&inode).map(|e| e.clone())
    }
    
    pub fn lookup_by_path(&self, path: &str) -> Option<InodeKey> {
        let key = PathKey::new(path).normalize();
        self.path_index.get(&key).map(|i| *i)
    }
    
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<InodeKey> {
        self.torrent_index.get(&TorrentKey::new(torrent_id))
            .map(|i| *i)
    }
    
    // Write operations (controlled)
    pub fn allocate(&self, entry: InodeEntry) -> InodeKey {
        // ... atomic allocation
    }
    
    pub fn remove_inode(&self, inode: InodeKey) -> Result<(), InodeError> {
        // ... atomic removal
    }
    
    // Internal batch operations for tests/migrations
    #[cfg(test)]
    pub(crate) fn insert_test_entry(&self, entry: InodeEntry) {
        // Only available in tests
    }
}
```

## 4. Data Structures

### 4.1 InodeManager Fields

```rust
use dashmap::DashMap;
use dashmap::DashSet;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe inode manager with atomic operations
pub struct InodeManager {
    /// Next available inode number (starts at 2, root is 1)
    next_inode: AtomicU64,
    
    /// Primary storage: inode number -> entry
    /// Source of truth for all inode data
    entries: DashMap<u64, InodeEntry>,
    
    /// Secondary index: path -> inode
    /// Rebuildable from entries if corrupted
    path_to_inode: DashMap<String, u64>,
    
    /// Secondary index: torrent ID -> torrent directory inode
    /// Only maps torrent directories (not files)
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
    /// Creates new manager with root inode pre-allocated
    pub fn new() -> Self {
        Self::with_max_inodes(0)
    }
    
    /// Creates new manager with a maximum inode limit
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
}
```

### 4.2 InodeEntry Types

```rust
/// Inode entry representing a filesystem object
#[derive(Debug, Clone)]
pub enum InodeEntry {
    Directory {
        ino: u64,
        name: String,
        parent: u64,
        children: DashSet<u64>,
        canonical_path: String,
    },
    File {
        ino: u64,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: u64,
        size: u64,
        canonical_path: String,
    },
    Symlink {
        ino: u64,
        name: String,
        parent: u64,
        target: String,
        canonical_path: String,
    },
}

impl InodeEntry {
    // Common accessors (work across all variants)
    pub fn ino(&self) -> u64
    pub fn name(&self) -> &str
    pub fn parent(&self) -> u64
    pub fn canonical_path(&self) -> &str
    
    // Type checks
    pub fn is_directory(&self) -> bool
    pub fn is_file(&self) -> bool
    pub fn is_symlink(&self) -> bool
    
    // Torrent-related (only valid for files)
    pub fn torrent_id(&self) -> Option<u64>
    
    // File size (only valid for files)
    pub fn file_size(&self) -> u64
    
    // Returns a new InodeEntry with the specified inode number
    pub fn with_ino(&self, ino: u64) -> Self
}
```

**Design Notes:**
- Uses enum variants instead of struct + EntryType for cleaner pattern matching
- `DashSet<u64>` for children provides efficient concurrent insert/remove
- Implements `Serialize` and `Deserialize` for persistence
- Common fields (`ino`, `name`, `parent`, `canonical_path`) exist in all variants

### 4.3 Lookup Methods

```rust
impl InodeManager {
    /// Lookup by inode number
    pub fn get(&self, inode: InodeKey) -> Option<InodeEntry> {
        self.entries.get(&inode).map(|e| e.clone())
    }
    
    /// Lookup by path (normalized)
    pub fn lookup_by_path(&self, path: &str) -> Option<InodeKey> {
        let key = PathKey::new(path).normalize();
        self.path_index.get(&key).map(|r| *r)
    }
    
    /// Lookup torrent directory by torrent ID
    pub fn lookup_torrent(&self, torrent_id: u64) -> Option<InodeKey> {
        self.torrent_index.get(&TorrentKey::new(torrent_id))
            .map(|r| *r)
    }
    
    /// Get children of a directory
    pub fn get_children(&self, parent: InodeKey) -> Vec<(InodeKey, InodeEntry)> {
        let parent_entry = match self.entries.get(&parent) {
            Some(e) => e.clone(),
            None => return Vec::new(),
        };
        
        let children = match parent_entry.children() {
            Some(c) => c.to_vec(),
            None => return Vec::new(),
        };
        
        children.iter()
            .filter_map(|&child_ino| {
                self.entries.get(&child_ino)
                    .map(|e| (child_ino, e.clone()))
            })
            .collect()
    }
    
    /// Find entry by name within a directory
    pub fn lookup_by_name(&self, parent: InodeKey, name: &str) -> Option<InodeKey> {
        self.get_children(parent)
            .into_iter()
            .find(|(_, entry)| entry.name() == name)
            .map(|(inode, _)| inode)
    }
    
    /// Resolve absolute path to inode
    pub fn resolve_path(&self, path: &str) -> Option<InodeKey> {
        // Try fast path first
        let normalized = PathKey::new(path).normalize();
        if let Some(inode) = self.path_index.get(&normalized) {
            return Some(*inode);
        }
        
        // Slow path: component-by-component resolution
        self.resolve_path_slow(&normalized)
    }
    
    /// Get all torrent IDs
    pub fn get_all_torrent_ids(&self) -> Vec<u64> {
        self.torrent_index.iter()
            .map(|item| item.key().as_u64())
            .collect()
    }
    
    /// Get inode count (excluding root)
    pub fn inode_count(&self) -> usize {
        self.entries.len().saturating_sub(1)
    }
    
    /// Get next available inode number
    pub fn next_inode(&self) -> u64 {
        self.next_inode.load(Ordering::SeqCst)
    }
}
```

## 5. Implementation Tasks

### 5.1 Task INODE-001: Research inode table design

**Status:** Complete (this document)

**Deliverables:**
- ✅ Compared current multi-map approach
- ✅ Evaluated single DashMap with composite keys
- ✅ Evaluated RwLock + HashMap approach
- ✅ Documented trade-offs
- ✅ Recommended design selected

### 5.2 Task INODE-002: Make inode table operations atomic

**Priority:** High

**Implementation Steps:**

1. **Update type definitions**
   ```rust
   // src/types/inode.rs
   #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
   pub struct InodeKey(u64);
   
   #[derive(Clone, Debug, PartialEq, Eq, Hash)]
   pub struct PathKey(String);
   
   #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
   pub struct TorrentKey(u64);
   ```

2. **Refactor InodeEntry to store canonical path**
   ```rust
   pub struct InodeEntry {
       ino: InodeKey,
       name: String,
       parent: InodeKey,
       canonical_path: String,  // NEW
       entry_type: EntryType,
   }
   ```

3. **Rewrite allocate() method**
   ```rust
   pub fn allocate(&self, entry: InodeEntry) -> InodeKey {
       let inode = InodeKey::new(
           self.next_inode.fetch_add(1, Ordering::SeqCst)
       );
       
       let entry = entry.with_inode(inode);
       
       // Atomic insertion
       match self.entries.entry(inode) {
           dashmap::Entry::Vacant(e) => {
               e.insert(entry.clone());
           }
           dashmap::Entry::Occupied(_) => {
               panic!("Inode {:?} already exists", inode);
           }
       }
       
       // Update indices (these can be repaired if needed)
       self.path_index.insert(
           PathKey::new(entry.canonical_path()),
           inode
       );
       
       if let Some(torrent_id) = entry.torrent_id() {
           if entry.is_directory() {
               self.torrent_index.insert(
                   TorrentKey::new(torrent_id),
                   inode
               );
           }
       }
       
       inode
   }
   ```

4. **Rewrite remove_inode() method**
   ```rust
   pub fn remove_inode(&self, inode: InodeKey) -> Result<(), InodeError> {
       if inode == InodeKey::ROOT {
           return Err(InodeError::CannotRemoveRoot);
       }
       
       // Get entry first to find path and children
       let entry = match self.entries.get(&inode) {
           Some(e) => e.clone(),
           None => return Err(InodeError::NotFound { inode }),
       };
       
       // Recursively remove children first
       if let Some(children) = entry.children() {
           for child in children.to_vec() {
               self.remove_inode(child)?;
           }
       }
       
       // Remove from parent (if not root)
       if entry.parent() != InodeKey::ROOT {
           if let Some(mut parent) = self.entries.get_mut(&entry.parent()) {
               let _ = parent.remove_child(inode);
           }
       }
       
       // Remove from indices using stored canonical path
       self.path_index.remove(&PathKey::new(entry.canonical_path()));
       
       if entry.is_directory() {
           // Find and remove from torrent_index
           let torrent_ids: Vec<TorrentKey> = self.torrent_index.iter()
               .filter(|item| *item.value() == inode)
               .map(|item| *item.key())
               .collect();
           
           for torrent_id in torrent_ids {
               self.torrent_index.remove(&torrent_id);
           }
       }
       
       // Finally, remove the entry itself
       self.entries.remove(&inode);
       
       Ok(())
   }
   ```

5. **Add tests for atomicity**
   ```rust
   #[test]
   fn test_concurrent_allocation_atomicity() {
       use std::sync::Arc;
       use std::thread;
       
       let manager = Arc::new(InodeManager::new());
       let mut handles = vec![];
       
       // Spawn threads that allocate and immediately verify
       for thread_id in 0..100 {
           let manager = Arc::clone(&manager);
           handles.push(thread::spawn(move || {
               for i in 0..10 {
                   let entry = InodeEntry::new_file(
                       InodeKey::new(0),
                       format!("file_{}_{}", thread_id, i),
                       InodeKey::ROOT,
                       format!("/file_{}_{}", thread_id, i),
                       thread_id as u64,
                       i as u64,
                       100,
                   );
                   
                   let inode = manager.allocate(entry);
                   
                   // Immediate verification
                   let retrieved = manager.get(inode);
                   assert!(retrieved.is_some(), "Allocated inode should exist");
                   
                   let path_lookup = manager.lookup_by_path(
                       &format!("/file_{}_{}", thread_id, i)
                   );
                   assert_eq!(path_lookup, Some(inode), "Path lookup should succeed");
               }
           }));
       }
       
       for handle in handles {
           handle.join().unwrap();
       }
       
       // Final consistency check
       assert_eq!(manager.inode_count(), 1000);
   }
   ```

### 5.3 Task INODE-003: Fix torrent directory mapping

**Priority:** High

**Current Bug:**
```rust
// WRONG: Maps torrent_id to file's parent
if let InodeEntry::File { torrent_id, .. } = &entry {
    self.torrent_to_inode.insert(*torrent_id, entry.parent());
}
```

**Fix:**
```rust
// CORRECT: Only map torrent directories
pub fn allocate_torrent_directory(&self, torrent_id: u64, name: String, parent: InodeKey) -> InodeKey {
    let inode = InodeKey::new(self.next_inode.fetch_add(1, Ordering::SeqCst));
    
    let canonical_path = if parent == InodeKey::ROOT {
        format!("/{}", name)
    } else {
        format!("{}/{}", self.get(parent).unwrap().canonical_path(), name)
    };
    
    let entry = InodeEntry::new_directory(
        inode,
        name,
        parent,
        canonical_path,
    );
    
    // Atomic insertion
    self.entries.insert(inode, entry);
    self.path_index.insert(PathKey::new(format!("/torrent_{}", torrent_id)), inode);
    
    // CORRECT: Map torrent_id to torrent directory inode
    self.torrent_index.insert(TorrentKey::new(torrent_id), inode);
    
    inode
}
```

**Update file allocation:**
```rust
pub fn allocate_file(
    &self,
    name: String,
    parent: InodeKey,
    torrent_id: u64,
    file_index: u64,
    size: u64,
) -> InodeKey {
    let inode = InodeKey::new(self.next_inode.fetch_add(1, Ordering::SeqCst));
    
    let parent_entry = self.get(parent).expect("Parent must exist");
    let canonical_path = format!("{}/{}", parent_entry.canonical_path(), name);
    
    let entry = InodeEntry::new_file(
        inode,
        name,
        parent,
        canonical_path,
        torrent_id,
        file_index,
        size,
    );
    
    self.entries.insert(inode, entry.clone());
    self.path_index.insert(PathKey::new(canonical_path), inode);
    
    // NOTE: Do NOT add to torrent_index - only directories are mapped
    
    inode
}
```

**Add verification test:**
```rust
#[test]
fn test_torrent_directory_mapping() {
    let manager = InodeManager::new();
    
    // Allocate torrent directory
    let torrent_inode = manager.allocate_torrent_directory(
        42, // torrent_id
        "MyTorrent".to_string(),
        InodeKey::ROOT,
    );
    
    // Lookup should return directory inode
    let found = manager.lookup_torrent(42);
    assert_eq!(found, Some(torrent_inode));
    
    // Allocate file in torrent
    let file_inode = manager.allocate_file(
        "file.txt".to_string(),
        torrent_inode,
        42,
        0,
        1000,
    );
    
    // File allocation should not change torrent mapping
    let still_found = manager.lookup_torrent(42);
    assert_eq!(still_found, Some(torrent_inode));
    assert_ne!(still_found, Some(file_inode));
}
```

### 5.4 Task INODE-004: Make entries field private

**Priority:** Medium

**Changes:**

1. **Remove public accessor:**
   ```rust
   // REMOVE THIS METHOD
   // pub fn entries(&self) -> &DashMap<u64, InodeEntry> {
   //     &self.entries
   // }
   ```

2. **Add controlled accessors:**
   ```rust
   impl InodeManager {
       /// Get a single entry by inode
       pub fn get(&self, inode: InodeKey) -> Option<InodeEntry> {
           self.entries.get(&inode).map(|e| e.clone())
       }
       
       /// Check if an inode exists
       pub fn contains(&self, inode: InodeKey) -> bool {
           self.entries.contains_key(&inode)
       }
       
       /// Iterate over all entries (read-only)
       pub fn iter_entries(&self) -> impl Iterator<Item = (InodeKey, InodeEntry)> + '_ {
           self.entries.iter()
               .map(|item| (*item.key(), item.value().clone()))
       }
       
       /// Get entry count
       pub fn len(&self) -> usize {
           self.entries.len()
       }
       
       /// Check if empty (only root)
       pub fn is_empty(&self) -> bool {
           self.entries.len() <= 1
       }
       
       /// Get all inodes (for debugging/migration)
       #[cfg(debug_assertions)]
       pub fn get_all_inodes(&self) -> Vec<InodeKey> {
           self.entries.iter()
               .map(|item| *item.key())
               .collect()
       }
   }
   ```

3. **Update tests that used direct access:**
   ```rust
   // OLD:
   let inodes: Vec<u64> = manager.entries()
       .iter()
       .map(|e| e.ino())
       .collect();
   
   // NEW:
   let inodes = manager.get_all_inodes();
   ```

4. **Add invariant checks (debug only):**
   ```rust
   #[cfg(debug_assertions)]
   pub fn verify_invariants(&self) -> Result<(), InodeError> {
       // Check that every entry has a valid parent (except root)
       for item in self.entries.iter() {
           let entry = item.value();
           if entry.ino() != InodeKey::ROOT {
               if !self.entries.contains_key(&entry.parent()) {
                   return Err(InodeError::InvalidParent {
                       inode: entry.ino(),
                       parent: entry.parent(),
                   });
               }
           }
       }
       
       // Check that path_index matches entries
       for item in self.path_index.iter() {
           let path = item.key();
           let inode = *item.value();
           
           match self.entries.get(&inode) {
               Some(entry) => {
                   if entry.canonical_path() != path.as_str() {
                       return Err(InodeError::PathMismatch {
                           inode,
                           expected: entry.canonical_path().to_string(),
                           actual: path.as_str().to_string(),
                       });
                   }
               }
               None => {
                   return Err(InodeError::DanglingPath {
                       path: path.as_str().to_string(),
                       inode,
                   });
               }
           }
       }
       
       Ok(())
   }
   ```

### 5.5 Task INODE-005: Fix path resolution

**Priority:** High

**Implementation:**

1. **Store canonical path during construction:**
   ```rust
   impl InodeEntry {
       pub fn with_inode(self, inode: InodeKey) -> Self {
           Self { ino: inode, ..self }
       }
       
       pub fn with_parent_and_path(
           self,
           parent: InodeKey,
           parent_path: &str,
           name: &str,
       ) -> Self {
           let canonical_path = if parent_path == "/" {
               format!("/{}", name)
           } else {
               format!("{}/{}", parent_path, name)
           };
           
           Self {
               parent,
               canonical_path,
               ..self
           }
       }
       
       pub fn canonical_path(&self) -> &str {
           &self.canonical_path
       }
   }
   ```

2. **Remove stale path rebuild logic:**
   ```rust
   // REMOVE build_path() method entirely
   // Paths are now stored in entries, not rebuilt
   ```

3. **Update remove_inode to use stored path:**
   ```rust
   pub fn remove_inode(&self, inode: InodeKey) -> Result<(), InodeError> {
       // ... get entry ...
       
       // Use stored canonical path - never rebuild
       let path_key = PathKey::new(entry.canonical_path());
       self.path_index.remove(&path_key);
       
       // ... rest of removal ...
   }
   ```

4. **Add path normalization:**
   ```rust
   impl PathKey {
       pub fn normalize(&self) -> Self {
           let path = &self.0;
           
           // Handle edge cases
           if path == "/" || path.is_empty() {
               return Self("/".to_string());
           }
           
           // Split and process components
           let components: Vec<&str> = path
               .split('/')
               .filter(|c| !c.is_empty() && *c != ".")
               .collect();
           
           // Resolve ".."
           let mut normalized = Vec::new();
           for comp in components {
               if comp == ".." {
                   normalized.pop();
               } else {
                   normalized.push(comp);
               }
           }
           
           // Rebuild path
           if normalized.is_empty() {
               Self("/".to_string())
           } else {
               Self(format!("/{}", normalized.join("/")))
           }
       }
   }
   ```

5. **Add path resolution tests:**
   ```rust
   #[test]
   fn test_path_normalization() {
       assert_eq!(PathKey::new("/").normalize().as_str(), "/");
       assert_eq!(PathKey::new("/foo/bar").normalize().as_str(), "/foo/bar");
       assert_eq!(PathKey::new("/foo/../bar").normalize().as_str(), "/bar");
       assert_eq!(PathKey::new("/foo/./bar").normalize().as_str(), "/foo/bar");
       assert_eq!(PathKey::new("/foo/bar/../..").normalize().as_str(), "/");
       assert_eq!(PathKey::new("///foo//bar").normalize().as_str(), "/foo/bar");
   }
   
   #[test]
   fn test_stored_path_consistency() {
       let manager = InodeManager::new();
       
       // Create nested structure
       let dir1 = manager.allocate(InodeEntry::new_directory(
           InodeKey::new(0),
           "dir1",
           InodeKey::ROOT,
           "/dir1",
       ));
       
       let dir2 = manager.allocate(InodeEntry::new_directory(
           InodeKey::new(0),
           "dir2",
           dir1,
           "/dir1/dir2",
       ));
       
       let file = manager.allocate(InodeEntry::new_file(
           InodeKey::new(0),
           "file.txt",
           dir2,
           "/dir1/dir2/file.txt",
           1,
           0,
           100,
       ));
       
       // Verify stored paths
       assert_eq!(manager.get(dir1).unwrap().canonical_path(), "/dir1");
       assert_eq!(manager.get(dir2).unwrap().canonical_path(), "/dir1/dir2");
       assert_eq!(manager.get(file).unwrap().canonical_path(), "/dir1/dir2/file.txt");
       
       // Verify lookups work
       assert_eq!(manager.lookup_by_path("/dir1"), Some(dir1));
       assert_eq!(manager.lookup_by_path("/dir1/dir2"), Some(dir2));
       assert_eq!(manager.lookup_by_path("/dir1/dir2/file.txt"), Some(file));
   }
   ```

### 5.6 Update Callers

**Files to update:**

1. **`src/fs/filesystem.rs`**
   - Replace `manager.entries().get()` → `manager.get()`
   - Replace `manager.entries().iter()` → `manager.iter_entries()`
   - Update to use `InodeKey` instead of `u64`
   - Fix path resolution calls

2. **`src/fs/inode.rs` (tests)**
   - Update all tests to use new API
   - Remove direct DashMap access
   - Add tests for new functionality

3. **Migration helpers (optional):**
   ```rust
   // Temporary helper for gradual migration
   impl InodeManager {
       #[deprecated(note = "Use get() instead")]
       pub fn get_entry(&self, inode: u64) -> Option<InodeEntry> {
           self.get(InodeKey::new(inode))
       }
   }
   ```

## Appendix A: Error Types

```rust
/// Errors that can occur during inode operations
#[derive(Debug, Clone, PartialEq)]
pub enum InodeError {
    NotFound { inode: InodeKey },
    CannotRemoveRoot,
    NotADirectory { inode: InodeKey },
    InvalidParent { inode: InodeKey, parent: InodeKey },
    PathMismatch { inode: InodeKey, expected: String, actual: String },
    DanglingPath { path: String, inode: InodeKey },
    AlreadyExists { inode: InodeKey },
    InvalidName { name: String, reason: String },
}

impl std::fmt::Display for InodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InodeError::NotFound { inode } => {
                write!(f, "Inode {:?} not found", inode)
            }
            InodeError::CannotRemoveRoot => {
                write!(f, "Cannot remove root inode")
            }
            InodeError::NotADirectory { inode } => {
                write!(f, "Inode {:?} is not a directory", inode)
            }
            InodeError::InvalidParent { inode, parent } => {
                write!(f, "Inode {:?} has invalid parent {:?}", inode, parent)
            }
            InodeError::PathMismatch { inode, expected, actual } => {
                write!(f, "Inode {:?} path mismatch: expected '{}', actual '{}'",
                    inode, expected, actual)
            }
            InodeError::DanglingPath { path, inode } => {
                write!(f, "Path '{}' points to non-existent inode {:?}", path, inode)
            }
            InodeError::AlreadyExists { inode } => {
                write!(f, "Inode {:?} already exists", inode)
            }
            InodeError::InvalidName { name, reason } => {
                write!(f, "Invalid name '{}': {}", name, reason)
            }
        }
    }
}

impl std::error::Error for InodeError {}
```

## Appendix B: Performance Considerations

### Memory Layout

**Current:**
- 3 DashMaps with redundant storage
- Path stored as key in path_to_inode and rebuilt from entries
- ~3x memory overhead for metadata

**New Design:**
- 1 primary DashMap + 2 index DashMaps
- Path stored once in entry, referenced by index
- ~1.5x memory overhead (indices store keys only)

### Lookup Performance

| Operation | Current | New Design |
|-----------|---------|------------|
| get(inode) | O(1) | O(1) |
| lookup_by_path | O(1) hash | O(1) hash + normalization |
| get_children | O(n) scan or O(1) list | O(1) list + O(c) lookups |
| resolve_path | O(d) recursive | O(d) component scan |

### Concurrent Access

**Read-heavy workloads (rqbit-fuse typical):**
- DashMap shines with sharded locks
- New design maintains this benefit
- Indices add minimal contention

**Write-heavy workloads:**
- May need batching with RwLock
- Consider single-writer pattern for bulk operations

## Appendix C: Migration Checklist

- [ ] Create new type definitions (InodeKey, PathKey, TorrentKey)
- [ ] Refactor InodeEntry to include canonical_path
- [ ] Rewrite InodeManager with private entries
- [ ] Implement new atomic allocate() method
- [ ] Implement new atomic remove_inode() method
- [ ] Fix torrent directory mapping
- [ ] Add path normalization
- [ ] Update all accessor methods
- [ ] Rewrite tests for new API
- [ ] Update filesystem.rs callers
- [ ] Add invariant verification (debug)
- [ ] Benchmark performance
- [ ] Document breaking changes

---

*Specification Version: 1.0*
*Date: February 14, 2026*
*Status: Ready for Implementation*
