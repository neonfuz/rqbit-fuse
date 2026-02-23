# SPEC-TODO.md - Specification Update Plan

This document tracks the plan for updating specification documents to reflect the code reduction changes from review-r4.md.

---

## Overview

The codebase is being reduced by ~70% (5400 lines → 1600 lines). Specifications must be updated to reflect the simplified architecture, configuration, and removed features.

---

## Spec Files Requiring Updates

### 1. architecture.md
**Status:** Needs significant updates
**Priority:** HIGH (Core specification)

#### Changes Required:
- [ ] **Remove status monitoring** from component responsibilities
  - Remove "Background torrent discovery and status monitoring" line
  - Update init() description to remove background task mentions
  
- [ ] **Simplify metrics** in architecture diagram
  - Remove Metrics Collection box from diagram
  - Keep only minimal metrics reference
  
- [ ] **Update CLI** section
  - Remove `--format json` from status command
  - Remove `--allow-other` and `--auto-unmount` mount options
  - Remove mount info from status output description
  
- [ ] **Update configuration schema**
  - Remove MonitoringConfig section
  - Remove LoggingConfig detailed options
  - Simplify to 6 essential fields
  
- [ ] **Remove background tasks** section
  - Remove status monitoring task description
  - Keep only torrent discovery

#### Lines affected: ~100 lines to be removed/updated

---

### 2. technical-design.md
**Status:** Needs major updates  
**Priority:** HIGH (Implementation reference)

#### Changes Required:
- [ ] **Update FUSE callbacks** section
  - Remove `track_and_prefetch()` from read() description
  - Remove piece availability checking with EAGAIN option
  - Update init() to remove background monitoring task
  
- [ ] **Simplify error mapping** table
  - Reduce from 28 error types to 8
  - Update ApiError section to show simplified enum
  
- [ ] **Update configuration structures**
  - Replace Config struct with simplified version (6 fields)
  - Remove MonitoringConfig struct
  - Remove detailed LoggingConfig options
  - Update defaults section
  
- [ ] **Simplify metrics** section
  - Replace FuseMetrics/ApiMetrics with minimal Metrics struct
  - Remove ShardedCounter (no longer needed)
  
- [ ] **Update cache** section
  - Remove bitfield cache references
  - Simplify Cache struct (remove stats if removed)
  
- [ ] **Remove file handle state** section
  - Remove FileHandleState struct documentation
  - Update FileHandleManager to basic version
  
- [ ] **Remove macros** section
  - Remove fs/macros.rs from file structure
  - Remove Fuse macros documentation

#### Lines affected: ~200 lines to be removed/updated

---

### 3. quickstart.md
**Status:** Needs updates
**Priority:** HIGH (User-facing documentation)

#### Changes Required:
- [ ] **Update status command** output
  - Remove filesystem/size/used/available from example output
  - Remove `--format json` option
  - Show simplified "MOUNTED"/"NOT MOUNTED" only
  
- [ ] **Simplify configuration examples**
  - Remove removed fields from config.toml example:
    - `allow_other`, `auto_unmount` from [mount]
    - `piece_check_enabled`, `return_eagain_for_unavailable` from [performance]
    - Entire [monitoring] section
    - `log_fuse_operations`, `log_api_calls`, `metrics_enabled`, `metrics_interval_secs` from [logging]
    - `torrent_list_ttl`, `piece_ttl` from [cache]
  - Reduce to 6 essential fields
  
- [ ] **Update environment variables**
  - Remove variables for deleted config options
  - Keep only: API_URL, MOUNT_POINT, METADATA_TTL, MAX_ENTRIES, READ_TIMEOUT, LOG_LEVEL
  
- [ ] **Update CLI reference**
  - Remove `--allow-other`, `--auto-unmount` from mount options
  - Remove `--format` from status command

#### Lines affected: ~150 lines to be removed/updated

---

### 4. api.md
**Status:** Minor updates needed
**Priority:** MEDIUM

#### Changes Required:
- [ ] **Remove bitfield caching** documentation
  - Remove mention of 5-second TTL for bitfields
  - Update to reflect synchronous checking
  
- [ ] **Update error responses** section
  - Simplify error list to 8 essential types
  - Remove detailed error categorization

#### Lines affected: ~30 lines

---

### 5. cache.md (if exists)
**Status:** Verify existence, then update
**Priority:** LOW

#### Changes Required:
- [ ] Simplify to reflect reduced cache implementation
- [ ] Remove statistics tracking if removed

---

### 6. error-handling.md (if exists)
**Status:** Verify existence, then update
**Priority:** MEDIUM

#### Changes Required:
- [ ] Replace 28 error variants with 8 simplified variants
- [ ] Update error mapping table
- [ ] Simplify error handling flow

---

### 7. testing.md (if exists)
**Status:** Verify existence, then update
**Priority:** LOW

#### Changes Required:
- [ ] Remove tests for removed features
- [ ] Update test strategy to reflect trimmed test suite

---

## Files NOT Requiring Updates

The following files likely don't need changes:
- `inode-design.md` - Inode management not affected
- `read-ahead.md` - May need removal if prefetching docs exist
- `async-fuse.md` - Async bridge not affected
- `signal-handling.md` - Signal handling not affected
- `public-api.md` - Public API unchanged

---

## Implementation Order

Update specs in this order to minimize rework:

