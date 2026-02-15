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
