# TODO.md - Status Command Removal Migration Checklist

This checklist tracks the migration to remove the `status` command from rqbit-fuse to match the spec documentation updates (commit 4ff33398b08c7ed4fe01ca4475d4cd2278fa5650).

## Migration Overview

**Goal:** Remove the `status` command implementation from src/main.rs to align with the updated spec documentation.

**Estimated Time:** 2-3 hours total
**Phases:** 3 phases with 14 tasks

---

## Phase 1: Code Removal

**Reference:** `migration/phase-1-code-removal.md`
**Estimated Time:** 30 minutes

### Task 1.1: Remove Status Enum Variant
- [ ] **Time:** 10 minutes
- [ ] Open `src/main.rs` at line 64
- [ ] Remove the `Status` variant from the `Commands` enum (lines 64-69)
- [ ] Verify with `nix-shell --run 'cargo check'`
- **References:** 
  - `migration/phase-1-code-removal.md` - Step 1
  - `spec/architecture.md` - CLI section (lines 122-126)

### Task 1.2: Remove Status Match Arm
- [ ] **Time:** 5 minutes
- [ ] Open `src/main.rs` at line 94
- [ ] Remove the `Commands::Status` match arm
- [ ] Verify with `nix-shell --run 'cargo check'`
- **References:**
  - `migration/phase-1-code-removal.md` - Step 2

### Task 1.3: Remove run_status() Function
- [ ] **Time:** 10 minutes
- [ ] Open `src/main.rs` at line 173
- [ ] Remove the entire `run_status()` function (lines 173-201)
- [ ] Verify with `nix-shell --run 'cargo build'`
- **References:**
  - `migration/phase-1-code-removal.md` - Step 3

### Task 1.4: Verify No Import Cleanup Needed
- [ ] **Time:** 5 minutes
- [ ] Review imports at top of `src/main.rs`
- [ ] Confirm `is_mount_point` is still used in `run_umount()`
- [ ] Run `nix-shell --run 'cargo clippy'` to check for unused imports
- **References:**
  - `migration/phase-1-code-removal.md` - Step 4

---

## Phase 2: Testing & Validation

**Reference:** `migration/phase-2-testing.md`
**Estimated Time:** 45 minutes

### Task 2.1: Build Verification
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo build'`
- [ ] Confirm clean build with no errors
- [ ] Check for any warnings
- **References:**
  - `migration/phase-2-testing.md` - Section 1

### Task 2.2: Run Full Test Suite
- [ ] **Time:** 15 minutes
- [ ] Run `nix-shell --run 'cargo test'`
- [ ] Verify all tests pass (0 failures)
- [ ] Note any test failures related to CLI changes
- **References:**
  - `migration/phase-2-testing.md` - Section 2

### Task 2.3: Run Clippy Lints
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo clippy'`
- [ ] Fix any warnings about unused code or imports
- [ ] Run `nix-shell --run 'cargo clippy -- -D warnings'` for strict check
- **References:**
  - `migration/phase-2-testing.md` - Section 3

### Task 2.4: Check Code Formatting
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo fmt --check'`
- [ ] If issues found, run `nix-shell --run 'cargo fmt'`
- [ ] Verify no formatting issues remain
- **References:**
  - `migration/phase-2-testing.md` - Section 4

### Task 2.5: CLI Help Verification
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo run -- --help'`
- [ ] Verify "status" does NOT appear in command list
- [ ] Confirm only "mount", "umount", and "help" are shown
- **References:**
  - `migration/phase-2-testing.md` - Section 5.1

### Task 2.6: CLI Status Command Rejection
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo run -- status'`
- [ ] Verify error message: "unrecognized subcommand 'status'"
- [ ] Confirm helpful suggestion to use --help
- **References:**
  - `migration/phase-2-testing.md` - Section 5.2

### Task 2.7: Verify Mount/Umount Still Work
- [ ] **Time:** 5 minutes
- [ ] Run `nix-shell --run 'cargo run -- mount --help'`
- [ ] Verify mount help displays correctly
- [ ] Run `nix-shell --run 'cargo run -- umount --help'`
- [ ] Verify umount help displays correctly
- **References:**
  - `migration/phase-2-testing.md` - Sections 5.3, 5.4

