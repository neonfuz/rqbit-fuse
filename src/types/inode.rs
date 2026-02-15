use dashmap::DashSet;
use serde::{Deserialize, Serialize};

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

impl Serialize for InodeEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        #[serde(tag = "type")]
        enum SerdeInodeEntry<'a> {
            Directory {
                ino: u64,
                name: &'a str,
                parent: u64,
                children: Vec<u64>,
                canonical_path: &'a str,
            },
            File {
                ino: u64,
                name: &'a str,
                parent: u64,
                torrent_id: u64,
                file_index: u64,
                size: u64,
                canonical_path: &'a str,
            },
            Symlink {
                ino: u64,
                name: &'a str,
                parent: u64,
                target: &'a str,
                canonical_path: &'a str,
            },
        }

        match self {
            InodeEntry::Directory {
                ino,
                name,
                parent,
                children,
                canonical_path,
            } => SerdeInodeEntry::Directory {
                ino: *ino,
                name,
                parent: *parent,
                children: children.iter().map(|c| *c).collect(),
                canonical_path,
            }
            .serialize(serializer),
            InodeEntry::File {
                ino,
                name,
                parent,
                torrent_id,
                file_index,
                size,
                canonical_path,
            } => SerdeInodeEntry::File {
                ino: *ino,
                name,
                parent: *parent,
                torrent_id: *torrent_id,
                file_index: *file_index,
                size: *size,
                canonical_path,
            }
            .serialize(serializer),
            InodeEntry::Symlink {
                ino,
                name,
                parent,
                target,
                canonical_path,
            } => SerdeInodeEntry::Symlink {
                ino: *ino,
                name,
                parent: *parent,
                target,
                canonical_path,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for InodeEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "type")]
        enum SerdeInodeEntry {
            Directory {
                ino: u64,
                name: String,
                parent: u64,
                children: Vec<u64>,
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

        let value = SerdeInodeEntry::deserialize(deserializer)?;
        match value {
            SerdeInodeEntry::Directory {
                ino,
                name,
                parent,
                children,
                canonical_path,
            } => Ok(InodeEntry::Directory {
                ino,
                name,
                parent,
                children: children.into_iter().collect(),
                canonical_path,
            }),
            SerdeInodeEntry::File {
                ino,
                name,
                parent,
                torrent_id,
                file_index,
                size,
                canonical_path,
            } => Ok(InodeEntry::File {
                ino,
                name,
                parent,
                torrent_id,
                file_index,
                size,
                canonical_path,
            }),
            SerdeInodeEntry::Symlink {
                ino,
                name,
                parent,
                target,
                canonical_path,
            } => Ok(InodeEntry::Symlink {
                ino,
                name,
                parent,
                target,
                canonical_path,
            }),
        }
    }
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

    /// Returns the stored canonical path
    pub fn canonical_path(&self) -> &str {
        match_fields!(self, Directory => canonical_path, File => canonical_path, Symlink => canonical_path)
    }

    /// Returns the torrent_id if this is a file
    pub fn torrent_id(&self) -> Option<u64> {
        match self {
            InodeEntry::File { torrent_id, .. } => Some(*torrent_id),
            _ => None,
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

    /// Returns a new InodeEntry with the specified inode number
    pub fn with_ino(&self, ino: u64) -> Self {
        match self {
            InodeEntry::Directory {
                name,
                parent,
                children,
                canonical_path,
                ..
            } => InodeEntry::Directory {
                ino,
                name: name.clone(),
                parent: *parent,
                children: children.clone(),
                canonical_path: canonical_path.clone(),
            },
            InodeEntry::File {
                name,
                parent,
                torrent_id,
                file_index,
                size,
                canonical_path,
                ..
            } => InodeEntry::File {
                ino,
                name: name.clone(),
                parent: *parent,
                torrent_id: *torrent_id,
                file_index: *file_index,
                size: *size,
                canonical_path: canonical_path.clone(),
            },
            InodeEntry::Symlink {
                name,
                parent,
                target,
                canonical_path,
                ..
            } => InodeEntry::Symlink {
                ino,
                name: name.clone(),
                parent: *parent,
                target: target.clone(),
                canonical_path: canonical_path.clone(),
            },
        }
    }
}
