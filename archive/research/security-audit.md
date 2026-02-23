# Security Audit Research

Date: 2026-02-14

## Summary

Created comprehensive security audit workflow using cargo-audit and cargo-deny to continuously monitor dependencies for security vulnerabilities and license compliance.

## Tools Implemented

### 1. cargo-audit
- **Purpose**: Scans dependencies for known security vulnerabilities
- **Data Source**: RustSec Advisory Database (https://rustsec.org/)
- **Trigger**: Runs on every push/PR to main branch and daily via cron schedule
- **GitHub Action**: rustsec/audit-check@v1.4.1

### 2. cargo-deny
- **Purpose**: Comprehensive dependency checking including:
  - License compliance validation
  - Security advisory checking
  - Duplicate dependency detection
  - Banned crate detection
- **Configuration**: `deny.toml` in project root
- **GitHub Action**: EmbarkStudios/cargo-deny-action@v2

## Workflow File

Location: `.github/workflows/security-audit.yml`

Features:
- Runs on Ubuntu-latest
- Triggers on push/PR to main/master branches
- Daily cron job at midnight UTC (0 0 * * *)
- Uses GitHub Actions for both tools (faster than installing)
- Colored output enabled for better readability

## Configuration (deny.toml)

### Allowed Licenses
- MIT
- Apache-2.0
- Apache-2.0 WITH LLVM-exception
- BSD-2-Clause
- BSD-3-Clause
- ISC
- MPL-2.0
- Unicode-DFS-2016
- Unicode-3.0
- OpenSSL

### Security Advisories
- Ignored: RUSTSEC-2025-0134 (rustls-pemfile unmaintained)
  - Reason: Transitive dependency via reqwest, no safe upgrade available yet

### Ban Policy
- Multiple versions warning only (common in large dependency trees)
- No crates currently banned

## Current Security Status

### Findings
1. **RUSTSEC-2025-0134**: rustls-pemfile is unmaintained
   - Severity: Informational
   - Impact: Low (repository archived, functionality moved to rustls-pki-types)
   - Status: Ignored in cargo-deny, monitored via advisory database
   - Path: reqwest -> rustls-pemfile

2. **RUSTSEC-2021-0154**: fuser unsound memory issue
   - Severity: Unsound
   - Impact: Potential uninitialized memory read
   - Status: Currently no fix available upstream
   - Note: This affects the fuser crate we use for FUSE filesystem

### License Compliance
✅ All dependency licenses are in the allow list
✅ No license conflicts detected

### Duplicate Dependencies
⚠️ Multiple versions of several crates (expected due to dependency tree complexity):
- bitflags (2 versions)
- core-foundation (2 versions)
- getrandom (2 versions)
- hashbrown (2 versions)
- socket2 (2 versions)
- windows-sys (4 versions)
- windows-targets (3 versions)
- Various windows platform crates (3 versions each)

These duplicates are common in Rust projects and generally don't cause issues, but can increase compile times and binary size.

## Recommendations

1. **Monitor RUSTSEC-2025-0134**: Update reqwest when a new version is available that doesn't depend on rustls-pemfile
2. **Consider fuser alternatives**: Evaluate if the fuser unsound issue affects production use (likely low impact for read-only filesystem)
3. **Regular updates**: Run `cargo update` periodically to get latest patch versions
4. **Review duplicate dependencies**: Consider if any duplicates can be resolved by updating transitive dependencies

## Integration

The security audit workflow is integrated with:
- CI pipeline (runs alongside tests)
- Release workflow (advisories checked before release)
- Daily monitoring (catches new advisories quickly)

## References

- [RustSec Advisory Database](https://rustsec.org/advisories/)
- [cargo-audit documentation](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
- [cargo-deny documentation](https://embarkstudios.github.io/cargo-deny/)
- [GitHub Action: cargo-audit](https://github.com/rustsec/audit-check)
- [GitHub Action: cargo-deny](https://github.com/EmbarkStudios/cargo-deny-action)
