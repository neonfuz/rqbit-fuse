# AGENTS.md - AI Agent Guidelines

This file helps AI agents understand and navigate the rqbit-fuse project.

## Key Documents

### 1. Specification (`spec/README.md`)

The specification directory contains detailed technical documentation for the project. This is your primary reference for understanding the system.

**What it contains:**
- Architecture overview (`architecture.md`)
- API documentation (`api.md`)
- Technical design details (`technical-design.md`)
- Development roadmap (`roadmap.md`)
- User quickstart guide (`quickstart.md`)

**When to use it:**
- Before implementing any feature, read the relevant specification
- Reference `architecture.md` for high-level design questions
- Reference `api.md` when working with the rqbit HTTP API
- Reference `technical-design.md` for implementation details

**Reading order:**
1. Start with `spec/architecture.md` for the big picture
2. Read `spec/technical-design.md` for implementation details
3. Reference `spec/api.md` during API client development
4. Check `spec/quickstart.md` to understand user workflows

---

### 2. Tasks (`TODO.md`)

This is the prioritized task list for implementing the project.

---

### 3. Reference (`reference/`)

The reference directory contains **external documentation** that should **NOT be modified**. These are third-party specifications, guides, and reference materials.

**What it contains:**
- Third-party API specifications (e.g., rqbit streaming API)
- External guides and playbooks (e.g., Ralph workflow documentation)
- Reference materials from outside sources

**Important:**
- **DO NOT MODIFY** files in this directory
- These are external resources for reference only
- If updates are needed, create new files in `spec/` instead

**What it contains:**
- Phase-by-phase implementation plan
- Detailed task breakdowns with specific requirements
- Testing and quality assurance tasks
- Completed/discovery sections for tracking progress

**When to use it:**
- Check current phase before starting work
- Understand task dependencies and prerequisites
- Mark tasks as completed when finished
- Add discovered issues during implementation

---

## Agent Workflow

### Starting a new task:
1. Read `spec/README.md` to understand what spec documents apply
2. Read the relevant spec files (architecture, api, etc.)
3. Check `TODO.md` for the current phase and task
4. Review any discovered issues or notes

### During implementation:
1. Follow the technical design in the spec files
2. Reference API documentation when implementing API calls
3. Add new issues to the "Discovered Issues" section in TODO.md
4. Update the spec with any new designs implemented and fix any mistakes in the spec
5. Update "Completed" section as tasks finish

### Before submitting changes:
1. Run tests: `nix-shell --run 'cargo test'`
2. Run linting: `nix-shell --run 'cargo clippy'`
3. Format code: `nix-shell --run 'cargo fmt'`
4. Mark relevant tasks as completed in TODO.md

## Building

**ALWAYS use nix-shell for all build, test, and development operations.**

This project uses Nix for dependency management. The code will not compile correctly without the proper environment provided by nix-shell.

```bash
# Build the project
nix-shell --run 'cargo build'

# Build with release optimizations
nix-shell --run 'cargo build --release'

# Run tests
nix-shell --run 'cargo test'

# Run linting
nix-shell --run 'cargo clippy'

# Format code
nix-shell --run 'cargo fmt'
```

## Testing Guidelines

### Test Categories

1. **Boundary Tests**: EOF, offsets, capacity limits
2. **Resource Tests**: Handle limits, stream limits, inode limits
3. **Error Tests**: Network failures, invalid inputs, edge responses
4. **Concurrency Tests**: Race conditions, simultaneous operations
5. **Unicode Tests**: Special characters, normalization, encoding
6. **Configuration Tests**: Invalid configs, edge values

### Running Tests

**Always run tests within nix-shell:**

```bash
# Run all tests
nix-shell --run 'cargo test'

# Run all edge case tests
nix-shell --run 'cargo test edge_'

# Run specific category
nix-shell --run 'cargo test edge_001'
nix-shell --run 'cargo test edge_006'  # File handle tests
nix-shell --run 'cargo test edge_021'  # Streaming tests

# Run with output
nix-shell --run 'cargo test edge_ -- --nocapture'
```

### Completion Criteria

Each test should:
- Be isolated (no dependencies on other tests)
- Run quickly (< 1 second per test)
- Cover both success and failure paths
- Include assertions for all outcomes
- Pass `nix-shell --run 'cargo test'`
- Pass `nix-shell --run 'cargo clippy'`
- Be formatted with `nix-shell --run 'cargo fmt'`
- Have checkbox marked as complete

## Important Notes

- Create temporary files in `.tmp/` directory (not `/tmp`)
- Treat `src/lib/` as shared utilities
- The project uses Rust with async (tokio), FUSE (fuser), HTTP (reqwest)
- FUSE operations require careful error mapping (ENOENT, EACCES, EIO, etc.)
- HTTP Range requests are used for reading torrent data
- Cache implementation uses TTL and LRU eviction
- **Always use nix-shell** - the code requires specific dependencies and environment variables that are only available within the nix-shell

---
