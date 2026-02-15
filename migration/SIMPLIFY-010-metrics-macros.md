# SIMPLIFY-010: Simplify Metrics with Macros and Traits

## Task ID
**SIMPLIFY-010**

## Scope
- **Primary File**: `src/metrics.rs`
- **Lines to Modify**: 36-80 (FuseMetrics recording methods), 83-91 (avg_read_latency_ms), 145-179 (ApiMetrics recording methods), 181-190 (avg_latency_ms)

## Current State

### Repetitive Recording Methods

The `FuseMetrics` struct contains 7 nearly identical recording methods that increment counters and log:

```rust
// src/metrics.rs:36-76
impl FuseMetrics {
    pub fn record_getattr(&self) {
        self.getattr_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "getattr");
    }

    pub fn record_setattr(&self) {
        self.setattr_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "setattr");
    }

    pub fn record_lookup(&self) {
        self.lookup_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "lookup");
    }

    pub fn record_readdir(&self) {
        self.readdir_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "readdir");
    }

    pub fn record_open(&self) {
        self.open_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "open");
    }

    pub fn record_release(&self) {
        self.release_count.fetch_add(1, Ordering::Relaxed);
        trace!(fuse_op = "release");
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }
}
```

### Duplicated Average Latency Calculations

Both `FuseMetrics` and `ApiMetrics` have identical average latency calculation logic:

```rust
// src/metrics.rs:83-91 (FuseMetrics)
pub fn avg_read_latency_ms(&self) -> f64 {
    let count = self.read_count.load(Ordering::Relaxed);
    if count == 0 {
        0.0
    } else {
        let total_ns = self.read_latency_ns.load(Ordering::Relaxed);
        (total_ns as f64 / count as f64) / 1_000_000.0
    }
}

// src/metrics.rs:182-190 (ApiMetrics)
pub fn avg_latency_ms(&self) -> f64 {
    let count = self.success_count.load(Ordering::Relaxed);
    if count == 0 {
        0.0
    } else {
        let total_ns = self.total_latency_ns.load(Ordering::Relaxed);
        (total_ns as f64 / count as f64) / 1_000_000.0
    }
}
```

## Target State

### 1. `record_op!` Macro

Create a declarative macro to generate the simple recording methods:

```rust
/// Macro to generate simple operation recording methods
/// 
/// Generates methods that:
/// - Increment a counter field
/// - Emit a trace log with the operation name
macro_rules! record_op {
    // Variant with trace logging
    ($method:ident, $field:ident, $op_name:expr) => {
        pub fn $method(&self) {
            self.$field.fetch_add(1, Ordering::Relaxed);
            trace!(fuse_op = $op_name);
        }
    };
    // Variant without trace logging
    ($method:ident, $field:ident) => {
        pub fn $method(&self) {
            self.$field.fetch_add(1, Ordering::Relaxed);
        }
    };
}
```

### 2. `LatencyMetrics` Trait

Create a trait to unify average latency calculations:

```rust
/// Trait for metrics that track latency
/// 
/// Implementors must provide:
/// - count(): The number of operations
/// - total_latency_ns(): Total latency in nanoseconds
pub trait LatencyMetrics {
    /// Get the count of operations
    fn count(&self) -> u64;
    /// Get total latency in nanoseconds
    fn total_latency_ns(&self) -> u64;

    /// Calculate average latency in milliseconds
    fn avg_latency_ms(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            0.0
        } else {
            let total_ns = self.total_latency_ns();
            (total_ns as f64 / count as f64) / 1_000_000.0
        }
    }
}

// Implement for FuseMetrics
impl LatencyMetrics for FuseMetrics {
    fn count(&self) -> u64 {
        self.read_count.load(Ordering::Relaxed)
    }

    fn total_latency_ns(&self) -> u64 {
        self.read_latency_ns.load(Ordering::Relaxed)
    }
}

// Implement for ApiMetrics  
impl LatencyMetrics for ApiMetrics {
    fn count(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }

    fn total_latency_ns(&self) -> u64 {
        self.total_latency_ns.load(Ordering::Relaxed)
    }
}
```

