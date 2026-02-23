# SIMPLIFY-004: Create FUSE Logging Macros

## Summary

Create logging macros to eliminate repetitive logging boilerplate in every FUSE operation in `src/fs/filesystem.rs`. This migration will reduce ~180 lines of duplicated logging code to ~60 lines of macro definitions and usage.

---

## Task ID

**SIMPLIFY-004**

---

## Scope

### Files to Modify

1. **src/fs/filesystem.rs** - Add macro imports and replace all repetitive logging patterns
2. **src/fs/macros.rs** (NEW) - Create new file with macro definitions
3. **src/fs/mod.rs** - Export the macros module

### Operations to Update

- `read()` (lines 802-1027) - ~15 logging blocks
- `release()` (lines 1032-1049) - 2 logging blocks
- `lookup()` (lines 1054-1149) - 6 logging blocks
- `getattr()` (lines 1155-1195) - 4 logging blocks
- `open()` (lines 1201-1288) - 6 logging blocks
- `readlink()` (lines 1292-1310) - 4 logging blocks
- `readdir()` (lines 1316-1483) - 5 logging blocks

**Total: ~42 repetitive logging blocks across 7 operations**

---

## Current State

### Repetitive Logging Pattern (~180 lines)

Each FUSE operation has this boilerplate repeated 3-8 times:

```rust
// Operation start logging (lines 818-820)
if self.config.logging.log_fuse_operations {
    debug!(fuse_op = "read", ino = ino, offset = offset, size = size);
}

// Error response logging (lines 826-834)
if self.config.logging.log_fuse_operations {
    debug!(
        fuse_op = "read",
        ino = ino,
        result = "error",
        error = "EINVAL",
        reason = "negative_offset"
    );
}

// Success logging (lines 979-986)
if self.config.logging.log_fuse_operations {
    debug!(
        fuse_op = "read",
        ino = ino,
        result = "success",
        bytes_read = bytes_read,
        latency_ms = latency.as_millis() as u64
    );
}
```

### Issues with Current Approach

1. **Duplicated boilerplate** - Same `if self.config.logging.log_fuse_operations` check everywhere
2. **Inconsistent formatting** - Some use `debug!`, some use `error!`, some use `warn!`
3. **Error-prone** - Easy to forget to add logging to new operations
4. **Hard to maintain** - Changing log format requires updating 42+ locations
5. **Clutters code** - Logging obscures the actual business logic

---

## Target State

### Macro Definitions (src/fs/macros.rs)

```rust
/// Log the start of a FUSE operation
/// 
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name (e.g., "read", "lookup")
/// * `$( $key = $value ),*` - Key-value pairs to log
#[macro_export]
macro_rules! fuse_log {
    ($self:expr, $op:expr, $( $key:ident = $value:expr ),* $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                $( $key = $value, )*
            );
        }
    };
}

/// Log a FUSE error response
/// 
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name
/// * `$error` - Error code name (e.g., "ENOENT", "EINVAL")
/// * `$( $key = $value ),*` - Additional context
#[macro_export]
macro_rules! fuse_error {
    ($self:expr, $op:expr, $error:expr $(, $reason_key:ident = $reason:expr )? $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                result = "error",
                error = $error,
                $( $reason_key = $reason, )?
            );
        }
    };
}

/// Log a successful FUSE operation result
/// 
/// # Arguments
/// * `$self` - The filesystem instance
/// * `$op` - Operation name
/// * `$( $key = $value ),*` - Result fields to log
#[macro_export]
macro_rules! fuse_ok {
    ($self:expr, $op:expr, $( $key:ident = $value:expr ),* $(,)? ) => {
        if $self.config.logging.log_fuse_operations {
            ::tracing::debug!(
                fuse_op = $op,
                result = "success",
                $( $key = $value, )*
            );
        }
    };
}

// Re-export for internal use
pub use fuse_log;
pub use fuse_error;
pub use fuse_ok;
```

### Example Usage Transformations

#### Before (read operation - lines 818-820):
```rust
if self.config.logging.log_fuse_operations {
    debug!(fuse_op = "read", ino = ino, offset = offset, size = size);
}
```

#### After:
```rust
fuse_log!(self, "read", ino = ino, offset = offset, size = size);
```

#### Before (error case - lines 826-834):
```rust
if self.config.logging.log_fuse_operations {
    debug!(
        fuse_op = "read",
        ino = ino,
        result = "error",
        error = "EINVAL",
        reason = "negative_offset"
    );
}
```

#### After:
```rust
fuse_error!(self, "read", "EINVAL", reason = "negative_offset");
```