1. **technical-design.md** (Phase 1) - Core implementation details
2. **architecture.md** (Phase 1) - High-level design  
3. **quickstart.md** (Phase 2) - User documentation
4. **api.md** (Phase 2) - API documentation
5. **error-handling.md** (Phase 3) - Error system simplification
6. **cache.md** (Phase 7) - Cache simplification

---

## Tracking Template for Each Document

When updating a spec document, use this checklist:

```markdown
## Document: [filename]

### Phase 1 Updates (High Priority Removals)
- [ ] Remove status monitoring references
- [ ] Remove mount info references  
- [ ] Remove JSON output references
- [ ] Remove DiscoveryResult references

### Phase 2 Updates (Configuration)
- [ ] Simplify Config struct documentation
- [ ] Remove MonitoringConfig
- [ ] Remove ResourceLimitsConfig
- [ ] Update CLI options
- [ ] Update environment variables list

### Phase 3 Updates (Errors)
- [ ] Replace error enum with simplified version
- [ ] Update error mapping table

### Phase 4 Updates (Metrics)
- [ ] Replace metrics documentation with minimal version
- [ ] Remove ShardedCounter documentation

### Phase 5 Updates (File Handles)
- [ ] Remove FileHandleState documentation
- [ ] Simplify FileHandleManager docs

### Phase 6 Updates (FUSE Logging)
- [ ] Remove macro documentation

### Phase 7 Updates (Cache)
- [ ] Remove bitfield cache references
- [ ] Simplify cache implementation docs

### Phase 8-9 Updates (Tests/Docs)
- [ ] Note reduced test coverage
- [ ] Trim verbose documentation

### Final Review
- [ ] Verify all removed features are documented as removed
- [ ] Check for broken internal links
- [ ] Update "Last updated" date
```

---

## Specific Content Changes

### Error Type Mapping (Old → New)

Document these mappings for reference:

| Old Error | New Error |
|-----------|-----------|
| TorrentNotFound | NotFound |
| FileNotFound | NotFound |
| InvalidRange | InvalidArgument |
| ConnectionTimeout | TimedOut |
| ReadTimeout | TimedOut |
| ServerDisconnected | NetworkError |
| NetworkError(_) | NetworkError(_) |
| CircuitBreakerOpen | Other(String) |
| ServiceUnavailable | NetworkError |
| ApiError { 404 } | NotFound |
| ApiError { 403 } | PermissionDenied |
| ApiError { 503 } | NetworkError |
| All validation errors | InvalidArgument |
| All resource errors | Other(String) |
| All state errors | Other(String) |
| All directory errors | Other(String) |
| All data errors | IoError |

### Configuration Mapping (Old → New)

| Old Config | New Config | Notes |
|------------|-----------|-------|
| api.url | api_url | Flattened |
| mount.mount_point | mount_point | Flattened |
| cache.metadata_ttl | cache_ttl | Renamed |
| cache.max_entries | max_cache_entries | Renamed |
| performance.read_timeout | read_timeout | Flattened |
| logging.level | log_level | Renamed |
| mount.allow_other | REMOVED | Use defaults |
| mount.auto_unmount | REMOVED | Use defaults |
| mount.uid/gid | REMOVED | Use current user |
| cache.torrent_list_ttl | REMOVED | Use cache_ttl |
| cache.piece_ttl | REMOVED | Use cache_ttl |
| performance.prefetch_enabled | REMOVED | Feature removed |
| performance.check_pieces_before_read | REMOVED | Always check |
| performance.max_concurrent_reads | REMOVED | Hardcoded |
| performance.readahead_size | REMOVED | Hardcoded |
| monitoring.status_poll_interval | REMOVED | Feature removed |
| monitoring.stalled_timeout | REMOVED | Feature removed |
| logging.log_fuse_operations | REMOVED | Always log debug |
| logging.log_api_calls | REMOVED | Always log debug |
| logging.metrics_enabled | REMOVED | Feature removed |
| logging.metrics_interval_secs | REMOVED | Feature removed |
| resources.max_cache_bytes | REMOVED | Hardcoded |
| resources.max_open_streams | REMOVED | Hardcoded |
| resources.max_inodes | REMOVED | Hardcoded |

---

## Definition of Done

Each spec document is complete when:
- [ ] All references to removed features are deleted or marked as removed
- [ ] Configuration examples show only the 6 essential fields
- [ ] Error documentation reflects 8 simplified types
- [ ] CLI documentation shows only available options
- [ ] Architecture diagrams are updated (ASCII art removed or simplified)
- [ ] No broken internal links exist
- [ ] "Last updated" date is current
- [ ] Consistent with actual implementation

---

## Notes for Spec Writers

1. **Preserve essential documentation**: Don't remove everything, focus on what users actually need
2. **Mark removals clearly**: Use strikethrough or "(removed)" markers during transition
3. **Update examples**: Ensure all config examples use the simplified format
4. **Check consistency**: Verify architecture.md matches technical-design.md
5. **User impact**: Consider what users need to know about removed features

---

## Success Criteria

After all spec updates:
- [ ] Specifications reflect the 70% reduced codebase
- [ ] New developers can understand the system from specs alone
- [ ] Users can set up and run with simplified configuration
- [ ] No references to removed features in active documentation
- [ ] All examples work with the new simplified configuration

---

*Created: 2026-02-23*
*Based on: review-r4.md code review and TODO.md implementation plan*
