/// Log the start of a FUSE operation
///
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name (e.g., "read", "lookup")
/// * `$( $key = $value ),*` - Key-value pairs to log
#[macro_export]
macro_rules! fuse_log {
    ($self:expr, $op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                $( $key = $value, )*
            );
        }
    };
}

/// Log a FUSE error response
///
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name
/// * `$error` - Error code name (e.g., "ENOENT", "EINVAL")
/// * `$( $key = $value ),*` - Optional key-value pairs for additional context
#[macro_export]
macro_rules! fuse_error {
    ($self:expr, $op:expr, $error:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                result = "error",
                error = $error,
                $( $key = $value, )*
            );
        }
    };
}

/// Log a successful FUSE operation result
///
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name
/// * `$( $key = $value ),*` - Result fields to log
#[macro_export]
macro_rules! fuse_ok {
    ($self:expr, $op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                result = "success",
                $( $key = $value, )*
            );
        }
    };
}

/// Reply with ENOENT (inode not found) and record error metric
#[macro_export]
macro_rules! reply_ino_not_found {
    ($self:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $self.metrics.fuse.record_error();
        fuse_error!($self, $op, "ENOENT", ino = $ino);
        $reply.error(libc::ENOENT);
    }};
}

/// Reply with ENOTDIR (not a directory) and record error metric
#[macro_export]
macro_rules! reply_not_directory {
    ($self:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $self.metrics.fuse.record_error();
        fuse_error!($self, $op, "ENOTDIR", ino = $ino);
        $reply.error(libc::ENOTDIR);
    }};
}

/// Reply with EISDIR (is a directory, not a file) and record error metric
#[macro_export]
macro_rules! reply_not_file {
    ($self:expr, $reply:expr, $op:expr, $ino:expr) => {{
        $self.metrics.fuse.record_error();
        fuse_error!($self, $op, "EISDIR", ino = $ino);
        $reply.error(libc::EISDIR);
    }};
}

/// Reply with EACCES (permission denied) and record error metric
#[macro_export]
macro_rules! reply_no_permission {
    ($self:expr, $reply:expr, $op:expr, $ino:expr, $reason:expr) => {{
        $self.metrics.fuse.record_error();
        fuse_error!($self, $op, "EACCES", ino = $ino, reason = $reason);
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
