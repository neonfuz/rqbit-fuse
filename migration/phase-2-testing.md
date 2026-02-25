# Phase 2 Testing Guide: Status Command Removal

This guide provides detailed instructions for testing the removal of the `status` command from rqbit-fuse.

## Prerequisites

Before running these tests, ensure you have:
- Nix shell environment set up
- The code changes removing the `status` command have been applied
- A clean working directory

## Testing Steps

### 1. Build the Project

**Command:**
```bash
nix-shell --run 'cargo build'
```

**Expected Output:**
```
   Compiling rqbit-fuse v0.1.0 (/home/opencode/code/torrent-fuse)
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

**Interpretation:**
- ✅ **Success:** The project compiles without errors
- ❌ **Failure:** Any compilation errors indicate the removal was incomplete or introduced syntax errors

**Troubleshooting:**
- If you see errors about missing `Commands::Status`, ensure all references to the Status variant have been removed from:
  - The `Commands` enum definition
  - The `match cli.command` statement in `main()`
  - Any helper functions like `run_status()`

---

### 2. Run All Tests

**Command:**
```bash
nix-shell --run 'cargo test'
```

**Expected Output:**
```
   Compiling rqbit-fuse v0.1.0 (/home/opencode/code/torrent-fuse)
    Finished test [unoptimized + debuginfo] target(s) in X.XXs
     Running unittests src/lib.rs (target/debug/deps/...)

running XX tests
test result: ok. XX passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/integration_tests.rs (target/debug/deps/...)

running XX tests
test result: ok. XX passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/performance_tests.rs (target/debug/deps/...)

running XX tests
test result: ok. XX passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Interpretation:**
- ✅ **Success:** All tests pass (0 failed)
- ❌ **Failure:** Any test failures indicate the removal broke existing functionality

**Troubleshooting:**
- If tests related to CLI parsing fail, check if any tests reference the `status` command
- Look for test files that might import or test the Commands enum

---

### 3. Run Clippy Lints

**Command:**
```bash
nix-shell --run 'cargo clippy'
```

**Expected Output:**
```
    Checking rqbit-fuse v0.1.0 (/home/opencode/code/torrent-fuse)
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

**Interpretation:**
- ✅ **Success:** No warnings or errors (clean output)
- ⚠️ **Warnings:** Non-blocking but should be reviewed
- ❌ **Errors:** Must be fixed before proceeding

**Common Issues:**
- **Unused imports:** If `run_status` or related functions were removed but imports remain
- **Dead code:** Helper functions only used by `status` command that weren't removed

**Troubleshooting:**
```bash
# Run with more verbose output
nix-shell --run 'cargo clippy -- -W clippy::all'

# Auto-fix some issues
nix-shell --run 'cargo clippy --fix'
```

---

### 4. Check Code Formatting

**Command:**
```bash
nix-shell --run 'cargo fmt --check'
```

**Expected Output:**
```
# No output indicates formatting is correct
```

**Interpretation:**
- ✅ **Success:** No output means all files are properly formatted
- ❌ **Failure:** Lists files that need formatting

**If formatting issues found:**
```bash
nix-shell --run 'cargo fmt'
```

---

### 5. CLI Verification Tests

These tests verify that the `status` command has been properly removed and other commands still work.

#### 5.1 Verify Status Command Removed from Help

**Command:**
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

**Verification:**
- ✅ **Success:** Help output shows only `mount`, `umount`, and `help` commands
- ❌ **Failure:** If `status` still appears in the list, the command wasn't fully removed

**What to check:**
- [ ] No `status` or `Status` entry in the Commands list
- [ ] Only 3 commands listed: mount, umount, help

---

#### 5.2 Verify Status Command Fails

**Command:**
```bash
nix-shell --run 'cargo run -- status'
```

**Expected Output:**
```
error: unrecognized subcommand 'status'

Usage: rqbit-fuse <COMMAND>

For more information, try '--help'.
```

**Verification:**
- ✅ **Success:** Error message indicates "unrecognized subcommand"
- ❌ **Failure:** If the command executes or shows a different error, the removal was incomplete

**Alternative error format (depending on clap version):**
```
error: Found argument 'status' which wasn't expected, or isn't valid in this context

USAGE:
    rqbit-fuse <SUBCOMMAND>

For more information try --help
```

---

#### 5.3 Verify Mount Command Still Works

**Command:**
```bash
nix-shell --run 'cargo run -- mount --help'
```

**Expected Output:**
```
Mount the torrent filesystem

Usage: rqbit-fuse mount [OPTIONS]

Options:
  -m, --mount-point <MOUNT_POINT>  Path to mount point (overrides config)
  -u, --api-url <API_URL>          rqbit API URL (overrides config)
  -c, --config <FILE>              Path to config file
      --username <USERNAME>        rqbit API username for HTTP Basic Auth (overrides config)
      --password <PASSWORD>        rqbit API password for HTTP Basic Auth (overrides config)
  -v, --verbose...                 Increase verbosity (can be used multiple times)
  -q, --quiet                      Suppress all output except errors
  -h, --help                       Print help
