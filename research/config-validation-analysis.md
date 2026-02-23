# Configuration Validation Analysis

**Date:** 2026-02-23
**Task:** 2.1.1 - Research and document validation rules to keep
**Source:** `src/config/mod.rs` lines 708-983

## Summary

The codebase contains **33 validation rules** across 7 validation methods. Of these:
- **19 rules are ESSENTIAL** (must keep for correctness/safety)
- **14 rules are ARBITRARY** (upper bounds that can be removed)

## Detailed Analysis by Method

### 1. validate_api_config() - 3 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 1.1 | URL cannot be empty | **ESSENTIAL** | Empty URL causes runtime errors |
| 1.2 | URL scheme must be http/https | **ARBITRARY** | Could support other valid schemes (ftp, file, etc.) |
| 1.3 | URL must be parseable | **ESSENTIAL** | Invalid URLs cause connection errors |

**Recommendation:** Remove scheme validation, keep only non-empty and parseable checks.

### 2. validate_cache_config() - 6 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 2.1 | metadata_ttl > 0 | **ESSENTIAL** | Zero TTL breaks caching |
| 2.2 | metadata_ttl <= 86400 | **ARBITRARY** | No technical reason to limit to 24 hours |
| 2.3 | torrent_list_ttl > 0 | **ESSENTIAL** | Zero TTL breaks caching |
| 2.4 | piece_ttl > 0 | **ESSENTIAL** | Zero TTL breaks caching |
| 2.5 | max_entries > 0 | **ESSENTIAL** | Zero entries makes cache useless |
| 2.6 | max_entries <= 1,000,000 | **ARBITRARY** | Moka cache can handle more entries |

**Note:** With task 2.2.7 (Simplify CacheConfig), torrent_list_ttl and piece_ttl will be removed in favor of metadata_ttl.

**Recommendation:** Keep only > 0 checks, remove all upper bounds.

### 3. validate_mount_config() - 4 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 3.1 | Mount point must be absolute | **ESSENTIAL** | FUSE requires absolute paths |
| 3.2 | Mount point must be directory if exists | **ESSENTIAL** | FUSE mounts require directory |
| 3.3 | UID bounds check | **ESSENTIAL** | Must fit in u32 for FUSE |
| 3.4 | GID bounds check | **ESSENTIAL** | Must fit in u32 for FUSE |

**Note:** With task 2.2.2 (Remove MountConfig options), uid/gid fields will be removed entirely.

**Recommendation:** All mount config validations are essential and should remain.

### 4. validate_performance_config() - 6 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 4.1 | read_timeout > 0 | **ESSENTIAL** | Zero timeout causes hangs |
| 4.2 | read_timeout <= 3600 | **ARBITRARY** | No reason to limit to 1 hour max |
| 4.3 | max_concurrent_reads > 0 | **ESSENTIAL** | Zero disables all reads |
| 4.4 | max_concurrent_reads <= 1000 | **ARBITRARY** | Semaphore can handle more |
| 4.5 | readahead_size > 0 | **ESSENTIAL** | Zero disables prefetching |
| 4.6 | readahead_size <= 1GB | **ARBITRARY** | No technical limit at 1GB |

**Note:** With task 2.2.3 (Remove PerformanceConfig options), prefetch_enabled and check_pieces_before_read will be removed.

**Recommendation:** Keep only > 0 checks, remove all upper bounds.

### 5. validate_monitoring_config() - 5 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 5.1 | status_poll_interval > 0 | **ESSENTIAL** | Zero causes busy-loop |
| 5.2 | status_poll_interval <= 3600 | **ARBITRARY** | No reason to limit polling interval |
| 5.3 | stalled_timeout > 0 | **ESSENTIAL** | Zero timeout is invalid |
| 5.4 | stalled_timeout <= 86400 | **ARBITRARY** | No technical limit at 24 hours |
| 5.5 | status_poll_interval <= stalled_timeout | **LOGICAL** | Prevents invalid configuration |

**Note:** With task 2.2.4 (Remove MonitoringConfig), all these validations will be removed.

**Recommendation:** Remove entire method with MonitoringConfig.

### 6. validate_logging_config() - 3 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 6.1 | Log level must be valid | **ESSENTIAL** | Invalid levels cause tracing errors |
| 6.2 | metrics_interval_secs > 0 if enabled | **ESSENTIAL** | Zero interval causes division by zero or busy-loop |
| 6.3 | metrics_interval_secs <= 86400 | **ARBITRARY** | No reason to limit to 24 hours |

