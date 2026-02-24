# FUSE Macro Usage Analysis

**Research Date**: 2026-02-23  
**Scope**: Phase 6.1 - FUSE Logging Simplification  
**Objective**: Document all FUSE macro usages to prepare for replacement with direct tracing calls

---

## Summary

There are **4 macro types** used in the codebase, totaling **33 call sites**:

| Macro Type | Count | Purpose |
|------------|-------|---------|
| `fuse_log!` | 7 | Log start of FUSE operation |
| `fuse_error!` | 8 | Log error responses (direct + via reply_* macros) |
| `fuse_ok!` | 7 | Log successful operation completion |
| `reply_*!` | 11 | Combined error metric + fuse_error! + reply.error() |

**Total Lines in macros.rs**: 98 lines (can be removed entirely)

---

## Macro Definitions

### 1. fuse_log! (lines 7-14)

Logs the start of a FUSE operation with operation name and optional key-value pairs.

```rust
macro_rules! fuse_log {
    ($op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            $( $key = $value, )*
        );
    };
}
```

**Replacement**: Direct `tracing::debug!` call with `fuse_op` field.

---

### 2. fuse_error! (lines 23-32)

Logs a FUSE error response with operation name, error code, and optional context.

```rust
macro_rules! fuse_error {
    ($op:expr, $error:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            result = "error",
            error = $error,
            $( $key = $value, )*
        );
    };
}
```

**Replacement**: Direct `tracing::debug!` call with `result = "error"` field.

---

### 3. fuse_ok! (lines 41-48)

Logs a successful FUSE operation result.

```rust
macro_rules! fuse_ok {
    ($op:expr $(, $key:ident = $value:expr)* $(,)? ) => {
        ::tracing::debug!(
            fuse_op = $op,
            result = "success",
            $( $key = $value, )*
        );
    };
}
```

**Replacement**: Direct `tracing::debug!` call with `result = "success"` field.

---

### 4. reply_* Macros (lines 52-88)

These macros combine three operations:
1. Record error metric: `$metrics.record_error()`
2. Log error: `fuse_error!(...)`
3. Send FUSE error reply: `$reply.error(errno)`

**reply_ino_not_found!** (lines 52-58):
- Records error metric
- Logs "ENOENT" error
- Replies with `libc::ENOENT`

**reply_not_directory!** (lines 62-68):
- Records error metric  
- Logs "ENOTDIR" error
- Replies with `libc::ENOTDIR`

**reply_not_file!** (lines 72-78):
- Records error metric
- Logs "EISDIR" error  
- Replies with `libc::EISDIR`

**reply_no_permission!** (lines 82-88):
- Records error metric
- Logs "EACCES" error with reason
- Replies with `libc::EACCES`

**Replacement**: Each call site needs 3 explicit lines:
1. `self.metrics.record_error();`
2. `tracing::debug!(...)` with error details
3. `reply.error(libc::EXXX);`

---

## Usage Locations

### fuse_log! Calls (7 total)

All in `src/fs/filesystem.rs`:

| Line | Operation | Context |
|------|-----------|---------|
| 809 | read | Start of read operation |
| 869 | read | Before streaming data |
| 996 | lookup | Start of lookup |
| 1100 | lookup | Before returning entry |
| 1116 | getattr | Start of getattr |
| 1143 | open | Start of open |
| 1231 | readdir | Start of readdir |

---

### fuse_error! Direct Calls (4 total)

All in `src/fs/filesystem.rs`:

| Line | Operation | Error | Context |
|------|-----------|-------|---------|
| 814 | read | EINVAL | Negative offset |
| 826 | read | EBADF | Invalid file handle |
| 1157 | open | ELOOP | Symlink loop |
| 1184 | open | EMFILE | Handle limit reached |

---

### fuse_ok! Calls (7 total)

All in `src/fs/filesystem.rs`:

| Line | Operation | Context |
|------|-----------|---------|
| 854 | read | After successful read |
| 909 | read | After streaming data |
| 971 | release | After releasing handle |
| 1031 | readlink | After reading symlink |
| 1077 | lookup | After successful lookup |
| 1124 | getattr | After successful getattr |
| 1189 | open | After successful open |

---

### reply_* Calls (11 total)

All in `src/fs/filesystem.rs`:

| Line | Macro | Operation | Inode Parameter |
|------|-------|-----------|-----------------|
| 842 | reply_not_file! | read | ino |
| 847 | reply_ino_not_found! | read | ino |
| 1002 | reply_ino_not_found! | lookup | parent |
| 1009 | reply_not_directory! | lookup | parent |
| 1133 | reply_ino_not_found! | getattr | ino |
| 1150 | reply_not_file! | open | ino |
| 1165 | reply_no_permission! | open | ino |
| 1193 | reply_ino_not_found! | open | ino |
| 1214 | reply_ino_not_found! | readlink | ino |
| 1323 | reply_ino_not_found! | readdir | ino |
| 1330 | reply_not_directory! | readdir | ino |

---

## Replacement Strategy

### Phase 1: Replace fuse_log! (7 call sites)

Replace each `fuse_log!("op", key = value)` with:
```rust
tracing::debug!(fuse_op = "op", key = value);
```

### Phase 2: Replace fuse_error! direct calls (4 call sites)

Replace each `fuse_error!("op", "ERR", key = value)` with:
```rust
tracing::debug!(fuse_op = "op", result = "error", error = "ERR", key = value);
```

### Phase 3: Replace fuse_ok! (7 call sites)

Replace each `fuse_ok!("op", key = value)` with:
```rust
tracing::debug!(fuse_op = "op", result = "success", key = value);
```

### Phase 4: Replace reply_* macros (11 call sites)

Each reply_* macro call expands to 3 operations. Example replacement for `reply_ino_not_found!`:

```rust
// Before:
reply_ino_not_found!(self.metrics, reply, "op", ino);

// After:
self.metrics.record_error();
tracing::debug!(fuse_op = "op", result = "error", error = "ENOENT", ino = ino);
reply.error(libc::ENOENT);
```

Similar pattern for other reply_* macros with appropriate error codes.

### Phase 5: Remove macros.rs

After all call sites are replaced:
1. Delete `src/fs/macros.rs` file
2. Remove `mod macros;` from `src/fs/mod.rs`
3. Remove macro imports from `src/fs/filesystem.rs`

---

## Benefits of Replacement

1. **Simpler code**: No macro indirection
2. **Easier to debug**: Direct tracing calls are clearer
3. **Better IDE support**: Go-to-definition works properly
4. **Consistent style**: All logging uses same pattern
5. **Removes 98 lines**: Entire macros.rs file can be deleted
6. **No functional change**: Same behavior, just direct calls

---

## Risk Assessment

**Risk Level**: Very Low

- Macros only wrap tracing::debug! calls
- No complex logic or side effects
- Easy to verify replacements are correct
- Can be done incrementally (one macro type at a time)
- All replacements are mechanical transformations

---

## Estimated Effort

- **fuse_log!**: 5 minutes (7 simple replacements)
- **fuse_error! direct**: 3 minutes (4 simple replacements)  
- **fuse_ok!**: 5 minutes (7 simple replacements)
- **reply_* macros**: 20 minutes (11 sites × 3 lines each)
- **Remove macros.rs**: 2 minutes
- **Test and verify**: 10 minutes

**Total**: ~45 minutes

---

## References

- File: `src/fs/macros.rs` (98 lines, to be removed)
- File: `src/fs/filesystem.rs` (33 call sites to update)
- File: `src/fs/mod.rs` (remove `mod macros;` declaration)

---

*Generated for Task 6.1.1*
