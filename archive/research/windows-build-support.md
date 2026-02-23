# Windows Build Support Research

## Date: 2026-02-13

## Objective
Research requirements for adding Windows build support to the release workflow.

## Current State
The project currently builds for:
- Linux (x86_64-gnu, x86_64-musl, aarch64)
- macOS (x86_64, aarch64)

Windows support is missing.

## Research Findings

### 1. FUSE on Windows

Windows doesn't have native FUSE support. The primary implementations are:

#### WinFsp (Windows File System Proxy)
- **Website**: https://winfsp.dev/
- **License**: GPLv3 / Commercial license available
- **Description**: High-quality FUSE-compatible file system driver for Windows
- **Requirements**: WinFsp must be installed on the target system
- **API Compatibility**: Compatible with FUSE API, but requires specific headers

#### Dokany (Dokan)
- **Website**: https://dokan-dev.github.io/
- **License**: LGPL/MIT
- **Description**: User-mode file system library for Windows
- **Less compatible** with standard FUSE API than WinFsp

### 2. GitHub Actions Windows Support

#### Available Runners
- `windows-latest` (Windows Server 2022)
- `windows-2019`

#### Installing WinFsp in CI
```yaml
- name: Install WinFsp
  run: |
    choco install winfsp
```

Or download directly:
```yaml
- name: Download and Install WinFsp
  run: |
    Invoke-WebRequest -Uri "https://github.com/winfsp/winfsp/releases/download/v2.0/winfsp-2.0.23075.msi" -OutFile "winfsp.msi"
    msiexec /i winfsp.msi /qn
```

### 3. Rust FUSE on Windows

The `fuser` crate (currently used) doesn't support Windows natively.

#### Options:

**Option A: Use WinFsp Rust bindings**
- Crate: `winfsp` or `winfsp-sys`
- Requires significant code changes
- Better native Windows support

**Option B: Use Dokan Rust bindings**
- Crate: `dokan` or `dokan-sys`
- Different API than FUSE
- Requires substantial refactoring

**Option C: Disable Windows builds**
- Document that Windows is not supported
- Suggest WSL2 as alternative
- Requires no code changes

### 4. Cross-Compilation Challenges

Cross-compiling from Linux to Windows:
- Requires MinGW-w64 toolchain
- Complex setup for FUSE dependencies
- Not recommended for this project

Native Windows builds:
- Use `windows-latest` runner
- Install Rust with `dtolnay/rust-toolchain`
- Install WinFsp or Dokan
- Build normally with `cargo build --release`

### 5. Recommendation

**Short-term**: Document that Windows is not currently supported, recommend WSL2.

**Long-term**: Evaluate WinFsp Rust bindings as a separate feature to add native Windows support. This would require:
1. Feature flags for different FUSE backends
2. Platform-specific code paths
3. Significant testing on Windows

## Conclusion

Adding Windows support requires either:
1. **Major refactoring** to support WinFsp/Dokan APIs
2. **Conditional compilation** to disable FUSE on Windows (non-functional build)
3. **Documentation update** noting Windows is unsupported

The current recommendation is to update documentation to clarify Windows is not supported, as full Windows support would require substantial architectural changes beyond the scope of the release workflow.
