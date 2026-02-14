# Agents.md - AI Agent Guidelines

This file helps AI agents understand and navigate the torrent-fuse project.

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

### 2. Tasks (`TASKS.md`)

This is the prioritized task list for implementing the project. Tasks are organized into 8 phases and ordered by dependency.

**What it contains:**
- Phase-by-phase implementation plan (8 phases total)
- Detailed task breakdowns with specific requirements
- Testing and quality assurance tasks
- Completed/discovery sections for tracking progress

**When to use it:**
- Check current phase before starting work
- Understand task dependencies and prerequisites
- Mark tasks as completed when finished
- Add discovered issues during implementation

**Task phases:**
1. **Foundation & Setup** - Project structure, data structures, API client
2. **FUSE Filesystem Core** - Inode management, FUSE callbacks
3. **Read Operations & Caching** - File reading, cache layer, read-ahead
4. **Torrent Lifecycle** - Adding/monitoring/removing torrents
5. **Error Handling** - Error mapping, edge cases, graceful degradation
6. **CLI & UX** - Command-line interface, logging, documentation
7. **Testing & Quality** - Unit tests, integration tests, CI/CD
8. **Polish & Release** - Security review, optimization, final docs

---

## Agent Workflow

### Starting a new task:
1. Read `spec/README.md` to understand what spec documents apply
2. Read the relevant spec files (architecture, api, etc.)
3. Check `TASKS.md` for the current phase and task
4. Review any discovered issues or notes

### During implementation:
1. Follow the technical design in the spec files
2. Reference API documentation when implementing API calls
3. Add new issues to the "Discovered Issues" section in `TASKS.md`
4. Update "Completed" section as tasks finish

### Before submitting changes:
1. Run tests: `cargo test`
2. Run linting: `cargo clippy`
3. Format code: `cargo fmt`
4. Mark relevant tasks as completed in `TASKS.md`

## Important Notes

- Treat `src/lib/` as shared utilities
- The project uses Rust with async (tokio), FUSE (fuser), HTTP (reqwest)
- FUSE operations require careful error mapping (ENOENT, EACCES, EIO, etc.)
- HTTP Range requests are used for reading torrent data
- Cache implementation uses TTL and LRU eviction
- All specs are complete and ready for implementation

---

## Docker Development (Linux on macOS)

Since this is a FUSE-based project with platform-specific differences, you may need to build and test the Linux version while developing on macOS.

### Building the Docker Image

```bash
docker build -t torrent-fuse-dev .
```

### Running Tests in Docker

```bash
# Run all tests
docker run --rm -v "$(pwd):/app" torrent-fuse-dev

# Run with cargo watch for continuous testing
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo watch -x test

# Run specific test
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo test <test_name>
```

### Building the Project

```bash
# Build release binary for Linux
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo build --release

# The binary will be in target/release/torrent-fuse (Linux ELF format)
```

### Running Clippy and Format

```bash
# Run linter
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo clippy

# Format code
docker run --rm -v "$(pwd):/app" torrent-fuse-dev cargo fmt
```

### Interactive Development Shell

```bash
# Get a bash shell inside the container
docker run --rm -it -v "$(pwd):/app" torrent-fuse-dev bash

# Then run cargo commands as usual:
# cargo test
# cargo build
# cargo clippy
```

### Notes

- The project directory is mounted as a volume, so changes are reflected immediately
- The Docker image includes `libfuse-dev` for Linux FUSE development
- Target directory is shared between host and container (may cause issues if switching between macOS and Linux builds)
- For clean builds, consider using `docker run --rm -v "$(pwd):/app" -v /app/target torrent-fuse-dev cargo build`
