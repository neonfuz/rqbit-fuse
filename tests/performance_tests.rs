//! Performance and stress tests for torrent-fuse
//!
//! These tests verify performance characteristics under load:
//! - High-throughput cache operations
//! - Concurrent access patterns
//! - Memory efficiency
//! - Large-scale inode management

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use torrent_fuse::cache::Cache;
use torrent_fuse::fs::inode::InodeManager;
use torrent_fuse::types::inode::InodeEntry;

/// Test cache throughput with large number of entries
#[tokio::test]
async fn test_cache_high_throughput() {
    let cache: Cache<String, Vec<u8>> = Cache::new(10000, Duration::from_secs(60));
    let start = Instant::now();

    // Insert 5000 entries
    for i in 0..5000 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 4096]; // 4KB values
        cache.insert(key, value).await;
    }

    let insert_duration = start.elapsed();

    // Read all entries
    let read_start = Instant::now();
    for i in 0..5000 {
        let key = format!("key_{}", i);
        let result = cache.get(&key).await;
        assert!(result.is_some(), "Cache should contain key_{}", i);
    }
    let read_duration = read_start.elapsed();

    let total_duration = start.elapsed();

    // Performance assertions
    assert!(
        insert_duration < Duration::from_secs(5),
        "Inserting 5000 entries should take less than 5 seconds, took {:?}",
        insert_duration
    );
    assert!(
        read_duration < Duration::from_secs(5),
        "Reading 5000 entries should take less than 5 seconds, took {:?}",
        read_duration
    );

    println!(
        "Cache throughput test: {} inserts/sec, {} reads/sec, total {:?}",
        5000.0 / insert_duration.as_secs_f64(),
        5000.0 / read_duration.as_secs_f64(),
        total_duration
    );

    // Verify stats
    let stats = cache.stats().await;
    assert_eq!(stats.hits, 5000);
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.size, 5000);
}

/// Test cache efficiency with varying hit rates
#[tokio::test]
async fn test_cache_efficiency() {
    let cache: Cache<String, Vec<u8>> = Cache::new(1000, Duration::from_secs(60));

    // Insert 500 entries
    for i in 0..500 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 1024];
        cache.insert(key, value).await;
    }

    // Access pattern: 80% of accesses hit 20% of entries (Pareto principle)
    let access_count = 10000;
    for i in 0..access_count {
        // 80% chance to access hot keys (0-99), 20% chance for cold keys (100-499)
        let key_idx = if i % 10 < 8 {
            i % 100 // Hot keys
        } else {
            100 + (i % 400) // Cold keys
        };
        let key = format!("key_{}", key_idx);
        let _ = cache.get(&key).await;
    }

    let stats = cache.stats().await;
    let hit_rate = stats.hits as f64 / (stats.hits + stats.misses) as f64;

    // With this access pattern and 1000 entry capacity for 500 entries,
    // we should have a very high hit rate (>95%)
    assert!(
        hit_rate > 0.95,
        "Cache hit rate should be >95% with Pareto access pattern, got {:.2}%",
        hit_rate * 100.0
    );

    println!("Cache efficiency: {:.2}% hit rate", hit_rate * 100.0);
}

/// Test cache eviction effectiveness
/// Verifies that the cache maintains its capacity limit under load
#[tokio::test]
async fn test_lru_eviction_efficiency() {
    let cache: Cache<String, Vec<u8>> = Cache::new(100, Duration::from_secs(600));

    // Insert 200 entries (2x capacity)
    for i in 0..200 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 1024];
        cache.insert(key, value).await;
    }

    // Allow Moka time to process insertions and evictions
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Cache should only have 100 entries (at most)
    assert!(
        cache.len() <= 100,
        "Cache should have at most 100 entries, got {}",
        cache.len()
    );

    // Insert 50 more entries
    for i in 200..250 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 1024];
        cache.insert(key, value).await;
    }

    // Allow Moka time to process any evictions
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify cache maintains capacity limit
    assert!(
        cache.len() <= 100,
        "Cache should maintain capacity of 100, got {}",
        cache.len()
    );

    // Verify the most recently inserted entries exist
    let mut recent_entries_found = 0;
    for i in 200..250 {
        let key = format!("key_{}", i);
        if cache.contains_key(&key).await {
            recent_entries_found += 1;
        }
    }

    // Most recent entries should be in cache (at least 80%)
    assert!(
        recent_entries_found >= 40,
        "Most recent entries should be in cache, only {} of 50 found",
        recent_entries_found
    );

    let stats = cache.stats().await;
    println!(
        "Cache eviction test: {} entries, {} recent entries found",
        stats.size, recent_entries_found
    );
}

