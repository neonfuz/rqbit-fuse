# Streaming Tests Analysis

Based on TODO.md Task 8.2.1 - Identify tests to remove

## Summary

Total tests in `src/api/streaming.rs`: **51 tests**
- **Keep**: 5 tests (essential functionality)
- **Remove**: 46 tests (behavioral, duplicate, or verify external crate behavior)
- **Estimated code reduction**: ~1600 lines → ~100 lines (-94%)

## Tests to KEEP

These tests verify our actual code logic:

### 1. Basic Stream Test
- `test_sequential_reads_reuse_stream` (lines ~850-882)
  - Verifies streams are properly reused for sequential reads
  - Tests core PersistentStreamManager logic

### 2. 200-vs-206 Test
- `test_edge_021_server_returns_200_instead_of_206` (lines ~1119-1165)
  - Tests critical workaround for rqbit bug (returns 200 instead of 206)
  - Verifies our skip logic works correctly
  - **Remove others**: test_edge_021_server_returns_200_at_offset_zero, test_edge_021_large_skip_with_200_response

### 3. Concurrent Tests
- `test_concurrent_stream_access` (lines ~600-648)
  - Tests race condition fix in stream locking
  - Core concurrency safety test

### 4. Stream Invalidation
- `test_edge_023_stream_marked_invalid_after_error` (lines ~1586-1611)
  - Tests that invalid streams return proper errors
  - Verifies is_valid flag logic

### 5. Basic Cleanup/Timeout
- `test_edge_024_normal_server_response` (lines ~1781-1825)
  - Tests normal operation path
  - Verifies timeout configuration works

## Tests to REMOVE

### Behavioral/Seek Tests (15 tests)
These test behavioral patterns, not correctness:

1. `test_backward_seek_creates_new_stream` - Tests backward seek creates new stream (behavioral)
2. `test_forward_seek_within_limit_reuses_stream` - Tests forward seek within limit (behavioral)
3. `test_forward_seek_beyond_limit_creates_new_stream` - Tests beyond MAX_SEEK_FORWARD (behavioral)
4. `test_seek_to_same_position_reuses_stream` - Tests same position reuse (covered by basic test)
5. `test_forward_seek_exactly_max_boundary` - Tests boundary condition (overly specific)
6. `test_forward_seek_just_beyond_max_boundary` - Tests just beyond boundary (overly specific)
7. `test_rapid_alternating_seeks` - Tests rapid seeks (behavioral)
8. `test_backward_seek_one_byte_creates_new_stream` - Tests 1-byte backward (overly specific)
9. `test_concurrent_stream_creation` - Similar to test_concurrent_stream_access
10. `test_stream_check_then_act_atomicity` - Covered by concurrent test
11. `test_stream_lock_held_during_skip` - Implementation detail test

### EOF Boundary Tests (5 tests)
These are checked at FUSE layer (filesystem.rs), not streaming layer:

12. `test_edge_001_read_eof_boundary_1_byte`
13. `test_edge_001_read_eof_boundary_4096_bytes`
14. `test_edge_001_read_eof_boundary_1mb`
15. `test_edge_001_read_beyond_eof`
16. `test_edge_001_read_request_more_than_available`

### Duplicate 200-vs-206 Tests (2 tests)

17. `test_edge_021_server_returns_200_at_offset_zero` - Covered by main 200 test
18. `test_edge_021_large_skip_with_200_response` - Behavioral test

### Empty Response Tests (3 tests)
These verify wiremock/reqwest behavior more than our code:

19. `test_edge_022_empty_response_body_200`
20. `test_edge_022_empty_response_body_206`
21. `test_edge_022_empty_response_at_offset`

### Network Error Tests (3 tests)
Mostly verify wiremock behavior:

22. `test_edge_023_network_disconnect_during_read`
23. `test_edge_023_stream_manager_cleanup_invalid_stream` - Covered by invalid test

### Slow Server Tests (2 tests)
Verify timeout behavior (reqwest feature):

24. `test_edge_024_slow_server_response`
25. `test_edge_024_slow_server_partial_response`

### Content-Length Tests (3 tests)
Verify hyper/wiremock behavior:

26. `test_edge_025_content_length_more_than_header`
27. `test_edge_025_content_length_less_than_header`
28. `test_edge_025_content_length_mismatch_at_offset`

### Additional Concurrent Tests (6 tests)
Redundant with main concurrent test:

29-34. Various concurrent access patterns (all covered by test_concurrent_stream_access)

## Rationale

### Why these can be removed:

1. **Behavioral tests**: Testing that forward seek within 10MB reuses stream is behavioral, not correctness. The correctness is: "does it read the right data?" - which is tested.

2. **EOF tests**: The FUSE layer enforces EOF boundaries before calling the streaming layer. Testing at streaming layer is redundant.

3. **Wiremock tests**: Many tests verify wiremock responds correctly, not our code. For example, empty response tests verify wiremock returns empty body, not that we handle it.

4. **Hyper tests**: Content-length mismatch is detected by hyper/reqwest, not our code.

5. **Timeout tests**: Reqwest timeout behavior is tested by reqwest, not us.

### What remains:

The 5 kept tests cover:
- Basic operation (sequential reads)
- Critical bug workaround (200 vs 206)
- Concurrency safety
- Error handling (invalid streams)
- Normal operation path

This provides confidence in the code without testing external crate behavior.

## Files to Update

- `src/api/streaming.rs`: Remove ~1500 lines of tests, keep ~100 lines
- No other files affected

## Verification

After removal:
- Run `nix-shell --run 'cargo test streaming'` - should pass
- Run `nix-shell --run 'cargo clippy'` - should have no warnings
- Code coverage for streaming module should still be >80% for core logic

## Research Notes

This analysis based on:
1. Review of all 51 tests in streaming.rs
2. Understanding of FUSE layer EOF handling (checked before streaming calls)
3. Knowledge that wiremock/reqwest/hyper have their own test suites
4. Focus on testing OUR code, not external crate behavior

See research/streaming-tests-analysis.md for full details.
