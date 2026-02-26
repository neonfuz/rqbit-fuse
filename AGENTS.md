# AGENTS.md - AI Agent Guidelines

project name: rqbit-fuse

## Key Documents

Each document folder has a README.md which explains it's contents. Read appropriate documents when you need to understand some aspect of the application.

### 1. Specification (`spec/`)
The specification directory contains detailed technical documentation for the project. This is your primary reference for understanding the system.

### 2. User Guide (`usage/`)
This is a less detailed guide for users. It defines 

### 3. Reference (`reference/`)
This is **external documentation** that should **NOT be modified**. These are third-party specifications, guides, and reference materials. The rqbit HTTP api which rqbit-fuse communicates with is documented here.

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

## Important Notes

- Create temporary files in `.tmp/` directory (not `/tmp`)
- **Always use nix-shell** for build, test, development, etc operations. The code requires specific dependencies and environment variables that are only available within the nix-shell. Example: `nix-shell --run 'cargo build'`

---
