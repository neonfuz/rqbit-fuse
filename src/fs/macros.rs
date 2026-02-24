/// Log the start of a FUSE operation
///
/// # Arguments
/// * `$op` - Operation name (e.g., "read", "lookup")
/// * `$( $key = $value ),*` - Key-value pairs to log
#[macro_export]
macro_rules! fuse_log {
    ($op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            $( $key = $value, )*
        );
    };
}

/// Log a FUSE error response
///
/// # Arguments
/// * `$op` - Operation name
/// * `$error` - Error code name (e.g., "ENOENT", "EINVAL")
/// * `$( $key = $value ),*` - Optional key-value pairs for additional context
#[macro_export]
macro_rules! fuse_error {
    ($op:expr, $error:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            result = "error",
            error = $error,
            $( $key = $value, )*
        );
    };
}

/// Log a successful FUSE operation result
///
/// # Arguments
/// * `$op` - Operation name
/// * `$( $key = $value ),*` - Result fields to log
#[macro_export]
macro_rules! fuse_ok {
    ($op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            result = "success",
            $( $key = $value, )*
        );
    };
}

/// Reply with ENOENT (inode not found) and record error metric
#[macro_export]
macro_rules! reply_ino_not_found {
    ($metrics:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $metrics.fuse.record_error();
        fuse_error!($op, "ENOENT", ino = $ino);
        $reply.error(libc::ENOENT);
    }};
}

/// Reply with ENOTDIR (not a directory) and record error metric
#[macro_export]
macro_rules! reply_not_directory {
    ($metrics:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $metrics.fuse.record_error();
        fuse_error!($op, "ENOTDIR", ino = $ino);
        $reply.error(libc::ENOTDIR);
    }};
}

/// Reply with EISDIR (is a directory, not a file) and record error metric
#[macro_export]
macro_rules! reply_not_file {
    ($metrics:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $metrics.fuse.record_error();
        fuse_error!($op, "EISDIR", ino = $ino);
        $reply.error(libc::EISDIR);
    }};
}

/// Reply with EACCES (permission denied) and record error metric
#[macro_export]
macro_rules! reply_no_permission {
    ($metrics:expr, $reply:expr, $op:expr, $ino:expr, $reason:expr) => {{
        $metrics.fuse.record_error();
        fuse_error!($op, "EACCES", ino = $ino, reason = $reason);
        $reply.error(libc::EACCES);
    }};
}

// Re-export for internal use
pub use fuse_error;
pub use fuse_log;
pub use fuse_ok;
pub use reply_ino_not_found;
pub use reply_no_permission;
pub use reply_not_directory;
pub use reply_not_file;
