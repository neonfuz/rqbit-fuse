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

## Important Notes

- Create temporary files in `.tmp/` directory (not `/tmp`)
- **Always use nix-shell** for build, test, development, etc operations. The code requires specific dependencies and environment variables that are only available within the nix-shell. Example: `nix-shell --run 'cargo build'`

---
