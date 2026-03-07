# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `fuzzy_expansion: Option<usize>` field on `Query::Text` — elastic fuzzy window that
  tries window sizes `needle_lines..=needle_lines+N`, finding matches even when upstream
  inserted lines inside the needle's span. Schema validation rejects values > 200.
- `crate::fuzzy::find_best_match_elastic` — public API for elastic window matching,
  pre-computing haystack line offsets once across all expansion iterations.
- 9 new unit tests in `fuzzy.rs` covering exact match, similar match, byte positions,
  trailing-newline offset correctness, elastic insertion bridging (with verified
  Levenshtein scores), score comparison across expansions, and edge cases.
- 5 new integration tests in `tests/integration/unified_patches.rs`: end-to-end
  elastic apply, idempotency after elastic apply, and schema validation for
  `fuzzy_expansion` boundary (200) and implicit-threshold behaviour.
- `patches/io-drain-interp.toml` added to the patch library: instruments the exec IO
  drain pipeline (`read_capped`, `await_with_timeout`, `child.wait`,
  `consume_truncated_output`) with tracing hooks for end-to-end observability.
  Includes an `exec_drain_trace` integration test that exercises the full path.
- "Text query type" section in `patches/README.md` documenting `search`,
  `fuzzy_threshold`, and `fuzzy_expansion` with an illustrated example of elastic
  window behaviour across version bumps.
- "Per-patch version constraints" prose in `patches/README.md` documenting the
  patch-level `version` field and `SkippedVersion` result.

### Fixed
- **`io-drain-interp.toml` path doubling**: all five `[[patches]]` blocks used
  `file = "codex-rs/core/src/exec.rs"` with `workspace_relative = true`. Because the
  workspace root is already inside `codex-rs/`, the resolved path became
  `codex-rs/codex-rs/core/src/exec.rs` (not found). Corrected to
  `file = "core/src/exec.rs"`.
- **`approvals-ui.toml` `add-ctrl-a-keybind`**: v0.112 inserted a `Ctrl+L`
  clear-terminal handler (~18 lines) between the `Ctrl+T` and `Ctrl+G` handlers.
  Updated `[patches.query] search` to span the full `Ctrl+T`–`Ctrl+L`–`Ctrl+G`
  sequence. Updated replacement to insert the `Ctrl+A` preset-cycling block between
  `Ctrl+L` and `Ctrl+G`. Added `fuzzy_expansion = 25` for future handler insertions.
- **`privacy.toml` `privacy-realtime-remove-overrides-v108`**: v0.112 added a
  `build_realtime_startup_context` block (~8 lines) between the
  `unwrap_or(params.prompt)` and `let model = ...` anchors, breaking the exact search.
  Updated search text to include the new block. Updated replacement to remove only the
  privacy-sensitive overrides while preserving `build_realtime_startup_context`.
  Added `fuzzy_expansion = 15`.
- **`privacy.toml` `privacy-metrics-exporter-test-default-none`**: the assertion
  `assert_eq!(config.otel.metrics_exporter, OtelExporterKind::Statsig)` was removed
  upstream in v0.112. Deleted this patch block; the production-code equivalent is
  already covered by `privacy-config-metrics-exporter-default-none`.
- **`timing-loops.toml` `app-server-archive-wait-for-shutdown-watch`**: v0.112
  extracted the sleep-polling loop from the archive handler into a dedicated
  `wait_for_thread_shutdown` function. Variable names changed (`thread.` vs
  `conversation.`), the comment was removed, and the return type changed to
  `ThreadShutdownResult`. Retargeted the patch to the new function signature.
- `fuzzy.rs` refactored to avoid O(max_expansion) redundant allocations: `haystack_lines`
  and `line_offsets` are now computed once via `build_haystack_info` and shared across
  all window-size iterations in `find_best_match_elastic`.

### Complete documentation with iconics icons
- GitHub Actions CI workflow
- Issue templates (bug report, feature request)
- Pull request template
- Contributing guidelines
- Security policy
- MIT and Apache-2.0 license files
- Version-specific privacy patch sets (two consolidated files):
  - `patches/privacy-v0.99.toml` (`>=0.99.0-alpha.14, <0.105.0-alpha.13`)
  - `patches/privacy-v0.105-alpha13.toml` (`>=0.105.0-alpha.13`)
- `patches/timing-loops.toml`: replace polling loops with event-driven waiting (shutdown, PTY, commit animation)
- New v0.99 integration test coverage in `tests/integration/privacy_patches_v0_99.rs`.

### Changed
- Expanded `patches/privacy-v0.105-alpha13.toml` to additionally:
  force non-login shell behavior, ignore `LOG_FORMAT`, ignore externally supplied zsh wrapper socket
  paths, require full wrapper handshake env in wrapper mode, stop exporting legacy
  `BASH_EXEC_WRAPPER`, and remove `CODEX_APP_SERVER_URL` env override in app-server test client.
- Expanded `patches/privacy-v0.105-alpha13.toml` for `0.107.0-alpha.3`-era regressions:
  removed raw realtime text debug logs, redacted js_repl nested-tool raw payload/error logs,
  defaulted network proxy audit metadata to empty, and disabled network proxy policy audit events.
- Retired 8 stale `privacy-v0.105-alpha13` entries that no longer match modern codex-rs layouts
  and archived their exact definitions at
  `archive/retired-patches/privacy-v0.105-alpha13.retired-on-0.107.0-alpha.3.toml`.
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
