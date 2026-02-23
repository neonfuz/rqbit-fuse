//! Performance benchmarks for rqbit-fuse
//!
//! Run with: cargo bench
//!
//! These benchmarks measure:
//! - Cache throughput and efficiency
//! - Inode management performance
//! - Concurrent read operations
//! - Memory usage patterns

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dashmap::DashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

use rqbit_fuse::cache::Cache;
use rqbit_fuse::fs::inode::{InodeEntry, InodeManager};

/// Create a Tokio runtime for async benchmarks
fn create_runtime() -> Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

/// Benchmark cache operations with varying entry counts
fn bench_cache_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_throughput");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        // Benchmark cache insertions
        group.bench_with_input(BenchmarkId::new("insert", size), size, |b, &size| {
            let rt = create_runtime();
            b.iter(|| {
                rt.block_on(async {
                    let cache: Cache<String, Vec<u8>> =
                        Cache::new(size * 2, Duration::from_secs(60));
                    for i in 0..size {
                        let key = format!("key_{}", i);
                        let value = vec![0u8; 1024]; // 1KB values
                        cache.insert(key, value).await;
                    }
                    black_box(cache);
                });
            });
        });

        // Benchmark cache reads (all hits)
        group.bench_with_input(BenchmarkId::new("read_hit", size), size, |b, &size| {
            let rt = create_runtime();
            let cache: Cache<String, Vec<u8>> = Cache::new(size * 2, Duration::from_secs(60));
            rt.block_on(async {
                for i in 0..size {
                    let key = format!("key_{}", i);
                    let value = vec![0u8; 1024];
                    cache.insert(key, value).await;
                }
            });

            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let key = format!("key_{}", i);
                        let _ = cache.get(&key).await;
                    }
                });
            });
        });

        // Benchmark cache reads (50% hit rate)
        group.bench_with_input(BenchmarkId::new("read_mixed", size), size, |b, &size| {
            let rt = create_runtime();
            let cache: Cache<String, Vec<u8>> = Cache::new(size * 2, Duration::from_secs(60));
            rt.block_on(async {
                // Insert half the keys
                for i in 0..size / 2 {
                    let key = format!("key_{}", i);
                    let value = vec![0u8; 1024];
                    cache.insert(key, value).await;
                }
            });

            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let key = format!("key_{}", i);
                        let _ = cache.get(&key).await;
                    }
                });
            });
        });
    }

    group.finish();
}

/// Benchmark inode management operations
fn bench_inode_management(c: &mut Criterion) {
    let mut group = c.benchmark_group("inode_management");

    // Benchmark inode allocation
    group.bench_function("allocate_inodes", |b| {
        b.iter(|| {
            let manager = InodeManager::new();
            for i in 0..1000 {
                let entry = InodeEntry::File {
                    ino: 0, // Will be assigned
                    name: format!("file_{}.txt", i),
                    parent: 1,
                    size: 1024,
                    torrent_id: i as u64,
                    file_index: 0,
                    canonical_path: format!("/file_{}.txt", i),
                };
                let _ = manager.allocate(entry);
            }
            black_box(manager);
        });
    });

    // Benchmark inode lookup
    group.bench_function("lookup_inodes", |b| {
        let manager = InodeManager::new();
        let mut inodes = Vec::new();

        // Setup: allocate 1000 inodes
        for i in 0..1000 {
            let entry = InodeEntry::File {
                ino: 0, // Will be assigned
                name: format!("file_{}.txt", i),
                parent: 1,
                size: 1024,
                torrent_id: i as u64,
                file_index: 0,
                canonical_path: format!("/file_{}.txt", i),
            };
            let inode = manager.allocate(entry);
            inodes.push(inode);
        }

        b.iter(|| {
            for inode in &inodes {
                let _ = manager.get(*inode);
            }
        });
    });

    // Benchmark parent-child relationship operations
    group.bench_function("parent_child_ops", |b| {
        let manager = InodeManager::new();

        // Setup: create directory structure
        let root = 1u64;
        let dir1 = manager.allocate(InodeEntry::Directory {
            ino: 0, // Will be assigned
            name: "dir1".to_string(),
            parent: root,
            children: DashSet::new(),
            canonical_path: "/dir1".to_string(),
        });
        let dir2 = manager.allocate(InodeEntry::Directory {
            ino: 0, // Will be assigned
            name: "dir2".to_string(),
            parent: root,
            children: DashSet::new(),
            canonical_path: "/dir2".to_string(),
        });

        manager.add_child(root, dir1);
        manager.add_child(root, dir2);

        b.iter(|| {
            // Get children
            let children = manager.get_children(root);
            for (inode, entry) in children {
                // Verify parent relationship
                let _ = entry.parent();
                let _ = manager.get(inode);
            }
        });
    });

    group.finish();
}

