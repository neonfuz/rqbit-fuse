//! Common test utilities for rqbit-fuse
//!
//! This module provides shared testing infrastructure including:
//! - Mock server setup for API testing
//! - FUSE filesystem helpers for mount/unmount operations  
//! - Test fixtures for torrent data
//! - Consolidated test helpers
//!
//! # Usage
//!
//! ```rust
//! use crate::common::test_helpers::*;
//! ```

pub mod fixtures;
pub mod fuse_helpers;
pub mod mock_server;
pub mod test_helpers;