```

**Verification:**
- ✅ **Success:** Help displays correctly with all mount options
- ❌ **Failure:** Missing options or errors indicate collateral damage from removal

---

#### 5.4 Verify Umount Command Still Works

**Command:**
```bash
nix-shell --run 'cargo run -- umount --help'
```

**Expected Output:**
```
Unmount the torrent filesystem

Usage: rqbit-fuse umount [OPTIONS]

Options:
  -m, --mount-point <MOUNT_POINT>  Path to mount point (overrides config)
  -c, --config <FILE>              Path to config file
  -f, --force                      Force unmount even if filesystem is busy
  -h, --help                       Print help
```

**Verification:**
- ✅ **Success:** Help displays correctly with all umount options
- ❌ **Failure:** Missing options or errors indicate collateral damage from removal

---

## Quick Verification Script

Run this script to perform all CLI verification tests at once:

```bash
#!/bin/bash
set -e

echo "=== Phase 2 Testing: Status Command Removal ==="
echo

echo "1. Building project..."
nix-shell --run 'cargo build' > /dev/null 2>&1
echo "   ✓ Build successful"
echo

echo "2. Running tests..."
nix-shell --run 'cargo test' > /dev/null 2>&1
echo "   ✓ All tests passed"
echo

echo "3. Running clippy..."
nix-shell --run 'cargo clippy' > /dev/null 2>&1
echo "   ✓ No linting errors"
echo

echo "4. Checking formatting..."
nix-shell --run 'cargo fmt --check' > /dev/null 2>&1
echo "   ✓ Code is properly formatted"
echo

echo "5. Verifying status command removal..."

# Test: status should not appear in help
if nix-shell --run 'cargo run -- --help' 2>&1 | grep -q "status"; then
    echo "   ✗ FAILED: status still appears in help"
    exit 1
else
    echo "   ✓ status not in help output"
fi

# Test: status command should fail
if nix-shell --run 'cargo run -- status' 2>&1 | grep -q "unrecognized subcommand\|Found argument.*status"; then
    echo "   ✓ status command properly rejected"
else
    echo "   ✗ FAILED: status command not properly rejected"
    exit 1
fi

# Test: mount help works
if nix-shell --run 'cargo run -- mount --help' 2>&1 | grep -q "Mount the torrent filesystem"; then
    echo "   ✓ mount command works"
else
    echo "   ✗ FAILED: mount command broken"
    exit 1
fi

# Test: umount help works
if nix-shell --run 'cargo run -- umount --help' 2>&1 | grep -q "Unmount the torrent filesystem"; then
    echo "   ✓ umount command works"
else
    echo "   ✗ FAILED: umount command broken"
    exit 1
fi

echo
echo "=== All Phase 2 tests passed! ==="
```

Save this as `test_phase2.sh` and run:
```bash
chmod +x test_phase2.sh
./test_phase2.sh
```

---

## Summary Checklist

Before marking Phase 2 as complete, verify:

- [ ] `cargo build` completes without errors
- [ ] `cargo test` shows all tests passing
- [ ] `cargo clippy` shows no errors
- [ ] `cargo fmt --check` shows no formatting issues
- [ ] `rqbit-fuse --help` does NOT list `status` command
- [ ] `rqbit-fuse status` fails with "unrecognized subcommand" error
- [ ] `rqbit-fuse mount --help` displays correctly
- [ ] `rqbit-fuse umount --help` displays correctly

---

## Troubleshooting Common Issues

### Issue: Build fails with "unresolved import" or "not found" errors

**Cause:** References to status-related types or functions still exist

**Solution:**
1. Search for remaining references:
   ```bash
   grep -r "Status" src/
   grep -r "run_status" src/
   ```
2. Remove all references including:
   - Enum variants
   - Match arms
   - Function definitions
   - Unused imports

### Issue: Tests fail after removal

**Cause:** Tests may reference the removed command

**Solution:**
1. Find test files referencing status:
   ```bash
   grep -r "status" tests/
   ```
2. Update or remove affected tests

### Issue: Clippy warns about unused code

**Cause:** Helper functions only used by status command remain

**Solution:**
1. Review warnings carefully
2. Remove truly unused functions
3. If functions might be useful later, mark with `#[allow(dead_code)]`

### Issue: Status command still works

**Cause:** Binary from previous build is being used

**Solution:**
```bash
# Force a clean rebuild
nix-shell --run 'cargo clean'
nix-shell --run 'cargo build'
```

---

## Next Steps

After Phase 2 testing is complete:

1. Update documentation to remove status command references
2. Update CHANGELOG.md with the breaking change
3. Proceed to Phase 3 (if applicable)

---

*Last updated: 2026-02-24*
*Phase: 2 - Status Command Removal Verification*
