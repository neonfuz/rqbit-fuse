//! Common test utilities for rqbit-fuse
//!
//! This module provides shared test helpers to reduce duplication across test files.

use std::sync::Mutex;

/// Global mutex to ensure env var tests run sequentially
pub static ENV_VAR_MUTEX: Mutex<()> = Mutex::new(());

/// Guard type for environment variable tests
pub type EnvVarGuard = std::sync::MutexGuard<'static, ()>;

/// Acquire a lock for environment variable tests
pub fn lock_env_vars() -> EnvVarGuard {
    ENV_VAR_MUTEX.lock().unwrap()
}

/// Guard that removes an environment variable when dropped
#[allow(dead_code)]
pub struct EnvVar {
    name: String,
}

#[allow(dead_code)]
impl Drop for EnvVar {
    fn drop(&mut self) {
        std::env::remove_var(&self.name);
    }
}

/// Set an environment variable and return a guard that removes it on drop
#[allow(dead_code)]
pub fn set_env_var(name: &str, value: &str) -> EnvVar {
    std::env::set_var(name, value);
    EnvVar {
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_guard() {
        let _guard = lock_env_vars();

        // Set a test env var
        std::env::set_var("TEST_VAR_COMMON", "test_value");
        assert_eq!(std::env::var("TEST_VAR_COMMON").unwrap(), "test_value");

        // Clean up
        std::env::remove_var("TEST_VAR_COMMON");
    }
}
