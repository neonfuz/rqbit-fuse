# TODO.md - Implementation Checklist

## Phase 1: High Priority Spec Updates

### architecture.md
- [x] Remove status monitoring from component responsibilities
  - Remove "Background torrent discovery and status monitoring" line
  - Update init() description to remove background task mentions
- [x] Simplify metrics in architecture diagram
  - Remove Metrics Collection box from diagram
  - Keep only minimal metrics reference
- [x] Update CLI section
  - Remove `--format json` from status command
  - Remove `--allow-other` and `--auto-unmount` mount options
  - Remove mount info from status output description
- [x] Update configuration schema
  - Remove MonitoringConfig section
  - Remove LoggingConfig detailed options
  - Simplify to 6 essential fields
- [x] Remove background tasks section
  - Remove status monitoring task description
  - Keep only torrent discovery

### technical-design.md
- [x] Update FUSE callbacks section
  - Remove `track_and_prefetch()` from read() description
  - Remove piece availability checking with EAGAIN option
  - Update init() to remove background monitoring task
- [x] Simplify error mapping table
  - Reduce from 28 error types to 8
  - Update ApiError section to show simplified enum
- [x] Update configuration structures
  - Replace Config struct with simplified version (6 fields)
  - Remove MonitoringConfig struct
  - Remove detailed LoggingConfig options
  - Update defaults section
- [x] Simplify metrics section
  - Replace FuseMetrics/ApiMetrics with minimal Metrics struct
  - Remove ShardedCounter (no longer needed)
- [x] Update cache section
  - Remove bitfield cache references
  - Simplify Cache struct (remove stats if removed)
- [x] Remove file handle state section
  - Remove FileHandleState struct documentation
  - Update FileHandleManager to basic version
- [x] Remove macros section
  - Remove fs/macros.rs from file structure
  - Remove Fuse macros documentation

### quickstart.md
- [x] Update status command output
  - Remove filesystem/size/used/available from example output
  - Remove `--format json` option
  - Show simplified "MOUNTED"/"NOT MOUNTED" only
- [x] Simplify configuration examples
  - Remove removed fields from config.toml example
  - Reduce to 6 essential fields
- [ ] Update environment variables
  - Remove variables for deleted config options
  - Keep only: API_URL, MOUNT_POINT, METADATA_TTL, MAX_ENTRIES, READ_TIMEOUT, LOG_LEVEL
- [ ] Update CLI reference
  - Remove `--allow-other`, `--auto-unmount` from mount options
  - Remove `--format` from status command

### api.md
- [ ] Remove bitfield caching documentation
  - Remove mention of 5-second TTL for bitfields
  - Update to reflect synchronous checking
- [ ] Update error responses section
  - Simplify error list to 8 essential types
  - Remove detailed error categorization

## Phase 2: Additional Spec Files

### error-handling.md
- [ ] Replace 28 error variants with 8 simplified variants
- [ ] Update error mapping table
- [ ] Simplify error handling flow

### cache.md
- [ ] Simplify to reflect reduced cache implementation
- [ ] Remove statistics tracking if removed

### testing.md
- [ ] Remove tests for removed features
- [ ] Update test strategy to reflect trimmed test suite

---

*Generated from SPEC-TODO.md*
*Created: 2026-02-23*
