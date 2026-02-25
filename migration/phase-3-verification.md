# Phase 3: Final Verification Checklist

This document provides a comprehensive checklist for final verification after removing the `status` command from rqbit-fuse.

## Overview

This phase ensures the migration is complete, clean, and ready for deployment. All items must be checked and signed off before considering the migration complete.

## Prerequisites

- [ ] Phase 1 (Code Removal) completed successfully
- [ ] Phase 2 (Testing) completed successfully
- [ ] All tests passing
- [ ] No compilation errors or warnings

---

## Section 1: Code Verification

### 1.1 Source Code Review

Check the following in `src/main.rs`:

- [ ] **Commands enum** contains only `Mount` and `Umount` variants (no `Status`)
  ```bash
  grep -A 20 "enum Commands" src/main.rs
  ```
  Expected: Only `Mount` and `Umount` variants, no `Status`

- [ ] **Match statement** in `main()` has only two arms (Mount and Umount)
  ```bash
  grep -A 30 "match cli.command" src/main.rs
  ```
  Expected: Two match arms, no `Commands::Status` handler

- [ ] **run_status() function** has been completely removed
  ```bash
  grep -n "run_status" src/main.rs
  ```
  Expected: No output (function not found)

- [ ] **No dead code** - All functions and imports are used
  ```bash
  nix-shell --run 'cargo clippy -- -W dead_code'
  ```
  Expected: No warnings about unused functions or imports

### 1.2 Import Verification

Verify all imports in `src/main.rs` are still needed:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use rqbit_fuse::config::{CliArgs, Config};
use rqbit_fuse::mount::{is_mount_point, setup_logging, unmount_filesystem};
use std::path::PathBuf;
```

- [ ] `is_mount_point` - Still used in `run_umount()` ✓
- [ ] `setup_logging` - Still used in `main()` for Mount command ✓
- [ ] `unmount_filesystem` - Still used in `run_umount()` ✓
- [ ] All other imports - Verified still in use ✓

**Verification command:**
```bash
nix-shell --run 'cargo clippy -- -W unused_imports'
```
Expected: No unused import warnings

---

## Section 2: Documentation Verification

### 2.1 Spec Documentation

The following spec files were already updated in commit `4ff33398b08c7ed4fe01ca4475d4cd2278fa5650`:

- [ ] `spec/architecture.md` - No status command references
- [ ] `spec/quickstart.md` - No status command references
- [ ] `spec/roadmap.md` - Status marked as removed

**Verification:**
```bash
grep -n "status" spec/architecture.md spec/quickstart.md spec/roadmap.md
```
Expected: Only references to HTTP status codes or "status" as a word in other contexts, NOT as a CLI command

### 2.2 Code Comments

- [ ] No comments referencing the status command
  ```bash
  grep -n "status" src/main.rs | grep -i "show\|display\|check"
  ```
  Expected: No references to status command functionality

- [ ] All remaining comments are accurate and up-to-date

---

## Section 3: CLI Behavior Verification

### 3.1 Help Output Verification

Test that help output is correct:

```bash
nix-shell --run 'cargo run -- --help'
```

**Expected Output:**
```
A FUSE filesystem for accessing torrents via rqbit

Usage: rqbit-fuse <COMMAND>

Commands:
  mount   Mount the torrent filesystem
  umount  Unmount the torrent filesystem
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

- [ ] Only 3 commands listed: mount, umount, help
- [ ] No "status" or "Status" entry
- [ ] Descriptions are accurate

### 3.2 Error Handling Verification

Test error message when status is attempted:

```bash
nix-shell --run 'cargo run -- status'
```

**Expected Output:**
```
error: unrecognized subcommand 'status'

Usage: rqbit-fuse <COMMAND>

For more information, try '--help'.
```

- [ ] Clear error message indicating "unrecognized subcommand"
- [ ] Helpful suggestion to use `--help`
- [ ] No stack trace or internal errors

### 3.3 Remaining Commands Verification

Verify mount and umount still work:

```bash
# Test mount help
nix-shell --run 'cargo run -- mount --help'
# Expected: Shows mount options, no errors

# Test umount help
nix-shell --run 'cargo run -- umount --help'
# Expected: Shows umount options, no errors
```

- [ ] Mount command help displays correctly
- [ ] Umount command help displays correctly
- [ ] All documented options are present

---

## Section 4: Build Quality Verification

