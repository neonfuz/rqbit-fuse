# Torrent Types Research

## Overview

This document analyzes the different torrent type representations in the codebase and documents a consolidation strategy.

## Current Torrent Type Representations

### 1. `types::torrent::Torrent` (src/types/torrent.rs:4-11)

```rust
pub struct Torrent {
    pub id: u64,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
}
```

**Status**: DEAD CODE - Not imported or used anywhere in the codebase.

### 2. `types::torrent::TorrentFile` (src/types/torrent.rs:14-18)

```rust
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: u64,
    pub offset: u64,
}
```

**Status**: DEAD CODE - Not imported or used anywhere in the codebase.

### 3. `api::types::TorrentInfo` (src/api/types.rs:160-173)

```rust
pub struct TorrentInfo {
    pub id: u64,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
    pub file_count: Option<usize>,
    pub files: Vec<FileInfo>,
    pub piece_length: Option<u64>,
}
```

**Status**: ACTIVE - Used extensively throughout the codebase for torrent representation.

### 4. `api::types::TorrentSummary` (src/api/types.rs:144-151)

```rust
pub struct TorrentSummary {
    pub id: u64,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
}
```

**Status**: UNUSED - Defined but never used. API returns `TorrentInfo` directly via `list_torrents()`.

### 5. `api::types::FileStats` (src/api/types.rs:243-246)

```rust
pub struct FileStats {
    pub length: u64,
    pub included: bool,
}
```

**Status**: UNUSED - Defined but never used. File information is provided via `FileInfo`.

### 6. `api::types::TorrentListResponse` (src/api/types.rs:154-157)

```rust
pub struct TorrentListResponse {
    pub torrents: Vec<TorrentSummary>,
}
```

**Status**: UNUSED - Defined but never used. The codebase uses `ListTorrentsResult` instead.

## Consolidation Strategy

### Recommendation

1. **Remove `src/types/torrent.rs` entirely** - It contains dead code that is never used.

2. **Remove unused types in `src/api/types.rs`**:
   - `TorrentSummary` (lines 143-151)
   - `TorrentListResponse` (lines 154-157)
   - `FileStats` (lines 242-246)

3. **Keep `TorrentInfo` as the canonical type** - It's actively used and provides all necessary torrent information.

### Rationale

- `types::torrent::Torrent` was likely an early design that was superseded by `TorrentInfo` from the API layer
- `TorrentSummary`, `FileStats`, and `TorrentListResponse` appear to be vestigial types from API experiments
- The codebase has already consolidated around `TorrentInfo` from `api::types`

## Implementation Plan

1. Delete `src/types/torrent.rs`
2. Remove `pub mod torrent;` from `src/types/mod.rs`
3. Remove unused types from `src/api/types.rs`
4. Verify no breaking changes with `cargo build` and `cargo test`
5. Run `cargo clippy` and `cargo fmt`

## Dependencies to Check

After removal, verify the following still work:
- `cargo test` - all tests pass
- `cargo clippy` - no warnings about unused code
- Integration tests in `tests/integration_tests.rs`
- FUSE operation tests in `tests/fuse_operations.rs`

## Related Tasks

- TYPES-002: Consolidate torrent representations (follow-up)
- TYPES-003: Remove unused types (follow-up)
