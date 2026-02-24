use rqbit_fuse::config::Config;
use std::sync::Mutex;

// Use a global mutex to ensure env var tests run sequentially
static ENV_VAR_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_edge_056_timeout_negative_from_env() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // When parsing from environment, a negative value would fail to parse as u64
    // This tests that the error is handled gracefully
    let env_var = "TORRENT_FUSE_READ_TIMEOUT";
    std::env::set_var(env_var, "-1");

    let config = Config::default();
    let result = config.merge_from_env();

    // Clean up
    std::env::remove_var(env_var);

    assert!(
        result.is_err(),
        "Negative timeout from environment should fail to parse"
    );
}

#[test]
fn test_edge_056_timeout_negative_large_from_env() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test with a large negative number
    let env_var = "TORRENT_FUSE_READ_TIMEOUT";
    std::env::set_var(env_var, "-3600");

    let config = Config::default();
    let result = config.merge_from_env();

    // Clean up
    std::env::remove_var(env_var);

    assert!(
        result.is_err(),
        "Large negative timeout from environment should fail to parse"
    );
}

#[test]
fn test_edge_056_timeout_valid_values() {
    // Test various valid timeout values - after simplification, any positive value is valid
    let valid_timeouts = [1, 30, 60, 300, 1800, 3600, 7200, u64::MAX];

    for timeout in valid_timeouts {
        let mut config = Config::default();
        config.performance.read_timeout = timeout;

        assert!(
            config.validate().is_ok(),
            "Timeout of {} should be valid (no upper bound after simplification)",
            timeout
        );
    }
}

#[test]
fn test_edge_056_timeout_one() {
    // Minimum valid timeout
    let mut config = Config::default();
    config.performance.read_timeout = 1;

    assert!(
        config.validate().is_ok(),
        "Timeout of 1 should be valid (minimum valid value)"
    );
}

#[test]
fn test_edge_056_invalid_timeout_from_env_handling() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test various invalid formats in environment variables
    let test_cases = [
        ("-1", "negative number"),
        ("abc", "letters"),
        ("30.5", "decimal"),
        ("", "empty string"),
        (" ", "whitespace"),
        ("30s", "with unit suffix"),
    ];

    for (value, description) in test_cases {
        let env_var = "TORRENT_FUSE_READ_TIMEOUT";
        std::env::set_var(env_var, value);

        let config = Config::default();
        let result = config.merge_from_env();

        // Clean up
        std::env::remove_var(env_var);

        assert!(
            result.is_err(),
            "Invalid timeout value '{}' ({}) should fail to parse",
            value,
            description
        );
    }
}

#[test]
fn test_edge_057_missing_required_env_vars() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Ensure required env vars are not set (they shouldn't be in test environment)
    // This tests that the system handles missing env vars gracefully by using defaults
    let vars_to_check = [
        "TORRENT_FUSE_API_URL",
        "TORRENT_FUSE_MOUNT_POINT",
        "TORRENT_FUSE_METADATA_TTL",
        "TORRENT_FUSE_READ_TIMEOUT",
    ];

    // Clean up any existing env vars
    for var in &vars_to_check {
        std::env::remove_var(var);
    }

    // Create config and merge from env - should succeed with defaults
    let config = Config::default();
    let result = config.merge_from_env();

    assert!(
        result.is_ok(),
        "Missing env vars should not cause error, defaults should be used"
    );

    // Verify defaults are used
    let merged = result.unwrap();
    assert_eq!(
        merged.api.url, "http://127.0.0.1:3030",
        "Default API URL should be used"
    );
    assert_eq!(
        merged.cache.metadata_ttl, 60,
        "Default metadata TTL should be used"
    );
    assert_eq!(
        merged.performance.read_timeout, 30,
        "Default read timeout should be used"
    );
}

