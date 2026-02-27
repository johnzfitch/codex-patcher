# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Complete documentation with iconics icons
- GitHub Actions CI workflow
- Issue templates (bug report, feature request)
- Pull request template
- Contributing guidelines
- Security policy
- MIT and Apache-2.0 license files
- Version-specific privacy patch sets for Codex `0.99.0-alpha` releases:
  - `patches/privacy-v0.99-alpha1-alpha22.toml` (`>=0.99.0-alpha.10, <0.99.0-alpha.14`)
  - `patches/privacy-v0.99-alpha14-alpha20.toml` (`>=0.99.0-alpha.14, <0.99.0-alpha.21`)
  - `patches/privacy-v0.99-alpha23.toml` (`>=0.99.0-alpha.21, <0.105.0-alpha.13`)
- Version-specific privacy patch set for Codex `0.105.x` releases:
  - `patches/privacy-v0.105-alpha13.toml` (`>=0.105.0-alpha.13, <0.106.0`)
- `patches/timing-loops.toml`: replace polling loops with event-driven waiting (shutdown, PTY, commit animation)
- New v0.99 integration test coverage in `tests/integration/privacy_patches_v0_99.rs`.

### Changed
- Expanded `patches/privacy-v0.105-alpha13.toml` to additionally:
  force non-login shell behavior, ignore `LOG_FORMAT`, ignore externally supplied zsh wrapper socket
  paths, require full wrapper handshake env in wrapper mode, stop exporting legacy
  `BASH_EXEC_WRAPPER`, and remove `CODEX_APP_SERVER_URL` env override in app-server test client.
- Enforced patch-file version gating in `src/config/applicator.rs`:
  incompatible files now return `PatchResult::SkippedVersion` instead of being applied.
- Updated legacy privacy patch range in `patches/privacy.toml` to
  `>=0.88.0, <0.99.0-alpha.7` to match upstream web-search signature changes.
- Improved patch discovery in `src/main.rs` to prefer `<workspace>/patches`
  with fallback to local `./patches`.
- Improved workspace version detection in `src/main.rs` by adding
  `Cargo.toml` parsing fallback when `cargo metadata` is unavailable.
- Normalized structural replacement trailing-newline handling in
  `src/config/applicator.rs` to restore idempotent `verify` behavior.

### Security
- Removed embedded Statsig-like key literals from docs/tests/patch comments and replaced with
  redacted placeholders to avoid accidental secret propagation.

### Fixed
- Updated end-to-end and integration expectations for the new privacy patch split and
  `metrics_exporter` default behavior across alpha version boundaries.

## [0.1.0] - 2025-01-27

### Added
- Core `Edit` primitive with byte-span replacement
- `EditVerification` with ExactMatch and Hash strategies
- Atomic file writes (tempfile + fsync + rename)
- `WorkspaceGuard` for workspace boundary enforcement
- Symlink escape detection
- Forbidden path blocking (~/.cargo, ~/.rustup, target/)

### Added - Tree-sitter Integration
- `RustParser` wrapper with edition support (2015/2018/2021/2024)
- `QueryEngine` for tree-sitter queries
- `StructuralLocator` for finding functions, structs, impls, consts
- Parse validation with ERROR node detection

### Added - ast-grep Integration
- `PatternMatcher` for pattern-based code search
- Metavariable support ($NAME, $$$NAME)
- `CaptureReplacer` for template-based replacement
- Context-aware matching (find in function)

### Added - TOML Editing
- `TomlEditor` for format-preserving TOML edits
- Section and key queries with path syntax
- Operations: insert_section, append_section, replace_value, delete_section
- Positioning control (after_section, before_section, at_end)

### Added - Patch Configuration
- TOML-based patch definitions
- Version range filtering (semver constraints)
- Idempotency checks
- Multiple query types (ast-grep, tree-sitter, toml)

### Added - CLI
- `apply` command with --dry-run and --diff
- `status` command for patch status
- `verify` command for verification
- `list` command for listing patches
- Workspace auto-detection
- Environment variable support (CODEX_WORKSPACE)

### Added - Validation
- `ParseValidator` for tree-sitter parse checking
- `syn_validate` module for snippet validation
- `ValidatedEdit` wrapper with automatic validation
- `SelectorValidator` for uniqueness checks

### Security
- Workspace boundary enforcement
- Symlink escape prevention
- Forbidden directory blocking
- Before-text verification
- UTF-8 validation

[Unreleased]: https://github.com/johnzfitch/codex-patcher/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/johnzfitch/codex-patcher/releases/tag/v0.1.0
