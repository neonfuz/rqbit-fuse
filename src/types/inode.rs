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

    /// Returns a new InodeEntry with the specified inode number
    pub fn with_ino(&self, ino: u64) -> Self {
        match self {
            InodeEntry::Directory {
                name,
                parent,
                children,
                ..
            } => InodeEntry::Directory {
                ino,
                name: name.clone(),
                parent: *parent,
                children: children.clone(),
            },
            InodeEntry::File {
                name,
                parent,
                torrent_id,
                file_index,
                size,
                ..
            } => InodeEntry::File {
                ino,
                name: name.clone(),
                parent: *parent,
                torrent_id: *torrent_id,
                file_index: *file_index,
                size: *size,
            },
            InodeEntry::Symlink {
                name,
                parent,
                target,
                ..
            } => InodeEntry::Symlink {
                ino,
                name: name.clone(),
                parent: *parent,
                target: target.clone(),
            },
        }
    }
}
