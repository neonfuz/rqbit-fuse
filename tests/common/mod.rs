//! Common test utilities for rqbit-fuse
//!
//! This module provides shared testing infrastructure including:
//! - Mock server setup for API testing
//! - FUSE filesystem helpers for mount/unmount operations
//! - Test fixtures for torrent data
//!
//! # Usage
//!
//! ```rust
//! use rqbit_fuse_test::common::{mock_server, fuse_helpers};
//! ```

pub mod fixtures;
pub mod fuse_helpers;
pub mod mock_server;

// Re-export commonly used items
pub use fixtures::*;
pub use fuse_helpers::*;
pub use mock_server::*;