#[test]
fn test_edge_057_empty_string_env_var_value() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test empty string values for various env vars
    let test_cases = [
        ("TORRENT_FUSE_API_URL", "API URL"),
        ("TORRENT_FUSE_MOUNT_POINT", "mount point"),
        ("TORRENT_FUSE_LOG_LEVEL", "log level"),
        ("TORRENT_FUSE_AUTH_USERPASS", "auth credentials"),
    ];

    for (var, description) in &test_cases {
        std::env::set_var(var, "");

        let config = Config::default();
        let result = config.merge_from_env();

        // Clean up immediately
        std::env::remove_var(var);

        // Empty strings should be handled gracefully
        assert!(
            result.is_ok(),
            "Empty string for {} should not cause panic",
            description
        );

        // For string fields, empty should override default
        let merged = result.unwrap();
        match *var {
            "TORRENT_FUSE_API_URL" => assert_eq!(merged.api.url, "", "Empty API URL should be set"),
            "TORRENT_FUSE_MOUNT_POINT" => assert_eq!(
                merged.mount.mount_point,
                std::path::PathBuf::from(""),
                "Empty mount point should be set"
            ),
            "TORRENT_FUSE_LOG_LEVEL" => {
                assert_eq!(merged.logging.level, "", "Empty log level should be set")
            }
            _ => {}
        }
    }
}

#[test]
fn test_edge_057_very_long_env_var_value() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test very long env var values (>4096 chars)
    let long_value = "a".repeat(5000);

    // Test with API URL (string field)
    std::env::set_var("TORRENT_FUSE_API_URL", &long_value);

    let config = Config::default();
    let result = config.merge_from_env();

    // Clean up
    std::env::remove_var("TORRENT_FUSE_API_URL");

    assert!(
        result.is_ok(),
        "Very long env var value should not cause error"
    );

    let merged = result.unwrap();
    assert_eq!(merged.api.url.len(), 5000, "Long value should be preserved");
    assert!(
        merged.api.url.starts_with("aaaa"),
        "Long value content should be correct"
    );

    // Test validation should fail for invalid URL (too long to be valid)
    assert!(
        merged.validate().is_err(),
        "Validation should fail for extremely long URL"
    );
}

#[test]
fn test_edge_057_empty_numeric_env_var_values() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test empty strings for numeric fields
    let test_cases = [
        ("TORRENT_FUSE_METADATA_TTL", "metadata TTL"),
        ("TORRENT_FUSE_READ_TIMEOUT", "read timeout"),
        ("TORRENT_FUSE_MAX_ENTRIES", "max entries"),
    ];

    for (var, description) in &test_cases {
        std::env::set_var(var, "");

        let config = Config::default();
        let result = config.merge_from_env();

        // Clean up
        std::env::remove_var(var);

        // Empty strings for numeric fields should fail to parse
        assert!(
            result.is_err(),
            "Empty string for numeric field {} should fail to parse",
            description
        );
    }
}

#[test]
fn test_edge_057_whitespace_only_env_var_values() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test whitespace-only values
    let test_cases = [
        ("TORRENT_FUSE_API_URL", "   ", "API URL"),
        ("TORRENT_FUSE_LOG_LEVEL", "\t\n", "log level"),
        ("TORRENT_FUSE_METADATA_TTL", "  ", "metadata TTL"),
    ];

    for (var, value, _description) in &test_cases {
        std::env::set_var(var, value);

        let config = Config::default();
        let result = config.merge_from_env();

        // Clean up
        std::env::remove_var(var);

        // Should not panic
        match result {
            Ok(merged) => {
                // For string fields, whitespace should be preserved
                if *var == "TORRENT_FUSE_API_URL" {
                    assert_eq!(
                        merged.api.url, *value,
                        "Whitespace API URL should be preserved"
                    );
                }
            }
            Err(_) => {
                // Numeric fields should fail to parse whitespace
                if *var == "TORRENT_FUSE_METADATA_TTL" {
                    // This is expected
                }
            }
        }
    }
}

#[test]
fn test_edge_057_env_var_case_sensitivity() {
    let _guard = ENV_VAR_MUTEX.lock().unwrap();

    // Test that env var names are case-sensitive
    std::env::set_var("torrent_fuse_api_url", "http://lowercase:8080");
    std::env::set_var("TORRENT_FUSE_API_URL", "http://uppercase:9090");

    let config = Config::default();
    let result = config.merge_from_env();

    // Clean up
    std::env::remove_var("torrent_fuse_api_url");
    std::env::remove_var("TORRENT_FUSE_API_URL");

    assert!(result.is_ok());
    let merged = result.unwrap();

    // Should use uppercase version (standard convention)
    assert_eq!(
        merged.api.url, "http://uppercase:9090",
        "Uppercase env var should be used"
    );
}
