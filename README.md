# Codex Patcher

Automated code patching system for Rust with byte-span replacement and tree-sitter integration.

## Overview

`codex-patcher` is a robust, compiler-aware patching system designed to apply LLM-generated fixes, refactors, and insertions to Rust source files with high reliability. It preserves comments, formatting, and handles Rust's macro/cfg complexity without silent corruption.

## Core Principles

1. **Single Primitive**: All operations compile to byte-span replacement with verification
2. **Intelligence in Acquisition**: Smart span location, dumb application
3. **Safety First**: Verify before applying, atomic writes, workspace boundaries
4. **Compiler Integration**: Use `cargo check` diagnostics as semantic oracle
5. **Idempotent**: Safe to run multiple times

## Architecture

### Edit Primitive

```rust
pub struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    expected_before: EditVerification,
}
```

Every edit operation:
- Verifies expected text before applying
- Uses atomic file writes (tempfile + fsync + rename)
- Validates UTF-8
- Updates mtimes for incremental compilation invalidation

### Verification Strategies

```rust
pub enum EditVerification {
    ExactMatch(String),  // For spans < 1KB
    Hash(u64),          // xxh3 for larger spans
}
```

Automatic selection based on text size, ensuring both safety and performance.

### Safety Rails

**Hard Rules** (never violated):
- Selector uniqueness: 0 or >1 matches = refuse
- Before-text verification: Always verify before overwriting
- No external edits: Never modify files outside workspace root
- Parse validation: Re-parse after edit, rollback on ERROR nodes
- Multi-edit ordering: Sort descending, apply bottom-to-top

**Workspace Boundaries**:
- Validates all paths against workspace root
- Blocks edits to `~/.cargo/registry`, `~/.rustup`, `target/`
- Handles symlink escapes correctly

## Usage

### Library API

```rust
use codex_patcher::{Edit, WorkspaceGuard};
use std::path::PathBuf;

// Create workspace guard
let guard = WorkspaceGuard::new("/path/to/workspace")?;

// Validate path is safe to edit
let file = guard.validate_path("src/main.rs")?;

// Create and apply edit
let edit = Edit::new(
    file,
    0,
    5,
    "HELLO",
    "hello",  // Expected before-text
);

match edit.apply()? {
    EditResult::Applied { .. } => println!("Edit applied"),
    EditResult::AlreadyApplied { .. } => println!("Already patched"),
}
```

### Batch Edits

```rust
let edits = vec![
    Edit::new("src/main.rs", 0, 5, "HELLO", "hello"),
    Edit::new("src/main.rs", 10, 15, "WORLD", "world"),
    Edit::new("src/lib.rs", 0, 3, "FOO", "foo"),
];

// Applies atomically per file, sorted correctly
let results = Edit::apply_batch(edits)?;
```

### CLI (Coming Soon)

```bash
# Apply patches from definitions
codex-patcher apply --workspace ~/dev/codex/codex-rs

# Dry run with diff
codex-patcher apply --workspace ~/dev/codex/codex-rs --dry-run --diff

# Check patch status
codex-patcher status --workspace ~/dev/codex/codex-rs
```

## Implementation Status

### Phase 1: Core Edit Primitive ✅ COMPLETE

- [x] Edit struct with byte-span replacement
- [x] EditVerification (ExactMatch + Hash)
- [x] Atomic file writes (tempfile + fsync + rename)
- [x] Workspace boundary checks
- [x] Multi-edit span translation
- [x] Comprehensive unit tests
- [x] Clippy clean

**Deliverable**: Can apply raw byte-span edits with verification

### Phase 2: TOML Patching (In Progress)

- [x] Integrate `toml_edit` for structure-preserving TOML edits
- [x] TOML query language (section path, key matching)
- [x] Operations: insert_section, append_section, replace_value, delete_section, replace_key
- [x] Compile TOML queries → byte spans with selector uniqueness validation
- [x] Post-edit TOML validation (parse after edit)

See `docs/toml.md` for query syntax, examples, and error guidance.

### Phase 3: Tree-Sitter Span Locator ✅ COMPLETE

- [x] Integrate `tree-sitter` + `tree-sitter-rust`
- [x] Tree-sitter query engine with capture support
- [x] Span extraction from query matches
- [x] Edition-aware parsing (2015/2018/2021/2024)
- [x] Structural locators for common Rust constructs (fn, struct, impl, const, etc.)
- [x] Parse validation (ERROR node detection)
- [x] 21 new unit tests

### Phase 4: ast-grep Integration

- [ ] Integrate `ast-grep-core` for pattern matching
- [ ] Pattern → byte span conversion
- [ ] Capture group replacement
- [ ] Context constraints

### Phase 5: Validation & Safety

- [ ] Parse validation (ERROR node detection)
- [ ] `syn` validation for generated snippets
- [ ] Selector uniqueness enforcement
- [ ] Compile validation (`cargo check` integration)

### Phase 6: Patch Config Parser

- [ ] TOML schema for patch definitions
- [ ] Patch config parser
- [ ] Version range filtering (semver)
- [ ] Idempotency checks

### Phase 7: CLI & UX

- [ ] CLI with clap (apply, status, verify commands)
- [ ] `--dry-run` and `--diff` output
- [ ] Conflict detection and reporting
- [ ] Progress indicators

### Phase 8: Initial Patch Definitions

- [ ] `patches/privacy.toml` - Statsig telemetry removal
- [ ] `patches/performance.toml` - Build profile optimizations
- [ ] Test on clean upstream checkout
- [ ] Patch authoring guide

## Design Rationale

### Why Byte-Span Replacement?

All high-level operations (AST transforms, diffs, structural edits) compile to the same primitive. This:
- Simplifies verification logic
- Makes debugging trivial (see exactly what changed)
- Enables multiple span location strategies
- Preserves formatting and comments

### Why Not syn for Editing?

`syn` is great for parsing, but:
- Drops comments
- Normalizes formatting
- Over-engineered for patching needs

We use `syn` only for validating generated code snippets.

### Why Tree-Sitter + ast-grep?

- Tree-sitter preserves exact source (CST not AST)
- ast-grep provides pattern-matching DSL
- Robust to whitespace/line changes
- Edition-aware parsing

### Why Not Git Patches?

- Line-number based (fragile)
- Merge conflicts require manual resolution
- No semantic understanding
- Can't detect "already applied"

## Testing

```bash
# Run tests
cargo test

# Run with coverage
cargo test --verbose

# Clippy
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --check
```

## Dependencies

- `tree-sitter` + `tree-sitter-rust` - CST parsing
- `ast-grep-core` - Pattern-based matching
- `toml_edit` - Structure-preserving TOML edits
- `syn` - Validation only (not editing)
- `cargo_metadata` - Workspace metadata parsing
- `xxhash-rust` - Fast hashing for verification
- `tempfile` - Atomic file writes
- `filetime` - Mtime updates for incremental compilation

## License

MIT OR Apache-2.0

## Author

Zack <zack@tier.net>

## References

- [CLAUDE.md](./CLAUDE.md) - Full technical specification
- [plan.md](./plan.md) - Implementation plan and timeline
- [spec.md](./spec.md) - Alternative specification format
