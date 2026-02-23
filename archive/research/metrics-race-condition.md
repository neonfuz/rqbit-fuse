# Metrics Race Condition Analysis

## Problem

The current metrics implementation has race conditions when calculating averages and rates:

1. **`avg_latency_ms()` in LatencyMetrics trait**: Loads `count()` and `total_latency_ns()` separately. Between these two loads, another thread could update the values, causing an inconsistent average calculation.

2. **`success_rate()` in ApiMetrics**: Same issue - loads `request_count` and `success_count` separately.

3. **`log_summary()` methods**: Multiple atomic values loaded separately, creating inconsistent snapshots.

## Current Implementation

```rust
fn avg_latency_ms(&self) -> f64 {
    let count = self.count();           // Load 1
    if count == 0 {
        0.0
    } else {
        let total_ns = self.total_latency_ns();  // Load 2
        (total_ns as f64 / count as f64) / 1_000_000.0
    }
}
```

The race: Between "Load 1" and "Load 2", another thread could:
- Add more operations, changing both count and total
- This could result in a count that doesn't match the total

## Solutions Considered

### 1. Accept Relaxed Consistency (Current)
Using `Ordering::Relaxed` means we accept some inconsistency. For metrics, this is often acceptable as values are approximate anyway.

### 2. Use Mutex for Reads
Protects reads but defeats the purpose of lock-free atomics. Not recommended for high-throughput metrics.

### 3. Atomic Snapshot Pattern
Load values multiple times and detect inconsistency. If inconsistent, retry.

### 4. Use fetch_add for Recording, Relaxed for Reading
Keep current approach but document the limitation.

## Selected Solution

Use **Atomic Snapshot Pattern** for consistent reads:

```rust
fn avg_latency_ms(&self) -> f64 {
    loop {
        let count = self.count();
        let total_ns = self.total_latency_ns();
        
        // Check if values are consistent (count hasn't changed)
        if count == 0 {
            return 0.0;
        }
        
        // Re-read count to check for consistency
        let new_count = self.count();
        if new_count == count {
            // Consistent snapshot
            return (total_ns as f64 / count as f64) / 1_000_000.0;
        }
        // Values changed, retry for consistent snapshot
    }
}
```

This ensures we get a consistent pair of values, even if it requires retries under high contention.

## Implementation Plan

1. Update `LatencyMetrics::avg_latency_ms()` to use atomic snapshot pattern
2. Update `ApiMetrics::success_rate()` to use atomic snapshot pattern  
3. Update `log_summary()` methods to load values in consistent order
4. Add tests for concurrent access

## References

- [Rust std::sync::atomic documentation](https://doc.rust-lang.org/std/sync/atomic/)
- [Atomic operations memory ordering](https://en.cppreference.com/w/cpp/atomic/memory_order)