#### Before (success case - lines 979-986):
```rust
if self.config.logging.log_fuse_operations {
    debug!(
        fuse_op = "read",
        ino = ino,
        result = "success",
        bytes_read = bytes_read,
        latency_ms = latency.as_millis() as u64
    );
}
```

#### After:
```rust
fuse_ok!(
    self,
    "read",
    ino = ino,
    bytes_read = bytes_read,
    latency_ms = latency.as_millis() as u64
);
```

---

## Implementation Steps

### Step 1: Create macros.rs (NEW FILE)

**File**: `src/fs/macros.rs`

Create the file with the macro definitions shown in "Target State" above.

### Step 2: Update mod.rs

**File**: `src/fs/mod.rs`

Add macro module export:

```rust
// After existing modules
pub mod macros;

// Re-export macros for convenience
pub use macros::{fuse_error, fuse_log, fuse_ok};
```

### Step 3: Update filesystem.rs imports

**File**: `src/fs/filesystem.rs`

Add macro imports at the top of the file (around line 17):

```rust
use crate::fs::macros::{fuse_error, fuse_log, fuse_ok};
```

### Step 4: Replace logging in read() operation

**File**: `src/fs/filesystem.rs`
**Lines**: 802-1027

Replace these logging blocks:

1. **Line 818-820** (operation start):
   ```rust
   // REPLACE:
   if self.config.logging.log_fuse_operations {
       debug!(fuse_op = "read", ino = ino, offset = offset, size = size);
   }
   // WITH:
   fuse_log!(self, "read", ino = ino, offset = offset, size = size);
   ```

2. **Lines 826-834** (EINVAL error):
   ```rust
   // REPLACE:
   if self.config.logging.log_fuse_operations {
       debug!(
           fuse_op = "read",
           ino = ino,
           result = "error",
           error = "EINVAL",
           reason = "negative_offset"
       );
   }
   // WITH:
   fuse_error!(self, "read", "EINVAL", reason = "negative_offset");
   ```

3. **Lines 854-861** (EISDIR error):
   ```rust
   // REPLACE with:
   fuse_error!(self, "read", "EISDIR");
   ```

4. **Lines 870-877** (ENOENT error):
   ```rust
   // REPLACE with:
   fuse_error!(self, "read", "ENOENT");
   ```

5. **Lines 886-894** (empty read success):
   ```rust
   // REPLACE with:
   fuse_ok!(self, "read", ino = ino, bytes_read = 0, reason = "empty_read");
   ```

6. **Lines 904-913** (range logging - keep as-is or simplify):
   ```rust
   // This is intermediate logging, can keep or convert to:
   fuse_log!(self, "read", ino = ino, torrent_id = torrent_id, file_index = file_index, 
             range_start = offset, range_end = end);
   ```

7. **Lines 922-931** (EAGAIN - not ready):
   ```rust
   // REPLACE with:
   fuse_error!(self, "read", "EAGAIN", reason = "torrent_not_ready");
   ```

8. **Lines 938-947** (EAGAIN - not monitored):
   ```rust
   // REPLACE with:
   fuse_error!(self, "read", "EAGAIN", reason = "torrent_not_monitored");
   ```

9. **Lines 979-986** (success):
   ```rust
   // REPLACE with:
   fuse_ok!(self, "read", ino = ino, bytes_read = bytes_read, 
            latency_ms = latency.as_millis() as u64);
   ```

### Step 5: Replace logging in release() operation

**Lines**: 1042-1046

```rust
// REPLACE:
if self.config.logging.log_fuse_operations {
    debug!(fuse_op = "release", ino = _ino);
}
// WITH:
fuse_log!(self, "release", ino = _ino);
```

### Step 6: Replace logging in lookup() operation

**Lines**: 1061-1147

Replace 6 logging blocks:

1. **Lines 1065-1067**: `fuse_log!(self, "lookup", parent = parent, name = %name_str);`
2. **Lines 1075-1082**: `fuse_error!(self, "lookup", "ENOENT");`
3. **Lines 1093-1100**: `fuse_error!(self, "lookup", "ENOTDIR");`
4. **Lines 1122-1124**: `fuse_ok!(self, "lookup", parent = parent, name = %name_str, ino = ino);`
5. **Line 1142-1144**: `fuse_log!(self, "lookup", parent = parent, name = %name_str, result = "not_found");`

### Step 7: Replace logging in getattr() operation

**Lines**: 1155-1195

Replace 4 logging blocks:

1. **Lines 1158-1160**: `fuse_log!(self, "getattr", ino = ino);`
2. **Lines 1168-1176**: `fuse_ok!(self, "getattr", ino = ino, kind = ?attr.kind, size = attr.size);`
3. **Lines 1183-1191**: `fuse_error!(self, "getattr", "ENOENT");`