---

## Phase 3: Final Verification & Documentation

**Reference:** `migration/phase-3-verification.md`
**Estimated Time:** 30 minutes

### Task 3.1: Code Review Checklist
- [ ] **Time:** 10 minutes
- [ ] Verify no `Commands::Status` references in `src/main.rs`
- [ ] Verify no `run_status` function references
- [ ] Confirm match statement only has Mount and Umount arms
- [ ] Check for any remaining status-related comments
- **References:**
  - `migration/phase-3-verification.md` - Section 1

### Task 3.2: Spec Documentation Review
- [ ] **Time:** 5 minutes
- [ ] Verify `spec/architecture.md` has no status command references
- [ ] Verify `spec/quickstart.md` has no status command references
- [ ] Verify `spec/roadmap.md` shows status as removed
- **References:**
  - `migration/phase-3-verification.md` - Section 2
  - `spec/architecture.md`
  - `spec/quickstart.md`
  - `spec/roadmap.md`

### Task 3.3: Final Build Quality Check
- [ ] **Time:** 10 minutes
- [ ] Run `nix-shell --run 'cargo build --release'`
- [ ] Run `nix-shell --run 'cargo test --all-targets'`
- [ ] Run `nix-shell --run 'cargo clippy --all-targets -- -D warnings'`
- [ ] Confirm zero warnings, zero errors
- **References:**
  - `migration/phase-3-verification.md` - Section 4

### Task 3.4: Update CHANGELOG.md
- [ ] **Time:** 5 minutes
- [ ] Add entry for breaking change (status command removal)
- [ ] Reference commit 4ff33398b08c7ed4fe01ca4475d4cd2278fa5650
- [ ] Document alternative commands for users
- **References:**
  - `migration/phase-3-verification.md` - Section 5.2

---

## Quick Reference Commands

### Build & Test
```bash
nix-shell --run 'cargo build'
nix-shell --run 'cargo test'
nix-shell --run 'cargo clippy'
nix-shell --run 'cargo fmt'
```

### Verification
```bash
# Check status removed
nix-shell --run 'cargo run -- --help' | grep status

# Should fail
nix-shell --run 'cargo run -- status'

# Should work
nix-shell --run 'cargo run -- mount --help'
nix-shell --run 'cargo run -- umount --help'
```

### Code Search
```bash
# Find any remaining status references
grep -n "Commands::Status\|run_status" src/main.rs

# Check spec files
grep -n "status" spec/architecture.md spec/quickstart.md spec/roadmap.md
```

---

## Migration Documents

- `migration/phase-1-code-removal.md` - Detailed code removal steps
- `migration/phase-2-testing.md` - Testing and validation procedures
- `migration/phase-3-verification.md` - Final verification checklist

## Spec Documents

- `spec/architecture.md` - Architecture documentation (already updated)
- `spec/quickstart.md` - User quickstart guide (already updated)
- `spec/roadmap.md` - Development roadmap (already updated)

---

## Success Criteria

- [ ] All Phase 1 tasks complete (code removed)
- [ ] All Phase 2 tasks complete (tests pass)
- [ ] All Phase 3 tasks complete (verified)
- [ ] No compiler warnings or errors
- [ ] `status` command no longer recognized
- [ ] `mount` and `umount` commands still work
- [ ] CHANGELOG.md updated

---

## Notes

- The spec documentation was already updated in commit 4ff33398b08c7ed4fe01ca4475d4cd2278fa5650
- This migration removes the implementation to match the spec
- This is a **breaking change** - users can no longer use `rqbit-fuse status`
- Users should use standard Unix commands instead:
  - `mount | grep torrents` - Check if mounted
  - `df -h | grep torrents` - Check mount status
  - `findmnt ~/torrents` - Check specific mount point

---

Last Updated: 2026-02-24
Migration: Remove Status Command
