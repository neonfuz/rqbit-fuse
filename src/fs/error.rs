/// Trait for converting errors to FUSE error codes.
/// Implemented for anyhow::Error and RqbitFuseError to provide
/// consistent error mapping across the codebase.
///
/// This trait is kept in the fs::error module for backward compatibility
/// during the migration from FuseError to RqbitFuseError.
pub trait ToFuseError {
    /// Convert the error to a FUSE error code.
    fn to_fuse_error(&self) -> i32;
}

impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types through downcasting
        if let Some(rqbit_err) = self.downcast_ref::<crate::error::RqbitFuseError>() {
            return rqbit_err.to_errno();
        }

        if let Some(io_err) = self.downcast_ref::<std::io::Error>() {
            return match io_err.kind() {
                std::io::ErrorKind::NotFound => libc::ENOENT,
                std::io::ErrorKind::PermissionDenied => libc::EACCES,
                std::io::ErrorKind::TimedOut => libc::ETIMEDOUT,
                std::io::ErrorKind::InvalidInput => libc::EINVAL,
                _ => libc::EIO,
            };
        }

        // Default to EIO for unknown errors
        libc::EIO
    }
}