### Step 8: Replace logging in open() operation

**Lines**: 1201-1288

Replace 6 logging blocks:

1. **Lines 1204-1206**: `fuse_log!(self, "open", ino = ino, flags = flags);`
2. **Lines 1215-1222**: `fuse_error!(self, "open", "EISDIR");`
3. **Lines 1232-1240**: `fuse_error!(self, "open", "ELOOP");`
4. **Lines 1250-1258**: `fuse_error!(self, "open", "EACCES", reason = "write_access_requested");`
5. **Lines 1267-1269**: `fuse_ok!(self, "open", ino = ino, fh = fh);`
6. **Lines 1276-1283**: `fuse_error!(self, "open", "ENOENT");`

### Step 9: Replace logging in readlink() operation

**Lines**: 1292-1310

Note: This operation uses raw `debug!()` calls without the `log_fuse_operations` check. Either:
- Keep as-is (it's inconsistent but functional)
- Or wrap with macros: `fuse_log!(self, "readlink", ino = ino);`

### Step 10: Replace logging in readdir() operation

**Lines**: 1316-1483

Replace 5 logging blocks:

1. **Lines 1326-1328**: `fuse_log!(self, "readdir", ino = ino, offset = offset);`
2. **Lines 1404-1411**: `fuse_error!(self, "readdir", "ENOENT");`
3. **Lines 1422-1429**: `fuse_error!(self, "readdir", "ENOTDIR");`

### Step 11: Handle special error logging

Some error logging uses `error!` macro for actual errors (not just debug). These should NOT be replaced:

- **Lines 1015-1022**: Real error logging for read failures - keep `error!`
- **Lines 1128-1135**: Path maps to missing inode - keep `error!`

---

## Testing

### Step 1: Build verification

```bash
cargo build
```

### Step 2: Run existing tests

```bash
cargo test fs::filesystem::tests
```

### Step 3: Verify logging still works

1. Enable FUSE operation logging in config
2. Mount the filesystem
3. Perform operations (ls, cat, etc.)
4. Verify logs appear with same format as before

### Step 4: Check line count reduction

```bash
# Before migration
grep -c "if self.config.logging.log_fuse_operations" src/fs/filesystem.rs
# Expected: ~42 occurrences

# After migration
grep -c "fuse_log!\|fuse_error!\|fuse_ok!" src/fs/filesystem.rs
# Expected: ~42 occurrences (but much shorter lines)
```

---

## Expected Reduction

### Line Count Analysis

| Metric | Before | After | Reduction |
|--------|--------|-------|-----------|
| Logging boilerplate lines | ~180 | ~60 | **~120 lines (66%)** |
| Characters in logging code | ~6,500 | ~2,200 | **~4,300 chars (66%)** |
| Average lines per operation | 25.7 | 8.6 | **17.1 lines** |

### Readability Improvements

1. **Clear intent**: `fuse_error!()` vs scattered `debug!()` calls
2. **Consistent format**: All errors log `result = "error"` automatically
3. **Self-documenting**: Macro names describe what's happening
4. **Less visual noise**: Business logic is more visible

---

## Backwards Compatibility

### Log Format Preservation

The macros preserve the exact log format:

- **Before**: `debug!(fuse_op = "read", ino = 5, result = "error", error = "ENOENT")`
- **After**: Same output, generated by `fuse_error!(self, "read", "ENOENT")`

### Configuration Compatibility

The `log_fuse_operations` config flag is still respected - the macros check it internally.

---

## Related Tasks

- **SIMPLIFY-001** through **SIMPLIFY-003** - Other code simplification tasks
- **FS-007** - FUSE operation tests (should be easier with cleaner code)
- **METRICS-003** - Reduce trace overhead (macros make this easier to implement)

---

## Risks and Mitigation

### Risk 1: Macro Compilation Errors

**Mitigation**: Start with simple macros, test incrementally after each operation replacement.

### Risk 2: Log Format Changes

**Mitigation**: Compare log output before/after to ensure identical format.

### Risk 3: Performance Impact

**Mitigation**: The macros inline the same code that was there before - no runtime overhead.

---

## Completion Criteria

- [ ] `src/fs/macros.rs` created with 3 macro definitions (~40 lines)
- [ ] `src/fs/mod.rs` updated to export macros
- [ ] All 42 logging blocks replaced with macro calls
- [ ] Code compiles without warnings: `cargo build`
- [ ] All tests pass: `cargo test`
- [ ] Clippy passes: `cargo clippy`
- [ ] Formatting passes: `cargo fmt`
- [ ] Log output verified identical to before migration
- [ ] Line count reduced by ~120 lines

---

*Created for rqbit-fuse project - February 14, 2026*
