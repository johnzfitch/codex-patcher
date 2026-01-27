# <img src="assets/icons/shield-security-protection-16x16.png" width="20" height="20" alt=""/> Security Policy

## <img src="assets/icons/tick.png" width="16" height="16" alt=""/> Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## <img src="assets/icons/warning-16x16.png" width="16" height="16" alt=""/> Reporting a Vulnerability

If you discover a security vulnerability in Codex Patcher, please report it responsibly:

### Do NOT

- Open a public GitHub issue
- Discuss the vulnerability publicly before it's fixed
- Exploit the vulnerability beyond proof-of-concept

### Do

1. **Email the maintainers directly** with:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact assessment
   - Any suggested fixes (optional)

2. **Allow reasonable time** for us to address the issue before public disclosure (typically 90 days)

3. **Coordinate disclosure** with us

## <img src="assets/icons/lock.png" width="16" height="16" alt=""/> Security Model

Codex Patcher is designed with security as a core principle:

### Workspace Isolation

- All file operations are restricted to the workspace root
- Paths are canonicalized to prevent directory traversal
- Symlinks escaping the workspace are rejected
- Forbidden directories (`~/.cargo`, `~/.rustup`, `target/`) are blocked

### Edit Safety

- Before-text verification prevents stale edits
- Atomic writes prevent partial file corruption
- Parse validation catches syntax errors before commit
- UTF-8 validation prevents encoding issues

### Trust Boundaries

Codex Patcher trusts:
- The workspace root path provided by the user
- Patch definition files in the `patches/` directory
- The Rust toolchain for compilation validation

Codex Patcher does NOT trust:
- File contents (always verified before editing)
- Symlinks (always canonicalized)
- External paths (always validated against workspace)

## <img src="assets/icons/star.png" width="16" height="16" alt=""/> Security Best Practices

When using Codex Patcher:

1. **Review patch definitions** before applying them
2. **Use `--dry-run`** to preview changes
3. **Keep patches in version control** for auditability
4. **Don't run as root** (not required, not recommended)

## <img src="assets/icons/book.png" width="16" height="16" alt=""/> Known Limitations

### TOCTOU (Time-of-Check-Time-of-Use)

Path validation and file editing are separate operations. In theory, a file could be replaced between validation and edit. Mitigations:

- Atomic writes prevent partial corruption
- Before-text verification catches unexpected changes
- Workspace is assumed to be under user's control

### Denial of Service

Large files or complex patch patterns could cause high memory/CPU usage. This is not considered a security vulnerability since the tool runs locally on trusted input.

## <img src="assets/icons/lightning.png" width="16" height="16" alt=""/> Security Updates

Security fixes are released as patch versions (e.g., 0.1.1, 0.1.2) and announced via:

- GitHub Security Advisories
- Release notes
- Changelog

Update promptly when security releases are available.
