# RqbitClient Module Split Analysis - ARCH-004

## Current Structure

### client.rs (1840 lines)
- `CircuitState` enum (lines ~19-26)
- `CircuitBreaker` struct with impl (lines ~29-120)
- `RqbitClient` struct with all methods (~1700 lines)

### streaming.rs (already separate)
- `PersistentStreamManager` 
- `StreamManagerStats`

## Current Imports
```rust
use crate::api::streaming::PersistentStreamManager;
use crate::api::types::*;
```

## Analysis

### What's Already Separated
- Streaming is in its own module (streaming.rs)

### What Could Be Split

1. **Circuit Breaker** (~100 lines)
   - Could be moved to `api/circuit_breaker.rs`
   - Contains: CircuitState, CircuitBreaker
   - Used only by RqbitClient

2. **Retry Logic** (embedded in methods)
   - Could be extracted to a helper function or `api/retry.rs`
   - Currently inline in various methods

## Recommendations

### Option A: Minimal Split (Recommended)
Keep RqbitClient as-is but extract CircuitBreaker to separate module:
- `api/circuit_breaker.rs` - CircuitState, CircuitBreaker
- `api/client.rs` - RqbitClient (imports circuit_breaker)

This is a small change that improves modularity without breaking the API.

### Option B: Full Split
Create multiple modules:
- `api/circuit_breaker.rs` - CircuitState, CircuitBreaker
- `api/retry.rs` - Retry logic/helper
- `api/client.rs` - RqbitClient

This would require more extensive refactoring and potentially API changes.

## Impact Assessment

### Breaking Changes
- Any code importing `CircuitState` or `CircuitBreaker` directly from `api::client` would break
- Need to update imports in `api/mod.rs`

### Non-Breaking
- RqbitClient stays in same location
- Streaming already separate

## Decision

**Recommended: Option A** - Extract CircuitBreaker only

This provides:
- Cleaner separation of concerns
- Minimal code changes
- No API breaking changes

However, this task may be lower priority since the code is already reasonably organized.
