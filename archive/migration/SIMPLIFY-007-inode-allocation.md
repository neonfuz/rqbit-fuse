# Migration Guide: SIMPLIFY-007 - Simplify Inode Allocation

## Task ID
SIMPLIFY-007

## Scope

**Files to modify:**
- `src/types/inode.rs` - Add `with_ino()` method to `InodeEntry`
- `src/fs/inode.rs` - Simplify allocation methods and `build_path()`

## Current State

### Current allocation methods (~124 lines total)

Four nearly identical allocation methods with duplicated boilerplate:

```rust
// src/fs/inode.rs lines 46-112
pub fn allocate(&self, entry: InodeEntry) -> u64 {
    let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

    // Create the entry with the correct inode number
    let entry = match entry {
        InodeEntry::Directory {
            name,
            parent,
            children,
            ..
        } => InodeEntry::Directory {
            ino: inode,
            name,
            parent,
            children,
        },
        InodeEntry::File {
            name,
            parent,
            torrent_id,
            file_index,
            size,
            ..
        } => InodeEntry::File {
            ino: inode,
            name,
            parent,
            torrent_id,
            file_index,
            size,
        },
        InodeEntry::Symlink {
            name,
            parent,
            target,
            ..
        } => InodeEntry::Symlink {
            ino: inode,
            name,
            parent,
            target,
        },
    };

    // Build the path for reverse lookup
    let path = self.build_path(inode, &entry);

    // Track torrent mappings
    if let InodeEntry::Directory { .. } = &entry {
        // Check if this is a torrent directory by looking at parent
        if let Some(parent_entry) = self.entries.get(&entry.parent()) {
            if parent_entry.ino() == 1 {
                // This is a torrent directory - parse torrent_id from name
            }
        }
    }

    if let InodeEntry::File { torrent_id, .. } = &entry {
        self.torrent_to_inode.insert(*torrent_id, entry.parent());
    }

    self.path_to_inode.insert(path, inode);
    self.entries.insert(inode, entry);

    inode
}

// Lines 116-133
pub fn allocate_torrent_directory(&self, torrent_id: u64, name: String, parent: u64) -> u64 {
    let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

    let entry = InodeEntry::Directory {
        ino: inode,
        name: name.clone(),
        parent,
        children: Vec::new(),
    };

    let path = self.build_path(inode, &entry);

    self.torrent_to_inode.insert(torrent_id, inode);
    self.path_to_inode.insert(path, inode);
    self.entries.insert(inode, entry);

    inode
}

// Lines 136-161
pub fn allocate_file(
    &self,
    name: String,
    parent: u64,
    torrent_id: u64,
    file_index: usize,
    size: u64,
) -> u64 {
    let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

    let entry = InodeEntry::File {
        ino: inode,
        name: name.clone(),
        parent,
        torrent_id,
        file_index,
        size,
    };

    let path = self.build_path(inode, &entry);

    self.path_to_inode.insert(path, inode);
    self.entries.insert(inode, entry);

    inode
}

// Lines 164-180
pub fn allocate_symlink(&self, name: String, parent: u64, target: String) -> u64 {
    let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

    let entry = InodeEntry::Symlink {
        ino: inode,
        name: name.clone(),
        parent,
        target,
    };

    let path = self.build_path(inode, &entry);

    self.path_to_inode.insert(path, inode);
    self.entries.insert(inode, entry);

    inode
}
```

### Current `build_path()` - recursive (lines 340-359)

```rust
fn build_path(&self, inode: u64, entry: &InodeEntry) -> String {
    if inode == 1 {
        return "/".to_string();
    }

    let name = entry.name();
    let parent = entry.parent();

    if parent == 1 {
        format!("/{}", name)
    } else {
        match self.entries.get(&parent) {
            Some(parent_entry) => {
                let parent_path = self.build_path(parent, &parent_entry);
                format!("{}/{}", parent_path, name)
            }
            None => format!("/{}", name),
        }
    }
}
```

## Target State

### Add `with_ino()` method to `InodeEntry` (~16 lines)

```rust
// src/types/inode.rs - add after existing methods (around line 62)
impl InodeEntry {
    // ... existing methods ...

    /// Returns a new InodeEntry with the specified inode number
    pub fn with_ino(&self, ino: u64) -> Self {
        match self {
            InodeEntry::Directory { name, parent, children, .. } => {
                InodeEntry::Directory {
                    ino,
                    name: name.clone(),
                    parent: *parent,
                    children: children.clone(),
                }
            }
            InodeEntry::File { name, parent, torrent_id, file_index, size, .. } => {
                InodeEntry::File {
                    ino,
                    name: name.clone(),
                    parent: *parent,
                    torrent_id: *torrent_id,
                    file_index: *file_index,
                    size: *size,
                }
            }
            InodeEntry::Symlink { name, parent, target, .. } => {
                InodeEntry::Symlink {
                    ino,
                    name: name.clone(),
                    parent: *parent,
                    target: target.clone(),
                }
            }
        }
    }
}
```

