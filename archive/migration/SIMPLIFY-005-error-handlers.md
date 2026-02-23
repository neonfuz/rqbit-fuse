# Migration Guide: SIMPLIFY-005 - Error Handler Helper Methods

**Task ID**: SIMPLIFY-005

**Status**: Ready for implementation

**Priority**: High

**Estimated Effort**: 30-45 minutes

**Expected Line Reduction**: ~100 lines

---

## Scope

**Files to Modify**:
- `src/fs/filesystem.rs` - Add helper methods and refactor existing error handling

**Dependencies**: None (can be done independently)

---

## Current State

The FUSE filesystem implementation in `src/fs/filesystem.rs` contains ~150 lines of duplicated error handling code across FUSE operation callbacks. Each error case follows the same 10-15 line pattern:

1. Record error metric
2. Log the error (if logging enabled)
3. Call `reply.error()` with appropriate libc error code
4. Return early

### Duplicated ENOENT Pattern (~40 lines across 6 locations)

```rust
// Example from lookup() (lines 1076-1091):
None => {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "lookup",
            parent = parent,
            result = "error",
            error = "ENOENT"
        );
    }

    reply.error(libc::ENOENT);
    return;
}

// Similar patterns in:
// - read() (lines 867-882)
// - getattr() (lines 1184-1197)
// - open() (lines 1277-1290)
// - readdir() (lines 1405-1418)
// - readlink() (lines 1309-1312)
```

### Duplicated ENOTDIR Pattern (~25 lines across 2 locations)

```rust
// Example from lookup() (lines 1093-1108):
if !parent_entry.is_directory() {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "lookup",
            parent = parent,
            result = "error",
            error = "ENOTDIR"
        );
    }

    reply.error(libc::ENOTDIR);
    return;
}

// Similar pattern in readdir() (lines 1422-1437)
```

### Duplicated EISDIR Pattern (~15 lines)

```rust
// From read() (lines 851-866):
_ => {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "read",
            ino = ino,
            result = "error",
            error = "EISDIR"
        );
    }

    reply.error(libc::EISDIR);
    return;
}
```

### Duplicated EACCES Pattern (~15 lines)

```rust
// From open() (lines 1250-1266):
if access_mode != libc::O_RDONLY {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "open",
            ino = ino,
            result = "error",
            error = "EACCES",
            reason = "write_access_requested"
        );
    }

    reply.error(libc::EACCES);
    return;
}
```

---

## Target State

Add helper methods to `TorrentFS` that encapsulate the common error handling pattern:

```rust
impl TorrentFS {
    /// Reply with ENOENT (inode not found)
    fn reply_ino_not_found(&self, reply: &mut impl Reply, op: &str, ino: u64) {
        self.metrics.fuse.record_error();

        if self.config.logging.log_fuse_operations {
            debug!(
                fuse_op = op,
                ino = ino,
                result = "error",
                error = "ENOENT"
            );
        }

        reply.error(libc::ENOENT);
    }

    /// Reply with ENOTDIR (not a directory)
    fn reply_not_directory(&self, reply: &mut impl Reply, op: &str, ino: u64) {
        self.metrics.fuse.record_error();

        if self.config.logging.log_fuse_operations {
            debug!(
                fuse_op = op,
                ino = ino,
                result = "error",
                error = "ENOTDIR"
            );
        }

        reply.error(libc::ENOTDIR);
    }

    /// Reply with EISDIR (is a directory, not a file)
    fn reply_not_file(&self, reply: &mut impl Reply, op: &str, ino: u64) {
        self.metrics.fuse.record_error();

        if self.config.logging.log_fuse_operations {
            debug!(
                fuse_op = op,
                ino = ino,
                result = "error",
                error = "EISDIR"
            );
        }

        reply.error(libc::EISDIR);
    }

    /// Reply with EACCES (permission denied)
    fn reply_no_permission(&self, reply: &mut impl Reply, op: &str, ino: u64, reason: &str) {
        self.metrics.fuse.record_error();

        if self.config.logging.log_fuse_operations {
            debug!(
                fuse_op = op,
                ino = ino,
                result = "error",
                error = "EACCES",
                reason = reason
            );
        }

        reply.error(libc::EACCES);
    }
}
```

### Refactored Usage Examples