**Note:** With task 2.2.5 (Remove LoggingConfig options), log_fuse_operations, log_api_calls, metrics_enabled, and metrics_interval_secs will be removed. Only `level` will remain.

**Recommendation:** Simplify to only validate log level (1 rule total).

### 7. validate_resources_config() - 6 rules

| Rule | Current Check | Classification | Rationale |
|------|---------------|----------------|-----------|
| 7.1 | max_cache_bytes > 0 | **ESSENTIAL** | Zero bytes is invalid cache size |
| 7.2 | max_cache_bytes <= 10GB | **ARBITRARY** | User's system may have more RAM |
| 7.3 | max_open_streams > 0 | **ESSENTIAL** | Zero streams disables all I/O |
| 7.4 | max_open_streams <= 1000 | **ARBITRARY** | No technical limit at 1000 |
| 7.5 | max_inodes > 0 | **ESSENTIAL** | Zero inodes is invalid |
| 7.6 | max_inodes <= 10,000,000 | **ARBITRARY** | No technical limit at 10M |

**Note:** With task 2.2.6 (Remove ResourceLimitsConfig), all these validations will be removed.

**Recommendation:** Remove entire method with ResourceLimitsConfig.

## Consolidated Recommendations

### Rules to REMOVE (14 total):

**Upper bound validations (13 rules):**
- metadata_ttl <= 86400 (rule 2.2)
- max_entries <= 1,000,000 (rule 2.6)
- read_timeout <= 3600 (rule 4.2)
- max_concurrent_reads <= 1000 (rule 4.4)
- readahead_size <= 1GB (rule 4.6)
- status_poll_interval <= 3600 (rule 5.2)
- stalled_timeout <= 86400 (rule 5.4)
- metrics_interval_secs <= 86400 (rule 6.3)
- max_cache_bytes <= 10GB (rule 7.2)
- max_open_streams <= 1000 (rule 7.4)
- max_inodes <= 10,000,000 (rule 7.6)

**Scheme validation (1 rule):**
- URL scheme must be http/https (rule 1.2)

### Rules to KEEP (19 total):

**Essential validations (19 rules):**
- URL cannot be empty (rule 1.1)
- URL must be parseable (rule 1.3)
- metadata_ttl > 0 (rule 2.1)
- torrent_list_ttl > 0 (rule 2.3)
- piece_ttl > 0 (rule 2.4)
- max_entries > 0 (rule 2.5)
- Mount point must be absolute (rule 3.1)
- Mount point must be directory if exists (rule 3.2)
- UID bounds check (rule 3.3)
- GID bounds check (rule 3.4)
- read_timeout > 0 (rule 4.1)
- max_concurrent_reads > 0 (rule 4.3)
- readahead_size > 0 (rule 4.5)
- status_poll_interval > 0 (rule 5.1)
- stalled_timeout > 0 (rule 5.3)
- status_poll_interval <= stalled_timeout (rule 5.5)
- Log level must be valid (rule 6.1)
- metrics_interval_secs > 0 if enabled (rule 6.2)
- max_cache_bytes > 0 (rule 7.1)
- max_open_streams > 0 (rule 7.3)
- max_inodes > 0 (rule 7.5)

### Methods to REMOVE entirely:

With configuration simplification (tasks 2.2.x), these entire validation methods will be removed:
- `validate_monitoring_config()` - 5 rules
- `validate_resources_config()` - 6 rules
- Most of `validate_logging_config()` - 2 rules (keep only log level validation)

### Final Validation Count:

After all simplifications:
- **Current:** 33 rules across 7 methods
- **After removing upper bounds:** 20 rules across 7 methods
- **After config simplification:** ~8-10 rules across 3-4 methods

## Implementation Plan

1. **Task 2.1.2**: Remove 14 arbitrary upper bound validations
2. **Task 2.1.3**: Remove URL scheme validation (already included in 2.1.2 count)
3. **Task 2.1.4**: Consolidate remaining methods after config simplification
4. **Task 2.2.x**: Remove entire methods as config sections are removed

## References

- `src/config/mod.rs:708-983` - All validation implementations
- `research/config_fields_usage_analysis.md` - Field usage analysis
- `TODO.md` - Task 2.1.1, 2.1.2, 2.1.3, 2.1.4