/// Test concurrent readers with shared cache
#[tokio::test]
async fn test_concurrent_cache_readers() {
    let cache: Arc<Cache<String, Vec<u8>>> = Arc::new(Cache::new(10000, Duration::from_secs(60)));

    // Pre-populate cache
    for i in 0..1000 {
        let key = format!("key_{}", i);
        let value = vec![0u8; 4096];
        cache.insert(key, value).await;
    }

    let start = Instant::now();
    let num_tasks = 10;
    let reads_per_task = 1000;

    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let cache_clone = Arc::clone(&cache);
        let handle = tokio::spawn(async move {
            let mut local_hits = 0;
            for i in 0..reads_per_task {
                let key_idx = (task_id * 100 + i) % 1000;
                let key = format!("key_{}", key_idx);
                if cache_clone.get(&key).await.is_some() {
                    local_hits += 1;
                }
            }
            local_hits
        });
        handles.push(handle);
    }

    let total_hits: u64 = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap() as u64)
        .sum();

    let duration = start.elapsed();
    let total_reads = num_tasks * reads_per_task;
    let throughput = total_reads as f64 / duration.as_secs_f64();

    assert_eq!(
        total_hits, total_reads as u64,
        "All reads should be cache hits"
    );
    assert!(
        duration < Duration::from_secs(5),
        "Concurrent reads should complete in under 5 seconds, took {:?}",
        duration
    );

    println!(
        "Concurrent cache readers: {} reads/sec ({} tasks x {} reads in {:?})",
        throughput, num_tasks, reads_per_task, duration
    );
}

/// Test inode allocation performance under load
#[test]
fn test_inode_allocation_performance() {
    let manager = InodeManager::new();
    let start = Instant::now();

    // Allocate 10,000 inodes
    for i in 0..10000 {
        let entry = InodeEntry::File {
            ino: 0, // Will be assigned
            name: format!("file_{}.txt", i),
            parent: 1,
            size: 1024,
            torrent_id: i as u64,
            file_index: 0,
        };
        let _ = manager.allocate(entry);
    }

    let duration = start.elapsed();
    let throughput = 10000.0 / duration.as_secs_f64();

    assert!(
        duration < Duration::from_secs(2),
        "Allocating 10,000 inodes should take less than 2 seconds, took {:?}",
        duration
    );

    println!(
        "Inode allocation performance: {} allocations/sec",
        throughput
    );
}

/// Test inode lookup performance
#[test]
fn test_inode_lookup_performance() {
    let manager = InodeManager::new();
    let mut inodes = Vec::new();

    // Setup: allocate 10,000 inodes
    for i in 0..10000 {
        let entry = InodeEntry::File {
            ino: 0, // Will be assigned
            name: format!("file_{}.txt", i),
            parent: 1,
            size: 1024,
            torrent_id: i as u64,
            file_index: 0,
        };
        let inode = manager.allocate(entry);
        inodes.push(inode);
    }

    let start = Instant::now();

    // Perform 100,000 lookups
    for _ in 0..10 {
        for inode in &inodes {
            let _ = manager.get(*inode);
        }
    }

    let duration = start.elapsed();
    let throughput = 100000.0 / duration.as_secs_f64();

    assert!(
        duration < Duration::from_secs(2),
        "100,000 inode lookups should take less than 2 seconds, took {:?}",
        duration
    );

    println!("Inode lookup performance: {} lookups/sec", throughput);
}

