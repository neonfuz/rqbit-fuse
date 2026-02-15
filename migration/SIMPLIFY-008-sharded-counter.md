# Migration Guide: SIMPLIFY-008 - Extract ShardedCounter to lib/

## Task ID
**SIMPLIFY-008**

## Scope

### Files to Modify
- `src/cache.rs` - Remove ShardedCounter implementation (~43 lines)
- `src/lib/sharded_counter.rs` - Create new file with extracted implementation
- `src/lib.rs` - Add module declaration and re-export

## Current State

### ShardedCounter in cache.rs (lines 7-55)

```rust
/// Number of shards for statistics counters.
/// Using 64 shards provides good concurrency reduction while keeping memory overhead low.
/// Each shard is ~16 bytes (2 AtomicU64s), so 64 shards = 1KB per cache instance.
const STATS_SHARDS: usize = 64;

/// Sharded counter to reduce contention under high concurrency.
/// Uses a thread-local counter to select shards, avoiding atomic contention
/// while working correctly in async contexts where tasks migrate between threads.
#[derive(Debug)]
struct ShardedCounter {
    shards: Vec<AtomicU64>,
}

impl ShardedCounter {
    fn new() -> Self {
        let mut shards = Vec::with_capacity(STATS_SHARDS);
        for _ in 0..STATS_SHARDS {
            shards.push(AtomicU64::new(0));
        }
        Self { shards }
    }

    /// Increment a counter shard using round-robin selection via thread-local counter.
    /// This avoids contention better than a single atomic while working in async contexts.
    #[inline]
    fn increment(&self) {
        // Use a thread-local counter for shard selection
        // This works in async contexts because we only need distribution, not thread affinity
        thread_local! {
            static COUNTER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        }

        let shard_idx = COUNTER.with(|c| {
            let val = c.get();
            c.set(val.wrapping_add(1));
            (val as usize) % STATS_SHARDS
        });

        self.shards[shard_idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Sum all shards to get the total count.
    fn sum(&self) -> u64 {
        self.shards
            .iter()
            .map(|shard| shard.load(Ordering::Relaxed))
            .sum()
    }
}
```

### Current cache.rs imports (lines 1-5)

```rust
use moka::future::Cache as MokaCache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::trace;
```

### Current usage in Cache struct (lines 79-88)

```rust
pub struct Cache<K, V> {
    /// The underlying moka cache
    inner: MokaCache<K, Arc<V>>,
    /// Sharded hit counter for reduced contention
    hits: ShardedCounter,
    /// Sharded miss counter for reduced contention
    misses: ShardedCounter,
    /// Default TTL for entries
    default_ttl: Duration,
}
```

## Target State

### New file: src/lib/sharded_counter.rs

```rust
//! Sharded counter for high-concurrency statistics collection.
//!
//! This module provides a sharded counter implementation that reduces
//! contention under high concurrency by distributing increments across
//! multiple atomic counters.

use std::sync::atomic::{AtomicU64, Ordering};

/// Number of shards for statistics counters.
/// Using 64 shards provides good concurrency reduction while keeping memory overhead low.
/// Each shard is ~16 bytes (2 AtomicU64s), so 64 shards = 1KB per cache instance.
const STATS_SHARDS: usize = 64;

/// Sharded counter to reduce contention under high concurrency.
/// Uses a thread-local counter to select shards, avoiding atomic contention
/// while working correctly in async contexts where tasks migrate between threads.
#[derive(Debug)]
pub struct ShardedCounter {
    shards: Vec<AtomicU64>,
}

impl ShardedCounter {
    /// Create a new sharded counter with all shards initialized to 0.
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(STATS_SHARDS);
        for _ in 0..STATS_SHARDS {
            shards.push(AtomicU64::new(0));
        }
        Self { shards }
    }

    /// Increment a counter shard using round-robin selection via thread-local counter.
    /// This avoids contention better than a single atomic while working in async contexts.
    #[inline]
    pub fn increment(&self) {
        // Use a thread-local counter for shard selection
        // This works in async contexts because we only need distribution, not thread affinity
        thread_local! {
            static COUNTER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        }

        let shard_idx = COUNTER.with(|c| {
            let val = c.get();
            c.set(val.wrapping_add(1));
            (val as usize) % STATS_SHARDS
        });

        self.shards[shard_idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Sum all shards to get the total count.
    pub fn sum(&self) -> u64 {
        self.shards
            .iter()
            .map(|shard| shard.load(Ordering::Relaxed))
            .sum()
    }
}

impl Default for ShardedCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharded_counter_basic() {
        let counter = ShardedCounter::new();
        
        counter.increment();
        counter.increment();
        counter.increment();
        
        assert_eq!(counter.sum(), 3);
    }

    #[test]
    fn test_sharded_counter_concurrent() {
        use std::sync::Arc;
        use std::thread;
        
        let counter = Arc::new(ShardedCounter::new());
        let mut handles = vec![];
        
        // Spawn 10 threads, each incrementing 1000 times
        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    counter.increment();
                }
            }));
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Verify total count
        assert_eq!(counter.sum(), 10_000);
    }

    #[test]
    fn test_sharded_counter_default() {
        let counter: ShardedCounter = Default::default();
        assert_eq!(counter.sum(), 0);
        
        counter.increment();
        assert_eq!(counter.sum(), 1);
    }
}
```

