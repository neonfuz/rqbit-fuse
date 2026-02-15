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
/// * `$( $reason_key = $reason )?` - Optional reason field
#[macro_export]
macro_rules! fuse_error {
    ($self:expr, $op:expr, $error:expr $(, $reason_key:ident = $reason:expr)? $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                result = "error",
                error = $error,
                $( $reason_key = $reason, )?
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

// Re-export for internal use
pub use fuse_error;
pub use fuse_log;
pub use fuse_ok;
