# Config Flattening Research

## Current Structure
The Config struct has 6 nested sub-structs:

1. **ApiConfig** (lines 21-27)
   - url: String
   - username: Option<String>
   - password: Option<String>

2. **CacheConfig** (lines 30-35)
   - metadata_ttl: u64
   - max_entries: usize

3. **MountConfig** (lines 38-42)
   - mount_point: PathBuf

4. **PerformanceConfig** (lines 45-51)
   - read_timeout: u64
   - max_concurrent_reads: usize
   - readahead_size: u64

5. **LoggingConfig** (lines 54-58)
   - level: String

## Flattening Strategy
Move all fields directly into Config with serde flatten attributes to maintain backward compatibility with existing TOML/JSON config files.

## Files Affected
1. src/config/mod.rs - Main config definition and all tests
2. src/fs/filesystem.rs - Uses: api.url, mount.mount_point, performance.max_concurrent_reads, performance.read_timeout
3. src/main.rs - Uses: mount.mount_point, api.url
4. tests/config_tests.rs - Uses all config fields in tests
5. tests/resource_tests.rs - Uses: api.url, mount.mount_point, performance.max_concurrent_reads

## Changes Required
- Remove 5 sub-structs (ApiConfig, CacheConfig, MountConfig, PerformanceConfig, LoggingConfig)
- Remove 5 manual Default impls
- Add serde(flatten) attributes to maintain backward compatibility
- Update all field access from `config.api.url` to `config.api_url`
- Update all field access from `config.mount.mount_point` to `config.mount_point`
- Update all field access from `config.cache.metadata_ttl` to `config.metadata_ttl`
- Update all field access from `config.cache.max_entries` to `config.max_entries`
- Update all field access from `config.performance.read_timeout` to `config.read_timeout`
- Update all field access from `config.performance.max_concurrent_reads` to `config.max_concurrent_reads`
- Update all field access from `config.performance.readahead_size` to `config.readahead_size`
- Update all field access from `config.logging.level` to `config.log_level`
- Update all field access from `config.api.username` to `config.api_username`
- Update all field access from `config.api.password` to `config.api_password`

## Line Count Impact
- Removing 5 struct definitions: ~35 lines
- Removing 5 Default impls: ~43 lines
- Total reduction: ~78 lines from config/mod.rs
- Plus simplification in other files where nested access is used
