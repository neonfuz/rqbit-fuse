//! Test fixtures for torrent data
//!
//! Provides predefined torrent structures for testing various scenarios
//! including single-file torrents, multi-file torrents, nested structures,
//! and edge cases like empty files or unicode names.

use rqbit_fuse::api::types::{FileInfo, TorrentInfo};

/// Create a single-file torrent fixture
///
/// Creates a simple torrent with a single file named "file.txt"
/// of size 1024 bytes.
///
/// # Arguments
/// * `id` - The torrent ID
///
/// # Returns
/// A TorrentInfo instance for testing
///
/// # Example
/// ```rust
/// let torrent = single_file_torrent(1);
/// assert_eq!(torrent.files.len(), 1);
/// ```
pub fn single_file_torrent(id: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("hash{}", id),
        name: format!("Single File {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "file.txt".to_string(),
            length: 1024,
            components: vec!["file.txt".to_string()],
        }],
        piece_length: Some(262144),
    }
}

/// Create a multi-file torrent fixture
///
/// Creates a torrent with multiple files including a README,
/// data file in a subdirectory, and an info file.
///
/// # Arguments
/// * `id` - The torrent ID
///
/// # Returns
/// A TorrentInfo instance with multiple files
///
/// # Example
/// ```rust
/// let torrent = multi_file_torrent(2);
/// assert_eq!(torrent.files.len(), 3);
/// ```
pub fn multi_file_torrent(id: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("hash{}", id),
        name: format!("Multi File {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(3),
        files: vec![
            FileInfo {
                name: "readme.txt".to_string(),
                length: 100,
                components: vec!["readme.txt".to_string()],
            },
            FileInfo {
                name: "data.bin".to_string(),
                length: 1024000,
                components: vec!["subdir".to_string(), "data.bin".to_string()],
            },
            FileInfo {
                name: "info.txt".to_string(),
                length: 200,
                components: vec!["subdir".to_string(), "info.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    }
}

/// Create a deeply nested torrent fixture
///
/// Creates a torrent with files nested at various depths.
///
/// # Arguments
/// * `id` - The torrent ID
/// * `depth` - The maximum nesting depth (number of directory levels)
///
/// # Returns
/// A TorrentInfo with deeply nested file structure
///
/// # Example
/// ```rust
/// let torrent = deeply_nested_torrent(3, 5);
/// assert!(torrent.files.len() >= 5);
/// ```
pub fn deeply_nested_torrent(id: u64, depth: usize) -> TorrentInfo {
    let mut files = vec![];
    let mut current_path = vec![];

    for i in 0..depth {
        current_path.push(format!("level{}", i));
        files.push(FileInfo {
            name: format!("file{}.txt", i),
            length: 100 * (i + 1) as u64,
            components: current_path.clone(),
        });
    }

    // Add a file at the root level
    files.push(FileInfo {
        name: "root.txt".to_string(),
        length: 50,
        components: vec!["root.txt".to_string()],
    });

    TorrentInfo {
        id,
        info_hash: format!("nested{}", id),
        name: format!("Nested {} levels", depth),
        output_folder: "/downloads".to_string(),
        file_count: Some(files.len()),
        files,
        piece_length: Some(262144),
    }
}

/// Create a unicode-named torrent fixture
///
/// Creates a torrent with files named using various unicode characters
/// including Chinese, Japanese, Russian, and emojis.
///
/// # Arguments
/// * `id` - The torrent ID
///
/// # Returns
/// A TorrentInfo with unicode filenames
///
/// # Example
/// ```rust
/// let torrent = unicode_torrent(100);
/// assert_eq!(torrent.files.len(), 4);
/// ```
pub fn unicode_torrent(id: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("unicode{}", id),
        name: format!("Unicode Test 🎉 {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "中文文件.txt".to_string(),
                length: 100,
                components: vec!["中文文件.txt".to_string()],
            },
            FileInfo {
                name: "日本語ファイル.txt".to_string(),
                length: 200,
                components: vec!["日本語ファイル.txt".to_string()],
            },
            FileInfo {
                name: "файл.txt".to_string(),
                length: 300,
                components: vec!["файл.txt".to_string()],
            },
            FileInfo {
                name: "emoji_🎊_file.txt".to_string(),
                length: 400,
                components: vec!["emoji_🎊_file.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    }
}

/// Create an empty-file torrent fixture
///
/// Creates a torrent containing a zero-byte file.
///
/// # Arguments
/// * `id` - The torrent ID
///
/// # Returns
/// A TorrentInfo with an empty file
///
/// # Example
/// ```rust
/// let torrent = empty_file_torrent(200);
/// assert_eq!(torrent.files[0].length, 0);
/// ```
pub fn empty_file_torrent(id: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("empty{}", id),
        name: format!("Empty File {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(1),
        files: vec![FileInfo {
            name: "empty.txt".to_string(),
            length: 0,
            components: vec!["empty.txt".to_string()],
        }],
        piece_length: Some(262144),
    }
}

/// Create a large-file torrent fixture
///
/// Creates a torrent with files of various large sizes.
///
/// # Arguments
/// * `id` - The torrent ID
/// * `size_mb` - The size of the large file in megabytes
///
/// # Returns
/// A TorrentInfo with large files
///
/// # Example
/// ```rust
/// let torrent = large_file_torrent(300, 100); // 100 MB file
/// assert_eq!(torrent.files[0].length, 100 * 1024 * 1024);
/// ```
pub fn large_file_torrent(id: u64, size_mb: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("large{}", id),
        name: format!("Large File {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(2),
        files: vec![
            FileInfo {
                name: "small.txt".to_string(),
                length: 100,
                components: vec!["small.txt".to_string()],
            },
            FileInfo {
                name: "large.bin".to_string(),
                length: size_mb * 1024 * 1024,
                components: vec!["large.bin".to_string()],
            },
        ],
        piece_length: Some(1048576),
    }
}

/// Create a torrent with special characters in filenames
///
/// Creates a torrent with filenames containing special characters
/// like spaces, parentheses, brackets, etc.
///
/// # Arguments
/// * `id` - The torrent ID
///
/// # Returns
/// A TorrentInfo with special character filenames
///
/// # Example
/// ```rust
/// let torrent = special_chars_torrent(400);
/// // Files have names like "file (1).txt", "file [test].txt"
/// ```
pub fn special_chars_torrent(id: u64) -> TorrentInfo {
    TorrentInfo {
        id,
        info_hash: format!("special{}", id),
        name: format!("Special Chars {}", id),
        output_folder: "/downloads".to_string(),
        file_count: Some(4),
        files: vec![
            FileInfo {
                name: "file (1).txt".to_string(),
                length: 100,
                components: vec!["file (1).txt".to_string()],
            },
            FileInfo {
                name: "file [test].txt".to_string(),
                length: 200,
                components: vec!["file [test].txt".to_string()],
            },
            FileInfo {
                name: "file with spaces.txt".to_string(),
                length: 300,
                components: vec!["file with spaces.txt".to_string()],
            },
            FileInfo {
                name: "file-dash_underscore.txt".to_string(),
                length: 400,
                components: vec!["file-dash_underscore.txt".to_string()],
            },
        ],
        piece_length: Some(262144),
    }
}
