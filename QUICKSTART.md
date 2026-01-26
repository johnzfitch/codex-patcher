# Codex Patcher - Quick Start Guide

## What is Codex Patcher?

Automated code patching system for Rust. Applies privacy/performance patches to OpenAI's Codex CLI after upstream releases using byte-span replacement as the primitive.

## Installation

```bash
git clone https://github.com/your-org/codex-patcher.git
cd codex-patcher
cargo build --release
```

## Basic Usage

### 1. Apply patches to a workspace

```bash
cargo run -- apply --workspace /path/to/workspace
```

This will:
- Discover all `.toml` patch files in `workspace/patches/`
- Read workspace version from `Cargo.toml`
- Apply patches that match version constraints
- Show clear success/failure status for each patch

### 2. Check what patches are applied

```bash
cargo run -- status --workspace /path/to/workspace
```

Output shows:
- ✓ APPLIED - Patches already in effect
- ⊙ NOT APPLIED - Patches that can be applied
- ⊘ SKIPPED - Patches filtered by version constraints

### 3. Verify patches are correct

```bash
cargo run -- verify --workspace /path/to/workspace
```

Checks that applied patches match expected state. Useful after manual edits or upstream changes.

## Advanced Options

### Dry-run mode

See what would happen without modifying files:

```bash
cargo run -- apply --workspace /path/to/workspace --dry-run
```

### Show diffs

See unified diffs of changes:

```bash
cargo run -- apply --workspace /path/to/workspace --diff
```

### Apply specific patch file

```bash
cargo run -- apply --workspace /path/to/workspace --patches patches/privacy.toml
```

## Creating Patches

Patches are defined in TOML files. Example:

```toml
[meta]
name = "privacy-patches"
description = "Remove telemetry and analytics"
version_range = ">=0.88.0"
workspace_relative = true

[[patches]]
id = "disable-statsig"
file = "otel/src/config.rs"

[patches.query]
type = "ast-grep"
pattern = "fn resolve_exporter($$$) { $$$BODY }"

[patches.operation]
type = "replace"
text = "fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter { OtelExporter::None }"
```

### Query Types

1. **ast-grep**: Pattern matching with AST awareness
   ```toml
   [patches.query]
   type = "ast-grep"
   pattern = "fn $NAME() { $$$BODY }"
   ```

2. **tree-sitter**: Structural queries
   ```toml
   [patches.query]
   type = "tree-sitter"
   pattern = "(function_item name: (identifier) @name)"
   ```

3. **toml**: TOML-specific operations
   ```toml
   [patches.query]
   type = "toml"
   path = "dependencies.serde"
   ```

### Operation Types

1. **replace**: Replace matched content
   ```toml
   [patches.operation]
   type = "replace"
   text = "new content"
   ```

2. **insert-before**: Insert before match
   ```toml
   [patches.operation]
   type = "insert-before"
   text = "content to insert"
   ```

3. **insert-after**: Insert after match
   ```toml
   [patches.operation]
   type = "insert-after"
   text = "content to insert"
   ```

4. **delete**: Remove matched content
   ```toml
   [patches.operation]
   type = "delete"
   ```

### Version Constraints

Use semver requirements:

```toml
version_range = ">=0.88.0"          # At least 0.88.0
version_range = ">=0.88.0, <0.89.0" # Range
version_range = "^0.88.0"           # Compatible with 0.88.x
version_range = "=0.88.0"           # Exact match
```

### Verification

Add verification to ensure patches target correct code:

```toml
[patches.verify]
type = "exact-match"
expected_text = "original code here"
```

Or use hash for large blocks:

```toml
[patches.verify]
type = "hash"
algorithm = "xxh3"
expected = "0x1234567890abcdef"
```

## Example Workflow

After merging an upstream Codex release:

```bash
# 1. Merge upstream
cd ~/dev/codex
git checkout -b merge-v0.89.0
git merge rust-v0.89.0

# 2. Apply patches
cd ~/dev/codex-patcher
cargo run -- apply --workspace ~/dev/codex/codex-rs --diff

# 3. Check results
cargo run -- status --workspace ~/dev/codex/codex-rs

# 4. Verify patches
cargo run -- verify --workspace ~/dev/codex/codex-rs

# 5. Test and commit
cd ~/dev/codex/codex-rs
cargo test
git commit -am "Apply privacy patches to v0.89.0"
```

## Troubleshooting

### "Query matched no locations"

The pattern didn't find a match. Possible causes:
- Function was renamed or removed
- Signature changed
- Code moved to different file

**Solution**: Update the patch pattern or mark as obsolete.

### "Query matched multiple locations"

The pattern is ambiguous.

**Solution**: Make the pattern more specific (e.g., include surrounding context).

### "Version mismatch"

Patch has version constraint that doesn't match workspace version.

**Solution**: Update version_range in patch metadata or workspace version.

## Documentation

- `CLAUDE.md` - System architecture and specification
- `PHASE7_COMPLETE.md` - Phase 7 completion report
- `HANDOFF_PHASE7.md` - Phase 7 implementation guide
- `plan.md` - Full implementation plan

## Testing

Run all tests:

```bash
cargo test --quiet
```

Run specific test suite:

```bash
cargo test --test cli_integration
cargo test --test config_integration
```

## Getting Help

For issues or questions:
- Check the documentation files
- Run `cargo run -- --help` for CLI help
- Run `cargo run -- <command> --help` for command-specific help
- Review existing patches in `patches/` directory
