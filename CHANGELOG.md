# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-03-07

### Fixed
- **Version range false positive for pre-release builds**: `matches_requirement` incorrectly
  matched pre-release versions past an upper bound that had a pre-release tag (e.g.
  `0.112.0-alpha.11` matched `>=0.105.0-alpha.13, <0.108.0-alpha.1`). The `dominated`
  fallback stripped the pre-release, then the semver crate silently ignored the upper-bound
  comparator (different `major.minor.patch` base), leaving only the lower bound to evaluate.
  Fixed by (a) relaxing `dominated` so upper-bound comparators don't need their base to be
  below the version's base (this also unblocks mid-range pre-release versions like
  `0.106.0-alpha.5`), and (b) adding a guard that skips the retry when a pre-release
  upper-bound comparator's base is exceeded by the version.
- **`privacy.toml` v105 realtime patch**: replacement text used `_config` but downstream
  code in the same function still referenced `config`, causing a compile error if the patch
  applied within its declared version range.

## [0.1.1] - 2026-03-07

### Added
- `fuzzy_expansion: Option<usize>` field on `Query::Text` &mdash; elastic fuzzy window that
  tries window sizes `needle_lines..=needle_lines+N`, finding matches even when upstream
  inserted lines inside the needle's span. Schema validation rejects values &gt; 200.
- `crate::fuzzy::find_best_match_elastic` &mdash; public API for elastic window matching,
  pre-computing haystack line offsets once across all expansion iterations.
- `AppendSection` end-to-end integration test through `apply_patches` (batched path):
  verifies first call returns `Applied` and second call returns `AlreadyApplied`.
- 9 new unit tests in `fuzzy.rs` covering exact match, similar match, byte positions,
  trailing-newline offset correctness, elastic insertion bridging, score comparison, and edge cases.
- 5 new integration tests in `tests/integration/unified_patches.rs`: end-to-end
  elastic apply, idempotency after elastic apply, and schema validation for
  `fuzzy_expansion` boundary (200) and implicit-threshold behaviour.
- `patches/io-drain-interp.toml`: instruments the exec IO drain pipeline
  (`read_capped`, `await_with_timeout`, `child.wait`, `consume_truncated_output`)
  with tracing hooks for end-to-end observability.
- `patches/native-ca-roots.toml`: added workspace-root `reqwest` declaration patch
  to satisfy Cargo 1.84+ `default-features` constraint on workspace-inherited deps.
- "Text query type" and "Per-patch version constraints" documentation in `patches/README.md`.
- Version-specific privacy patch sets and `patches/timing-loops.toml`.
- GitHub Actions CI, issue templates, PR template, contributing guidelines, security policy.

### Changed
- `fuzzy.rs` refactored to compute `haystack_lines` and `line_offsets` once via
  `build_haystack_info`, eliminating O(max_expansion) redundant allocations.
- Enforced patch-file version gating in `src/config/applicator.rs`:
  incompatible files now return `PatchResult::SkippedVersion` instead of being applied.
- Improved patch discovery in `src/main.rs` to prefer `<workspace>/patches`
  with fallback to local `./patches`.
- Improved workspace version detection to add `Cargo.toml` parsing fallback
  when `cargo metadata` is unavailable.
- Normalized structural replacement trailing-newline handling to restore idempotent `verify` behavior.

### Fixed
- **`io-drain-interp.toml` path doubling**: corrected `file` paths from
  `codex-rs/core/src/exec.rs` to `core/src/exec.rs` (workspace root already inside `codex-rs/`).
- **`approvals-ui.toml` `add-ctrl-a-keybind`**: updated search anchor to span
  `Ctrl+T`&ndash;`Ctrl+L`&ndash;`Ctrl+G` sequence after v0.112 inserted a new handler between them.
  Added `fuzzy_expansion = 25`.
- **`privacy.toml` `privacy-realtime-remove-overrides-v108`**: updated search text to
  include `build_realtime_startup_context` block inserted by v0.112. Added `fuzzy_expansion = 15`.
- **`privacy.toml`**: removed stale `privacy-metrics-exporter-test-default-none` patch
  block after upstream deleted the covered assertion in v0.112.
- **`timing-loops.toml`**: retargeted `app-server-archive-wait-for-shutdown-watch` to
  the new `wait_for_thread_shutdown` function extracted in v0.112.

### Security
- Removed embedded Statsig-like key literals from docs, tests, and patch comments;
  replaced with redacted placeholders.

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

[Unreleased]: https://github.com/johnzfitch/codex-patcher/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/johnzfitch/codex-patcher/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/johnzfitch/codex-patcher/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/johnzfitch/codex-patcher/releases/tag/v0.1.0
