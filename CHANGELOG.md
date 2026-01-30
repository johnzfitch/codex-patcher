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