### Generic `allocate_entry()` helper (~12 lines)

```rust
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
```

### Simplified allocation methods (~16 lines total)

```rust
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
    let inode = self.allocate_entry(entry, None);
    self.torrent_to_inode.insert(torrent_id, parent);
    inode
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
```

### Simplified `build_path()` - iterative (~15 lines)

```rust
/// Builds the full path for an inode using iteration.
fn build_path(&self, entry: &InodeEntry) -> String {
    let mut components = vec![entry.name()];
    let mut current = entry.parent();

    while current != 1 {
        if let Some(parent_entry) = self.entries.get(&current) {
            components.push(parent_entry.name());
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
```

## Implementation Steps

### Step 1: Add `with_ino()` method

1. Open `src/types/inode.rs`
2. Add the `with_ino()` method to the `InodeEntry` impl block (after `is_symlink()` around line 62)
3. This method creates a copy of the entry with a new inode number

### Step 2: Add generic `allocate_entry()` helper

1. Open `src/fs/inode.rs`
2. Add the private `allocate_entry()` method before the public `allocate()` method
3. This handles the common pattern: allocate inode, build path, insert into maps

### Step 3: Simplify `allocate()` method

1. Replace the body of `allocate()` with a single call to `allocate_entry(entry, None)`
2. Remove all the match logic that was rebuilding the entry with the inode

### Step 4: Simplify `allocate_torrent_directory()`

1. Create the entry with `ino: 0`
2. Call `allocate_entry(entry, Some(torrent_id))`
3. Remove duplicate fetch_add, path building, and map insertion logic

### Step 5: Simplify `allocate_file()`

1. Create the entry with `ino: 0`
2. Call `allocate_entry(entry, None)`
3. Add the torrent_id → parent mapping after allocation
4. Remove duplicate boilerplate

### Step 6: Simplify `allocate_symlink()`

1. Create the entry with `ino: 0`
2. Call `allocate_entry(entry, None)`
3. Remove duplicate fetch_add, path building, and map insertion logic

### Step 7: Simplify `build_path()` to use iteration

1. Replace the recursive implementation with the iterative version
2. Collect path components in a vector, walking up parent chain
3. Reverse and join to form the full path
4. This prevents stack overflow for very deep paths and is more efficient

### Step 8: Update `remove_inode()`

1. Change `self.build_path(inode, &entry)` to `self.build_path(&entry)` (new signature)

### Step 9: Run lint and format

```bash
cargo clippy
cargo fmt
```

## Testing

Verify all functionality still works:

```bash
# Run all tests
cargo test

# Run inode-specific tests
cargo test inode::tests

# Run specific allocation tests
cargo test test_allocate_directory
cargo test test_allocate_file
cargo test test_allocate_torrent_directory
cargo test test_allocate_symlink
cargo test test_lookup_by_path
cargo test test_deep_nesting
```

**Expected test results:**
- All existing tests should pass
- No behavior changes - only code simplification
- Deep nesting test verifies the new iterative `build_path()`

## Expected Reduction

- **Lines removed:** ~64 lines
  - `allocate()`: 66 → 3 lines (removed entry rebuilding)
  - `allocate_torrent_directory()`: 17 → 9 lines
  - `allocate_file()`: 25 → 11 lines
  - `allocate_symlink()`: 16 → 9 lines
  - `build_path()`: 19 → 15 lines (recursion → iteration)
  - Added `allocate_entry()`: +12 lines
  - Added `with_ino()`: +16 lines
  
- **Code duplication:** Eliminated - all allocation logic in one place
- **Maintainability:** Improved
  - Changes to allocation logic only needed in `allocate_entry()`
  - `with_ino()` centralizes entry cloning logic
  - Iterative `build_path()` is safer (no stack overflow risk)

## Notes

- The `with_ino()` method uses `clone()` for Strings - this is acceptable as inode allocation is not on a hot path
- `build_path()` now takes `&InodeEntry` instead of `(u64, &InodeEntry)` since inode is already in the entry
- The torrent-to-inode mapping in `allocate_file()` is preserved but moved after `allocate_entry()`
- The unused torrent directory detection in the old `allocate()` was removed (it was incomplete anyway)
- All public method signatures remain unchanged - 100% API compatibility
