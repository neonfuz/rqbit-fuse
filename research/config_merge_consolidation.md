# Config Merge Consolidation Research

## Current State

### `merge_from_env` (lines 109-157, ~48 lines)
- Returns `Result<Self, RqbitFuseError>`
- Uses `merge_env_var!` macro for most fields
- Has special handling for:
  - `TORRENT_FUSE_READ_TIMEOUT` with custom validation (lines 121-132)
  - Auth credentials with `TORRENT_FUSE_AUTH_USERPASS` support (lines 136-154)

### `merge_from_cli` (lines 188-206, ~18 lines)
- Returns `Self`
- Simple pattern: `if let Some(ref x) = cli.field { self.field = x.clone(); }`
- No error handling needed (CLI args already parsed)

## Usage Patterns

1. `load()` - `Self::from_default_locations()?.merge_from_env()`
2. `load_with_cli(cli)` - `Ok(Self::from_default_locations()?.merge_from_env()?.merge_from_cli(cli))`

## Consolidation Strategy

### Option 1: Generic Merge Method
Create a single `merge()` method that takes a configuration source trait:

```rust
pub trait ConfigSource {
    fn get_api_url(&self) -> Option<String>;
    fn get_mount_point(&self) -> Option<PathBuf>;
    fn get_username(&self) -> Option<String>;
    fn get_password(&self) -> Option<String>;
    // ... etc
}

impl ConfigSource for CliArgs { ... }
impl ConfigSource for EnvConfig { ... }

pub fn merge(mut self, source: &dyn ConfigSource) -> Self { ... }
```

### Option 2: Unified Macro Pattern
Create a unified macro that can handle both Some() and Result patterns:

```rust
macro_rules! merge_field {
    ($self:ident, $field:ident, $value:expr) => {
        if let Some(v) = $value {
            $self.$field = v;
        }
    };
}
```

### Option 3: Simplification
Make `merge_from_env` use the same simple pattern as `merge_from_cli` by:
- Moving env var parsing to a separate step that produces Option values
- Then using the same merge pattern

### Recommended Approach: Option 3 with Struct Extraction

1. Create a `ConfigSource` struct to hold optional values (like a "partial config")
2. Create methods to build `ConfigSource` from env vars (returns Result) and CLI (infallible)
3. Create single `merge_from_source(source: ConfigSource)` method
4. Update callers to use `merge_from_source(ConfigSource::from_env()?)` and `merge_from_source(ConfigSource::from_cli(cli))`

## Expected Line Reduction

- Remove `merge_from_env` method body: -48 lines
- Remove `merge_from_cli` method body: -18 lines
- Add `ConfigSource` struct and builders: +30 lines
- Add unified `merge_from_source` method: +10 lines
- Net reduction: ~50 lines (as estimated in TODO)
