# Environment Variable Handling Simplification

## Current State
The `merge_from_env()` method in `src/config/mod.rs` (lines 126-182) contains ~56 lines of repetitive code for parsing environment variables:

- 8 separate `if let Ok(val)` blocks for different config fields
- Each numeric field has its own error handling
- Special handling for auth credentials (combined and individual fields)

## Proposed Solution

Create a macro to consolidate the repetitive pattern:

```rust
macro_rules! merge_env_var {
    ($self:ident, $field:ident, $var:expr) => {
        if let Ok(val) = std::env::var($var) {
            $self.$field = val;
        }
    };
    ($self:ident, $field:ident, $var:expr, $parser:expr) => {
        if let Ok(val) = std::env::var($var) {
            $self.$field = $parser(&val).map_err(|_| {
                RqbitFuseError::InvalidArgument(format!("{} has invalid format", $var))
            })?;
        }
    };
}
```

This will reduce:
- ~8 repetitive blocks to ~8 macro invocations (1 line each)
- Remove duplicate error message formatting
- Keep auth handling separate due to its special logic

## Expected Line Reduction

Before: ~56 lines (merge_from_env method)
After: ~25 lines (macro definition + invocations)
Net reduction: ~31 lines

Combined with macro reuse opportunities in tests, total expected reduction: ~40-50 lines