/// Benchmark concurrent operations
fn bench_concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_operations");

    for num_threads in [2, 4, 8, 16].iter() {
        let thread_count = *num_threads;

        // Benchmark concurrent cache access
        group.bench_with_input(
            BenchmarkId::new("concurrent_cache_reads", thread_count),
            &thread_count,
            |b, &threads| {
                let rt = create_runtime();
                let cache: Arc<Cache<String, Vec<u8>>> =
                    Arc::new(Cache::new(10000, Duration::from_secs(60)));

                // Pre-populate cache
                rt.block_on(async {
                    for i in 0..1000 {
                        let key = format!("key_{}", i);
                        let value = vec![0u8; 4096]; // 4KB values
                        cache.insert(key, value).await;
                    }
                });

                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();

                        for t in 0..threads {
                            let cache_clone = Arc::clone(&cache);
                            let handle = tokio::spawn(async move {
                                let start = t * 100;
                                let end = start + 100;
                                for i in start..end {
                                    let key = format!("key_{}", i % 1000);
                                    let _ = cache_clone.get(&key).await;
                                }
                            });
                            handles.push(handle);
                        }

                        for handle in handles {
                            let _ = handle.await;
                        }
                    });
                });
            },
        );

        // Benchmark concurrent inode operations
        group.bench_with_input(
            BenchmarkId::new("concurrent_inode_ops", thread_count),
            &thread_count,
            |b, &threads| {
                b.iter(|| {
                    let manager = Arc::new(InodeManager::new());
                    let mut handles = Vec::new();

                    for t in 0..threads {
                        let manager_clone = Arc::clone(&manager);
                        let handle: std::thread::JoinHandle<()> = std::thread::spawn(move || {
                            let start = t * 100;
                            let end = start + 100;
                            for i in start..end {
                                let entry = InodeEntry::File {
                                    ino: 0, // Will be assigned
                                    name: format!("file_{}.txt", i),
                                    parent: 1,
                                    size: 1024,
                                    torrent_id: i as u64,
                                    file_index: 0,
                                    canonical_path: format!("/file_{}.txt", i),
                                };
                                let inode = manager_clone.allocate(entry);
                                let _ = manager_clone.get(inode);
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        let _: () = handle.join().unwrap();
                    }

                    black_box(manager);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");

    // Benchmark cache memory overhead
    group.bench_function("cache_memory_overhead", |b| {
        b.iter(|| {
            let rt = create_runtime();
            let cache: Cache<String, Vec<u8>> = Cache::new(10000, Duration::from_secs(60));

            rt.block_on(async {
                // Insert entries of various sizes
                for i in 0..1000 {
                    let key = format!("key_{}", i);
                    let value_size = if i % 10 == 0 {
                        1024 * 1024 // 1MB for every 10th entry
                    } else {
                        4096 // 4KB for others
                    };
                    let value = vec![0u8; value_size];
                    cache.insert(key, value).await;
                }

                // Access some entries to update LRU
                for i in 0..100 {
                    let key = format!("key_{}", i);
                    let _ = cache.get(&key).await;
                }
            });

            black_box(cache);
        });
    });

    // Benchmark inode manager memory usage
    group.bench_function("inode_manager_memory", |b| {
        b.iter(|| {
            let manager = InodeManager::new();

            // Create a deep directory structure
            let root = 1u64;
            let mut current_dir = root;

            for depth in 0..100 {
                // Create subdirectory
                let subdir = manager.allocate(InodeEntry::Directory {
                    ino: 0, // Will be assigned
                    name: format!("dir_{}", depth),
                    parent: current_dir,
                    children: DashSet::new(),
                    canonical_path: format!("/dir_{}", depth),
                });
                manager.add_child(current_dir, subdir);

                // Add some files to each directory
                for file_idx in 0..10 {
                    let file = manager.allocate(InodeEntry::File {
                        ino: 0, // Will be assigned
                        name: format!("file_{}.txt", file_idx),
                        parent: current_dir,
                        size: 1024 * 1024, // 1MB
                        torrent_id: depth as u64,
                        file_index: file_idx,
                        canonical_path: format!("/dir_{}/file_{}.txt", depth, file_idx),
                    });
                    manager.add_child(current_dir, file);
                }

                current_dir = subdir;
            }

            black_box(manager);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_throughput,
    bench_inode_management,
    bench_concurrent_reads,
    bench_memory_usage
);
criterion_main!(benches);