### 3. Simplified FuseMetrics Implementation

```rust
impl FuseMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    // Generate simple recording methods using macro
    record_op!(record_getattr, getattr_count, "getattr");
    record_op!(record_setattr, setattr_count, "setattr");
    record_op!(record_lookup, lookup_count, "lookup");
    record_op!(record_readdir, readdir_count, "readdir");
    record_op!(record_open, open_count, "open");
    record_op!(record_release, release_count, "release");
    record_op!(record_error, error_count);

    // Complex recording method remains hand-written
    pub fn record_read(&self, bytes: u64, latency: Duration) {
        self.read_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
        self.read_latency_ns
            .fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
        trace!(
            fuse_op = "read",
            bytes_read = bytes,
            latency_ns = latency.as_nanos() as u64
        );
    }

    // avg_read_latency_ms is now provided by LatencyMetrics trait
    // Keep this alias for backward compatibility or remove if not needed
}
```

## Implementation Steps

1. **Add the `record_op!` macro** at the top of `src/metrics.rs` after the imports
   - Define both variants (with and without trace logging)

2. **Add the `LatencyMetrics` trait** after the macro
   - Include trait definition with `avg_latency_ms()` default implementation
   - Add `count()` and `total_latency_ns()` required methods

3. **Implement `LatencyMetrics` for `FuseMetrics`**
   - Map to `read_count` and `read_latency_ns` fields

4. **Implement `LatencyMetrics` for `ApiMetrics`**
   - Map to `success_count` and `total_latency_ns` fields

5. **Replace FuseMetrics recording methods with macro calls**
   - Replace 7 hand-written methods with `record_op!` macro invocations

6. **Remove duplicate `avg_read_latency_ms()` from FuseMetrics**
   - Now provided by trait (or keep as deprecated alias)

7. **Remove duplicate `avg_latency_ms()` from ApiMetrics**
   - Now provided by trait (or keep as deprecated alias)

8. **Update existing callers if method names changed**
   - Verify `log_summary()` still works correctly

9. **Run tests to verify correctness**
   - Ensure all metrics tests pass

10. **Run clippy and fmt**
    - Ensure code meets style guidelines

## Testing

### Verification Steps

1. **Run existing tests**:
   ```bash
   cargo test metrics::tests
   ```
   - Should pass: `test_fuse_metrics`, `test_api_metrics`

2. **Verify macro-generated methods work**:
   ```bash
   cargo test --features metrics_test -- --nocapture
   ```

3. **Verify trait provides correct averages**:
   - Test that `avg_latency_ms()` returns same values as before
   - Test edge case: zero operations returns 0.0

4. **Run full test suite**:
   ```bash
   cargo test
   ```

### Test Coverage

Ensure these scenarios are tested:
- [ ] Simple recording methods increment counters correctly
- [ ] Trace logs are emitted for appropriate operations
- [ ] Average latency calculation is correct
- [ ] Zero operations returns 0.0 (not NaN or panic)
- [ ] Metrics summary logging still works

## Expected Reduction

- **Before**: ~45 lines (7 recording methods × 4 lines + 2 avg methods × 8 lines + boilerplate)
- **After**: ~30 lines (macro definition + trait + impls + macro invocations)
- **Net Reduction**: ~35 lines of repetitive code

## Completion Checklist

- [ ] `record_op!` macro created and working
- [ ] `LatencyMetrics` trait created with default implementation
- [ ] `LatencyMetrics` implemented for `FuseMetrics`
- [ ] `LatencyMetrics` implemented for `ApiMetrics`
- [ ] 7 recording methods replaced with macro calls
- [ ] Duplicate avg methods removed or deprecated
- [ ] All existing tests pass
- [ ] `cargo clippy` passes without warnings
- [ ] `cargo fmt` applied
- [ ] Migration guide marked complete

---

*Created from code review - February 14, 2026*
