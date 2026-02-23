//! Inode management module
//!
//! This module is maintained for backward compatibility.
//! The implementation has been split into:
//! - `inode_entry.rs` - InodeEntry enum and methods
//! - `inode_manager.rs` - InodeManager struct and methods

pub use super::inode_entry::InodeEntry;
pub use super::inode_manager::{InodeEntryRef, InodeManager};