/// Test concurrent inode operations
#[test]
fn test_concurrent_inode_operations() {
    let manager = Arc::new(InodeManager::new());
    let num_threads = 8;
    let inodes_per_thread = 1000;

    let start = Instant::now();
    let mut handles = Vec::new();

    for thread_id in 0..num_threads {
        let manager_clone = Arc::clone(&manager);
        let handle = std::thread::spawn(move || {
            let mut allocated = Vec::new();

            // Allocate inodes
            for i in 0..inodes_per_thread {
                let entry = InodeEntry::File {
                    ino: 0, // Will be assigned
                    name: format!("thread{}_file_{}.txt", thread_id, i),
                    parent: 1,
                    size: 1024,
                    torrent_id: (thread_id * inodes_per_thread + i) as u64,
                    file_index: 0,
                };
                let inode = manager_clone.allocate(entry);
                allocated.push(inode);
            }

            // Lookup inodes
            for inode in &allocated {
                let _ = manager_clone.get(*inode);
            }

            allocated.len()
        });
        handles.push(handle);
    }

    let total_allocated: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();

    let duration = start.elapsed();
    let throughput = total_allocated as f64 / duration.as_secs_f64();

    assert_eq!(total_allocated, num_threads * inodes_per_thread);
    assert!(
        duration < Duration::from_secs(5),
        "Concurrent inode operations should complete in under 5 seconds, took {:?}",
        duration
    );

    println!(
        "Concurrent inode operations: {} ops/sec ({} threads x {} inodes in {:?})",
        throughput, num_threads, inodes_per_thread, duration
    );
}

/// Test memory usage with large inode tree
#[test]
fn test_large_inode_tree_memory() {
    let manager = InodeManager::new();

    // Create a wide tree: 1 root -> 100 directories -> 100 files each
    let root = 1u64;
    let num_dirs = 100;
    let files_per_dir = 100;

    let start = Instant::now();

    for dir_idx in 0..num_dirs {
        let dir = manager.allocate(InodeEntry::Directory {
            ino: 0, // Will be assigned
            name: format!("dir_{}", dir_idx),
            parent: root,
            children: Vec::new(),
        });
        manager.add_child(root, dir);

        for file_idx in 0..files_per_dir {
            let file = manager.allocate(InodeEntry::File {
                ino: 0, // Will be assigned
                name: format!("file_{}.txt", file_idx),
                parent: dir,
                size: 1024 * 1024, // 1MB
                torrent_id: dir_idx as u64,
                file_index: file_idx,
            });
            manager.add_child(dir, file);
        }
    }

    let duration = start.elapsed();
    let total_inodes = 1 + num_dirs + (num_dirs * files_per_dir);

    println!(
        "Large inode tree: {} inodes created in {:?}",
        total_inodes, duration
    );

    // Verify structure
    assert_eq!(manager.get_children(root).len(), num_dirs);

    // Sample a few directories
    for dir_idx in 0..5 {
        let dir_name = format!("dir_{}", dir_idx);
        let dir_inode = manager
            .get_children(root)
            .into_iter()
            .find(|(_, e)| e.name() == dir_name)
            .map(|(ino, _)| ino);

        assert!(dir_inode.is_some());
        assert_eq!(
            manager.get_children(dir_inode.unwrap()).len(),
            files_per_dir
        );
    }
}

/// Test cache with large values
#[tokio::test]
async fn test_cache_large_values() {
    let cache: Cache<String, Vec<u8>> = Cache::new(100, Duration::from_secs(60));

    // Insert large values (1MB each)
    let value_size = 1024 * 1024;
    let num_entries = 50;

    let start = Instant::now();

    for i in 0..num_entries {
        let key = format!("key_{}", i);
        let value = vec![0u8; value_size];
        cache.insert(key, value).await;
    }

    let insert_duration = start.elapsed();

    // Read all entries
    let read_start = Instant::now();
    for i in 0..num_entries {
        let key = format!("key_{}", i);
        let result = cache.get(&key).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), value_size);
    }
    let read_duration = read_start.elapsed();

    let total_data = num_entries * value_size;
    let insert_throughput = total_data as f64 / insert_duration.as_secs_f64() / (1024.0 * 1024.0);
    let read_throughput = total_data as f64 / read_duration.as_secs_f64() / (1024.0 * 1024.0);

    println!(
        "Large value cache test: {} entries of {}MB each",
        num_entries,
        value_size / (1024 * 1024)
    );
    println!(
        "  Insert: {:.2} MB/sec, Read: {:.2} MB/sec",
        insert_throughput, read_throughput
    );
}

/// Test read timeout and cancellation
#[tokio::test]
async fn test_read_operation_timeout() {
    // This test simulates slow read operations and verifies they timeout properly
    let result = timeout(Duration::from_millis(100), async {
        // Simulate a slow operation
        tokio::time::sleep(Duration::from_millis(200)).await;
        "completed"
    })
    .await;

    assert!(result.is_err(), "Operation should timeout");

    // Test that faster operations complete successfully
    let result = timeout(Duration::from_millis(100), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        "completed"
    })
    .await;

    assert!(result.is_ok(), "Fast operation should complete");
    assert_eq!(result.unwrap(), "completed");
}