### 4.1 Clean Build

- [ ] Project builds without warnings
  ```bash
  nix-shell --run 'cargo build 2>&1' | grep -i warning || echo "No warnings"
  ```
  Expected: "No warnings" or empty output

- [ ] Release build succeeds
  ```bash
  nix-shell --run 'cargo build --release'
  ```
  Expected: Clean build completion

### 4.2 Test Suite

- [ ] All unit tests pass
  ```bash
  nix-shell --run 'cargo test --lib'
  ```

- [ ] All integration tests pass
  ```bash
  nix-shell --run 'cargo test --test integration_tests'
  ```

- [ ] No test failures or errors

### 4.3 Code Quality

- [ ] Clippy passes with no warnings
  ```bash
  nix-shell --run 'cargo clippy -- -D warnings'
  ```
  Expected: Clean completion

- [ ] Code is properly formatted
  ```bash
  nix-shell --run 'cargo fmt --check'
  ```
  Expected: No output (no formatting issues)

- [ ] No compiler warnings
  ```bash
  nix-shell --run 'cargo build --all-targets 2>&1' | grep -i "warning:" || echo "No warnings"
  ```
  Expected: "No warnings"

---

## Section 5: Final Review

### 5.1 Change Summary

Document what was changed:

| Component | Change | Lines |
|-----------|--------|-------|
| `Commands` enum | Removed `Status` variant | -6 lines |
| `main()` match | Removed status arm | -1 line |
| `run_status()` | Removed entire function | -29 lines |
| **Total** | **Code removal** | **~36 lines** |

### 5.2 Breaking Changes

- [ ] This is a **breaking change** - the `status` command is no longer available
- [ ] Documented in migration guides
- [ ] Users must be informed of alternative methods (check mount with `mount` or `df` commands)

### 5.3 Alternative for Users

Document what users should do instead:

```bash
# Instead of: rqbit-fuse status
# Use standard Unix commands:

# Check if filesystem is mounted
mount | grep torrents

# Or
df -h | grep torrents

# Check mount point specifically
findmnt ~/torrents
```

---

## Sign-Off

### Developer Sign-Off

I certify that:
- [ ] All code changes have been reviewed
- [ ] All tests pass
- [ ] No compiler warnings or errors
- [ ] Documentation is accurate
- [ ] The migration is complete

**Developer Name:** _________________ **Date:** _________________

### Reviewer Sign-Off

I certify that:
- [ ] Code review completed
- [ ] All verification steps checked
- [ ] No issues found
- [ ] Approved for merge

**Reviewer Name:** _________________ **Date:** _________________

---

## Post-Migration Actions

After sign-off, complete these tasks:

- [ ] Update CHANGELOG.md with breaking change notice
- [ ] Update README.md if it references status command
- [ ] Create git tag for release
- [ ] Deploy to production
- [ ] Notify users of breaking change

---

## Quick Verification Command

Run this single command to verify everything:

```bash
#!/bin/bash
echo "=== Phase 3 Final Verification ==="
echo

echo "1. Checking for status command in code..."
if grep -q "Commands::Status\|run_status" src/main.rs; then
    echo "   ✗ FAILED: status references still in code"
    exit 1
else
    echo "   ✓ No status references in code"
fi

echo "2. Building project..."
nix-shell --run 'cargo build --all-targets 2>&1' | grep -i "warning:" && echo "   ✗ Build has warnings" || echo "   ✓ Clean build"

echo "3. Running tests..."
nix-shell --run 'cargo test' > /dev/null 2>&1 && echo "   ✓ All tests pass" || echo "   ✗ Tests failed"

echo "4. Running clippy..."
nix-shell --run 'cargo clippy -- -D warnings' > /dev/null 2>&1 && echo "   ✓ Clippy clean" || echo "   ✗ Clippy warnings"

echo "5. Checking formatting..."
nix-shell --run 'cargo fmt --check' > /dev/null 2>&1 && echo "   ✓ Code formatted" || echo "   ✗ Formatting issues"

echo "6. Verifying CLI..."
if nix-shell --run 'cargo run -- --help' 2>&1 | grep -q "status"; then
    echo "   ✗ FAILED: status in help"
    exit 1
else
    echo "   ✓ status not in help"
fi

echo
echo "=== Phase 3 Verification Complete ==="
```

---

*Last updated: 2026-02-24*
*Phase: 3 - Final Verification*
