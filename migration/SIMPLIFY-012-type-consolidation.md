# Migration Guide: SIMPLIFY-012 - Type Consolidation

## Task ID
SIMPLIFY-012

---

## Scope

### Files to Modify
- `src/types/torrent.rs` - Merge `TorrentFile` into this file
- `src/types/inode.rs` - Add macro to reduce boilerplate
- `src/types/attr.rs` - Add `base_attr()` helper
- `src/types/mod.rs` - Update module exports

### Files to Delete
- `src/types/file.rs` - Content merged into torrent.rs

---

## Current State

### file.rs (9 lines - TO BE DELETED)
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: u64,
    pub offset: u64,
}
```

### torrent.rs (12 lines)
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: u64,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
}
```

### inode.rs (64 lines) - Repetitive accessor methods
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InodeEntry {
    Directory {
        ino: u64,
        name: String,
        parent: u64,
        children: Vec<u64>,
    },
    File {
        ino: u64,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: usize,
        size: u64,
    },
    Symlink {
        ino: u64,
        name: String,
        parent: u64,
        target: String,
    },
}

impl InodeEntry {
    pub fn ino(&self) -> u64 {
        match self {
            InodeEntry::Directory { ino, .. } => *ino,
            InodeEntry::File { ino, .. } => *ino,
            InodeEntry::Symlink { ino, .. } => *ino,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            InodeEntry::Directory { name, .. } => name,
            InodeEntry::File { name, .. } => name,
            InodeEntry::Symlink { name, .. } => name,
        }
    }

    pub fn parent(&self) -> u64 {
        match self {
            InodeEntry::Directory { parent, .. } => *parent,
            InodeEntry::File { parent, .. } => *parent,
            InodeEntry::Symlink { parent, .. } => *parent,
        }
    }

    pub fn is_directory(&self) -> bool {
        matches!(self, InodeEntry::Directory { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self, InodeEntry::File { .. })
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self, InodeEntry::Symlink { .. })
    }
}
```

### attr.rs (45 lines) - Duplicate timestamp and common fields
```rust
use fuser::FileAttr;
use std::time::SystemTime;

pub fn default_file_attr(ino: u64, size: u64) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: fuser::FileType::RegularFile,
        perm: 0o444,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn default_dir_attr(ino: u64) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: fuser::FileType::Directory,
        perm: 0o755,
        nlink: 2,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}
```

---

## Target State

### torrent.rs (23 lines - after merging file.rs)
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: u64,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: u64,
    pub offset: u64,
}
```

### inode.rs (40 lines - using macro)
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InodeEntry {
    Directory {
        ino: u64,
        name: String,
        parent: u64,
        children: Vec<u64>,
    },
    File {
        ino: u64,
        name: String,
        parent: u64,
        torrent_id: u64,
        file_index: usize,
        size: u64,
    },
    Symlink {
        ino: u64,
        name: String,
        parent: u64,
        target: String,
    },
}

macro_rules! match_fields {
    ($self:expr, $($variant:ident => $field:ident),+ $(,)?) => {
        match $self {
            $(InodeEntry::$variant { $field, .. } => $field,)+
        }
    };
}

impl InodeEntry {
    pub fn ino(&self) -> u64 {
        *match_fields!(self, Directory => ino, File => ino, Symlink => ino)
    }

    pub fn name(&self) -> &str {
        match_fields!(self, Directory => name, File => name, Symlink => name)
    }

    pub fn parent(&self) -> u64 {
        *match_fields!(self, Directory => parent, File => parent, Symlink => parent)
    }

    pub fn is_directory(&self) -> bool {
        matches!(self, InodeEntry::Directory { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self, InodeEntry::File { .. })
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self, InodeEntry::Symlink { .. })
    }
}
```

### attr.rs (34 lines - using base_attr helper)
```rust
use fuser::FileAttr;
use std::time::SystemTime;

fn base_attr(ino: u64, size: u64) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: fuser::FileType::RegularFile,
        perm: 0o444,
        nlink: 1,
        uid: 1000,
        gid: 1000,
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
    attr.kind = fuser::FileType::Directory;
    attr.perm = 0o755;
    attr.nlink = 2;
    attr.blocks = 0;
    attr
}
```

### mod.rs changes
Remove the `pub mod file;` line since file.rs is deleted.

---

## Implementation Steps

1. **Merge file.rs into torrent.rs**
   - Copy the `TorrentFile` struct from `src/types/file.rs`
   - Paste it at the end of `src/types/torrent.rs`
   - Delete `src/types/file.rs`
   - Remove `pub mod file;` from `src/types/mod.rs`

2. **Add macro to inode.rs**
   - Add the `match_fields!` macro before the `InodeEntry` impl block
   - Update `ino()`, `name()`, and `parent()` methods to use the macro
   - Keep the `is_directory()`, `is_file()`, and `is_symlink()` methods unchanged (they already use `matches!`)

3. **Add base_attr helper to attr.rs**
   - Add a private `base_attr()` function that creates the base `FileAttr` with common values
   - Update `default_file_attr()` to call `base_attr(ino, size)`
   - Update `default_dir_attr()` to call `base_attr(ino, 0)` and modify specific fields

4. **Update any imports**
   - Search for `use crate::types::file::TorrentFile;`
   - Replace with `use crate::types::torrent::TorrentFile;`
   - Verify all imports compile

5. **Run tests and linting**
   - `cargo test` - Ensure all tests pass
   - `cargo clippy` - Fix any warnings
   - `cargo fmt` - Format the code

---

## Testing

### Compilation Check
```bash
cargo check
```

### Run Tests
```bash
cargo test
```

### Verify No Regressions
```bash
cargo test --lib
cargo clippy --all-targets
cargo fmt --check
```

### Manual Verification
1. Check that `TorrentFile` is still accessible as `crate::types::torrent::TorrentFile`
2. Verify `InodeEntry::ino()`, `name()`, and `parent()` work correctly
3. Verify `default_file_attr()` and `default_dir_attr()` produce same output as before

---

## Expected Reduction

| File | Before | After | Change |
|------|--------|-------|--------|
| torrent.rs | 12 lines | 23 lines | +11 |
| file.rs | 9 lines | 0 lines (deleted) | -9 |
| inode.rs | 64 lines | 40 lines | -24 |
| attr.rs | 45 lines | 34 lines | -11 |
| mod.rs | ~1 line | ~1 line | 0 |
| **Total** | **131 lines** | **98 lines** | **-33 lines** |

**Net reduction: ~33-44 lines** (depending on how you count macro impact and mod.rs changes)

The actual runtime code is reduced by ~44 lines while adding a macro for better maintainability.

---

## Verification Checklist

- [ ] `src/types/file.rs` deleted
- [ ] `TorrentFile` struct in `src/types/torrent.rs`
- [ ] `match_fields!` macro in `src/types/inode.rs`
- [ ] `base_attr()` helper in `src/types/attr.rs`
- [ ] All imports updated
- [ ] `cargo test` passes
- [ ] `cargo clippy` passes
- [ ] `cargo fmt` run
- [ ] Code reduction verified (~44 lines)

---

## Related Tasks

- TYPES-001: Research torrent type consolidation
- TYPES-002: Consolidate torrent representations
- ARCH-001: Audit module visibility

---

*Created for SIMPLIFY-012 type consolidation migration*
