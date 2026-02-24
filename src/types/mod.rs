//! Core data types for inodes, handles, and file attributes.

pub mod attr;
pub mod handle;

pub use crate::fs::inode::InodeEntry;
pub use fuser::FileAttr;
pub use handle::FileHandle;