**Before** (lookup, ~15 lines):
```rust
None => {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "lookup",
            parent = parent,
            result = "error",
            error = "ENOENT"
        );
    }

    reply.error(libc::ENOENT);
    return;
}
```

**After** (lookup, ~3 lines):
```rust
None => {
    self.reply_ino_not_found(&mut reply, "lookup", parent);
    return;
}
```

**Before** (read EISDIR, ~15 lines):
```rust
_ => {
    self.metrics.fuse.record_error();

    if self.config.logging.log_fuse_operations {
        debug!(
            fuse_op = "read",
            ino = ino,
            result = "error",
            error = "EISDIR"
        );
    }

    reply.error(libc::EISDIR);
    return;
}
```

**After** (read EISDIR, ~3 lines):
```rust
_ => {
    self.reply_not_file(&mut reply, "read", ino);
    return;
}
```

---

## Implementation Steps

1. **Add helper methods** (lines ~520-560 in `impl TorrentFS` block):
   - Add the four helper methods shown in Target State
   - Place them near other TorrentFS implementation methods
   - Import necessary traits: `fuser::Reply` (or use `&mut dyn Reply`)

2. **Refactor ENOENT usages** (~6 locations):
   - `lookup()`: Line ~1076 - parent lookup
   - `read()`: Line ~867 - inode lookup
   - `getattr()`: Line ~1184 - inode lookup
   - `open()`: Line ~1277 - inode lookup
   - `readdir()`: Line ~1405 - inode lookup
   - `readlink()`: Line ~1309 - inode lookup

3. **Refactor ENOTDIR usages** (2 locations):
   - `lookup()`: Line ~1093 - parent directory check
   - `readdir()`: Line ~1422 - directory check

4. **Refactor EISDIR usage** (1 location):
   - `read()`: Line ~851 - file entry check

5. **Refactor EACCES usage** (1 location):
   - `open()`: Line ~1250 - write access check

6. **Verify and test**:
   - Run `cargo check` to verify compilation
   - Run `cargo clippy` to check for warnings
   - Run `cargo fmt` to format code

---

## Testing

### Build Verification
```bash
cargo check
cargo clippy
cargo fmt
```

### Test Scenarios

1. **ENOENT scenarios**:
   ```bash
   # Try to access non-existent file
   ls /mount/point/nonexistent_file
   # Should return "No such file or directory"
   ```

2. **ENOTDIR scenarios**:
   ```bash
   # Try to list a file as if it's a directory
   ls /mount/point/some_torrent/file.txt/subdir
   # Should return "Not a directory"
   ```

3. **EISDIR scenarios**:
   ```bash
   # Try to read a directory as if it's a file
   cat /mount/point/some_torrent/
   # Should return "Is a directory"
   ```

4. **EACCES scenarios**:
   ```bash
   # Try to write to a file
   echo "test" > /mount/point/some_torrent/file.txt
   # Should return "Permission denied"
   ```

### Code Review Checklist

- [ ] All helper methods are properly typed
- [ ] Error metrics are still recorded
- [ ] Debug logging still works when enabled
- [ ] Correct libc error codes are used
- [ ] Early returns are preserved
- [ ] No behavioral changes (only refactoring)

---

## Expected Reduction

**Before**: ~150 lines of error handling code

**After**: ~40 lines (4 helper methods) + ~10 lines of calls = ~50 lines

**Net Reduction**: ~100 lines

**Benefits**:
1. **DRY Principle**: Error handling logic defined once, used everywhere
2. **Consistency**: All error responses follow the same pattern
3. **Maintainability**: Changes to error handling only need to be made in one place
4. **Readability**: FUSE operation callbacks are shorter and easier to follow
5. **Testability**: Error handling logic can be unit tested independently

---

## Notes

- The helper methods use `&mut impl Reply` to work with any reply type (ReplyEntry, ReplyData, ReplyAttr, etc.)
- The `op` parameter allows identifying which FUSE operation triggered the error
- The `reason` parameter in `reply_no_permission()` allows optional context for EACCES errors
- Consider adding more helper methods in the future for other common error codes (EAGAIN, EIO, EINVAL)

---

## Related Tasks

- ERROR-002: Replace string matching with typed errors
- FS-007: Add proper FUSE operation tests (error case testing)

*Migration guide created: February 14, 2026*
