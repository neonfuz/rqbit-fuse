# Research: strum Dependency Usage

## Finding
The `strum` crate is used solely for deriving the `Display` trait on two enums in `src/api/types.rs`:

1. `DataUnavailableReason` (line 6)
2. `TorrentState` (line 381)

## Usage Analysis
Searched entire codebase for actual Display trait usage:
- No `.to_string()` calls on these enum types
- No `format!()` macros using these enums
- No logging/tracing calls that would invoke Display

These enums are only used for:
- Pattern matching (comparisons)
- Struct fields
- Match arms

## Conclusion
The Display derive is unused code. Can safely remove:
1. `use strum::Display;` import from src/api/types.rs
2. `Display` from derive macros on both enums
3. `strum` dependency from Cargo.toml

## Impact
- Removes unused dependency
- Simplifies code (no unnecessary derives)
- No functional changes
