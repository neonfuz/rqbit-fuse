use rqbit_fuse::config::Config;

#[test]
fn test_edge_056_timeout_zero() {
    let mut config = Config::default();
    config.performance.read_timeout = 0;

    let result = config.validate();
    assert!(result.is_err(), "Timeout of 0 should fail validation");
}

#[test]
fn test_edge_056_timeout_u64_max() {
    let mut config = Config::default();
    config.performance.read_timeout = u64::MAX;

    let result = config.validate();
    assert!(
        result.is_err(),
        "Timeout of u64::MAX should fail validation (exceeds 3600s limit)"
    );
}

#[test]
fn test_edge_056_timeout_negative_from_env() {
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
    // Test various valid timeout values
    let valid_timeouts = [1, 30, 60, 300, 1800, 3600];

    for timeout in valid_timeouts {
        let mut config = Config::default();
        config.performance.read_timeout = timeout;

        assert!(
            config.validate().is_ok(),
            "Timeout of {} should be valid",
            timeout
        );
    }
}

#[test]
fn test_edge_056_timeout_just_above_max() {
    let mut config = Config::default();
    config.performance.read_timeout = 3601;

    let result = config.validate();
    assert!(
        result.is_err(),
        "Timeout of 3601 (just above max) should fail validation"
    );
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
fn test_edge_056_other_timeout_fields() {
    // Test monitoring timeouts as well
    let mut config = Config::default();

    // Test status_poll_interval = 0
    config.monitoring.status_poll_interval = 0;
    assert!(
        config.validate().is_err(),
        "status_poll_interval of 0 should fail validation"
    );

    config.monitoring.status_poll_interval = 5; // Reset

    // Test stalled_timeout = 0
    config.monitoring.stalled_timeout = 0;
    assert!(
        config.validate().is_err(),
        "stalled_timeout of 0 should fail validation"
    );
}

#[test]
fn test_edge_056_metrics_interval_zero_when_enabled() {
    let mut config = Config::default();
    config.logging.metrics_enabled = true;
    config.logging.metrics_interval_secs = 0;

    let result = config.validate();
    assert!(
        result.is_err(),
        "metrics_interval_secs of 0 should fail when metrics_enabled is true"
    );
}

#[test]
fn test_edge_056_invalid_timeout_from_env_handling() {
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