### Updated src/lib.rs

Add module declaration after line 6:

```rust
pub mod sharded_counter;
```

Add re-export after line 8:

```rust
pub use sharded_counter::ShardedCounter;
```

### Updated src/cache.rs

Remove lines 7-55 (ShardedCounter implementation) and add import:

```rust
use crate::sharded_counter::ShardedCounter;
```

Update the struct field visibility (lines 83-85) - no change needed, stays private.

## Implementation Steps

1. **Create the new module file**
   - Create `src/lib/sharded_counter.rs`
   - Copy the ShardedCounter implementation from cache.rs
   - Add module-level documentation
   - Add `pub` visibility to struct and methods
   - Implement `Default` trait
   - Add unit tests

2. **Update src/lib.rs**
   - Add `pub mod sharded_counter;` declaration
   - Add `pub use sharded_counter::ShardedCounter;` re-export

3. **Update src/cache.rs**
   - Remove the `STATS_SHARDS` constant (line 10)
   - Remove the `ShardedCounter` struct and impl (lines 16-55)
   - Add import: `use crate::sharded_counter::ShardedCounter;`
   - Remove `AtomicU64` and `Ordering` from imports (no longer needed in cache.rs)

4. **Verify compilation**
   - Run `cargo check` to ensure no compilation errors
   - Fix any visibility or import issues

5. **Run tests**
   - Execute `cargo test cache::tests` to verify cache functionality
   - Execute `cargo test sharded_counter::tests` to verify new module tests
   - All tests should pass without modification

## Testing

### Test Commands

```bash
# Check compilation
cargo check

# Run cache tests (should pass unchanged)
cargo test cache::tests

# Run new sharded_counter tests
cargo test sharded_counter::tests

# Run all tests
cargo test

# Run linting
cargo clippy

# Format code
cargo fmt
```

### Expected Test Results

- All existing cache tests pass without modification
- New sharded_counter tests pass:
  - `test_sharded_counter_basic` - Single-threaded increment/sum
  - `test_sharded_counter_concurrent` - Multi-threaded increments
  - `test_sharded_counter_default` - Default trait implementation

### Verification Checklist

- [ ] `cargo check` passes with no errors
- [ ] `cargo test cache::tests` passes (8 tests)
- [ ] `cargo test sharded_counter::tests` passes (3 tests)
- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo fmt` makes no changes

## Expected Reduction

### Lines Removed from cache.rs

- `STATS_SHARDS` constant: 4 lines (including comment)
- `ShardedCounter` struct definition: 3 lines (including comment)
- `impl ShardedCounter` block: 38 lines

**Total: ~43 lines removed from cache.rs**

### Lines Added

- `src/lib/sharded_counter.rs`: ~180 lines (including tests and documentation)
- Import statement in cache.rs: 1 line
- Module declaration in lib.rs: 1 line
- Re-export in lib.rs: 1 line

**Net change: +~140 lines (but better organized and reusable)**

## Benefits

1. **Reusability**: ShardedCounter can now be used by other modules (e.g., metrics)
2. **Testability**: Dedicated unit tests for the counter logic
3. **Maintainability**: Single responsibility - cache.rs focuses on caching, counter logic is separate
4. **Documentation**: Module-level docs explain the sharding strategy
5. **Future-proof**: Can be extended with additional counter operations if needed

## Dependencies

- None - this is a pure refactoring with no external dependencies

## Notes

- The `STATS_SHARDS` constant should remain private to the sharded_counter module
- Consider making `STATS_SHARDS` configurable in the future via const generic
- The thread-local counter approach is intentional for async compatibility
- All methods should remain `#[inline]` for performance in hot paths
