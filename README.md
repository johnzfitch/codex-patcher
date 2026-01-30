<p align="center">
  <img src=".github/assets/icons/rocket.png" width="64" alt="Codex Patcher"/>
</p>

<h1 align="center">Codex Patcher</h1>

<p align="center">
  <strong>Automated code patching system for Rust with byte-span replacement and tree-sitter integration</strong>
</p>

<p align="center">
  <a href="https://github.com/johnzfitch/codex-patcher/actions"><img src="https://img.shields.io/github/actions/workflow/status/johnzfitch/codex-patcher/ci.yml?branch=master&style=flat-square" alt="CI Status"></a>
  <a href="https://crates.io/crates/codex-patcher"><img src="https://img.shields.io/crates/v/codex-patcher.svg?style=flat-square" alt="Crates.io"></a>
  <a href="https://docs.rs/codex-patcher"><img src="https://img.shields.io/docsrs/codex-patcher?style=flat-square" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square" alt="License"></a>
</p>

---

## <img src=".github/assets/icons/eye.png" width="16" height="16" alt=""/> Overview

`codex-patcher` is a robust, compiler-aware patching system designed to apply LLM-generated fixes, refactors, and insertions to Rust source files with high reliability. It preserves comments, formatting, and handles Rust's macro/cfg complexity without silent corruption.

### <img src=".github/assets/icons/star.png" width="16" height="16" alt=""/> Key Features

| Feature | Description |
|---------|-------------|
| <img src=".github/assets/icons/shield-security-protection-16x16.png" width="14" alt=""/> **Atomic Writes** | Tempfile + fsync + rename for crash safety |
| <img src=".github/assets/icons/tick.png" width="14" alt=""/> **Idempotent** | Safe to run multiple times without side effects |
| <img src=".github/assets/icons/tree.png" width="14" alt=""/> **Tree-sitter** | CST-based parsing preserves exact source layout |
| <img src=".github/assets/icons/search.png" width="14" alt=""/> **ast-grep** | Pattern-based code matching and replacement |
| <img src=".github/assets/icons/lock.png" width="14" alt=""/> **Workspace Safety** | Prevents edits outside project boundaries |
| <img src=".github/assets/icons/layers.png" width="14" alt=""/> **Multi-format** | Supports Rust, TOML, and more |

---

## <img src=".github/assets/icons/lightning.png" width="16" height="16" alt=""/> Quick Start

### Installation

```bash
# From crates.io (when published)
cargo install codex-patcher

# From source
git clone https://github.com/johnzfitch/codex-patcher
cd codex-patcher
cargo install --path .
```

### Basic Usage

```bash
# Apply all patches in patches/ directory
codex-patcher apply --workspace /path/to/codex-rs

# Dry run with diff output
codex-patcher apply --workspace /path/to/codex-rs --dry-run --diff

# Check status of patches
codex-patcher status --workspace /path/to/codex-rs

# Verify patches are applied correctly
codex-patcher verify --workspace /path/to/codex-rs
```

---

## <img src=".github/assets/icons/diagram.png" width="16" height="16" alt=""/> Architecture

### Core Primitive: Byte-Span Replacement

All edit operations compile down to a single primitive:

```rust
pub struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    expected_before: EditVerification,
}
```

**Why this design?**
- Simplifies verification logic
- Makes debugging trivial (see exactly what changed)
- Enables multiple span location strategies
- Preserves formatting and comments

### Verification Strategies

```rust
pub enum EditVerification {
    ExactMatch(String),  // For spans < 1KB
    Hash(u64),           // xxh3 for larger spans
}
```

Automatic selection based on text size ensures both safety and performance.

---

## <img src=".github/assets/icons/book.png" width="16" height="16" alt=""/> Documentation

| Document | Description |
|----------|-------------|
| [Getting Started](docs/getting-started.md) | Installation and first steps |
| [API Reference](docs/api.md) | Library API documentation |
| [Patch Authoring](docs/patches.md) | How to write patch definitions |
| [Architecture](docs/architecture.md) | System design and internals |
| [TOML Patching](docs/toml.md) | TOML-specific query syntax |

---

## <img src=".github/assets/icons/console.png" width="16" height="16" alt=""/> CLI Reference

### Commands

| Command | Description |
|---------|-------------|
| `apply` | Apply patches to a workspace |
| `status` | Check which patches are applied |
| `verify` | Verify patches match expected state |
| `list` | List available patches |

### Options

```
codex-patcher apply [OPTIONS]

Options:
  -w, --workspace <PATH>  Path to workspace root (auto-detected if not specified)
  -p, --patches <FILE>    Specific patch file to apply
  -n, --dry-run           Show what would be changed without modifying files
  -d, --diff              Show unified diff of changes
  -h, --help              Print help
  -V, --version           Print version
```

---

## <img src=".github/assets/icons/script.png" width="16" height="16" alt=""/> Library Usage

### Basic Edit

```rust
use codex_patcher::{Edit, EditResult, WorkspaceGuard};

// Create workspace guard for safety
let guard = WorkspaceGuard::new("/path/to/workspace")?;

// Validate path is within workspace
let file = guard.validate_path("src/main.rs")?;

// Create and apply edit
let edit = Edit::new(
    file,
    0,              // byte_start
    5,              // byte_end
    "HELLO",        // new_text
    "hello",        // expected_before
);

match edit.apply()? {
    EditResult::Applied { file, bytes_changed } => {
        println!("Applied {} bytes to {}", bytes_changed, file.display());
    }
    EditResult::AlreadyApplied { file } => {
        println!("Already patched: {}", file.display());
    }
}
```

### Batch Edits

```rust
use codex_patcher::Edit;

let edits = vec![
    Edit::new("src/main.rs", 0, 5, "HELLO", "hello"),
    Edit::new("src/main.rs", 10, 15, "WORLD", "world"),
    Edit::new("src/lib.rs", 0, 3, "FOO", "foo"),
];

// Applies atomically per file, sorted correctly
let results = Edit::apply_batch(edits)?;

for result in results {
    println!("{:?}", result);
}
```

### Pattern Matching with ast-grep

```rust
use codex_patcher::sg::PatternMatcher;

let source = r#"
    fn main() {
        let x = foo.clone();
        let y = bar.clone();
    }
"#;

let matcher = PatternMatcher::new(source);

// Find all .clone() calls
let matches = matcher.find_all("$EXPR.clone()")?;

for m in matches {
    println!("Found clone at bytes {}..{}", m.byte_start, m.byte_end);
}
```

---

## <img src=".github/assets/icons/shield-security-protection-16x16.png" width="16" height="16" alt=""/> Safety Guarantees

### Hard Rules (Never Violated)

| Rule | Description |
|------|-------------|
| <img src=".github/assets/icons/tick.png" width="12" alt=""/> **Selector Uniqueness** | 0 or >1 matches = refuse to edit |
| <img src=".github/assets/icons/tick.png" width="12" alt=""/> **Before-text Verification** | Always verify expected content exists |
| <img src=".github/assets/icons/tick.png" width="12" alt=""/> **No External Edits** | Never modify files outside workspace |
| <img src=".github/assets/icons/tick.png" width="12" alt=""/> **Parse Validation** | Re-parse after edit, rollback on errors |
| <img src=".github/assets/icons/tick.png" width="12" alt=""/> **UTF-8 Validation** | Prevent creation of invalid files |

### Workspace Boundaries

The `WorkspaceGuard` prevents edits to:
- `~/.cargo/registry` - Dependency source code
- `~/.rustup` - Toolchain installations
- `target/` - Build artifacts
- Symlinks escaping workspace

---

## <img src=".github/assets/icons/folder.png" width="16" height="16" alt=""/> Project Structure

```
codex-patcher/
├── src/
│   ├── lib.rs           # Library entry point
│   ├── main.rs          # CLI entry point
│   ├── edit.rs          # Core Edit primitive
│   ├── safety.rs        # WorkspaceGuard
│   ├── validate.rs      # Parse/syn validation
│   ├── config/          # Patch configuration
│   │   ├── schema.rs    # Patch definition types
│   │   ├── loader.rs    # TOML config parser
│   │   ├── applicator.rs # Patch application logic
│   │   └── version.rs   # Semver filtering
│   ├── ts/              # Tree-sitter integration
│   │   ├── parser.rs    # Rust parser wrapper
│   │   ├── query.rs     # Query engine
│   │   └── locator.rs   # Structural locators
│   ├── sg/              # ast-grep integration
│   │   ├── matcher.rs   # Pattern matching
│   │   ├── replacer.rs  # Replacement operations
│   │   └── lang.rs      # Language support
│   └── toml/            # TOML editing
│       ├── editor.rs    # TomlEditor
│       ├── query.rs     # Section/key queries
│       └── operations.rs # TOML operations
├── patches/             # Patch definitions
│   ├── privacy.toml     # Telemetry removal
│   └── performance.toml # Build optimizations
├── tests/               # Integration tests
└── docs/                # Documentation
```

---

## <img src=".github/assets/icons/toolbox.png" width="16" height="16" alt=""/> Development

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt
```

### Test Coverage

```bash
# Run all tests with output
cargo test --all-targets -- --nocapture

# Run specific test
cargo test test_atomic_write

# Run integration tests only
cargo test --test integration
```

---

## <img src=".github/assets/icons/checkbox.png" width="16" height="16" alt=""/> Status

| Component | Status |
|-----------|--------|
| Core Edit Primitive | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| Atomic File Writes | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| Workspace Guards | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| Tree-sitter Integration | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| ast-grep Integration | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| TOML Patching | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| CLI Interface | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| Patch Definitions | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |
| Documentation | <img src=".github/assets/icons/tick.png" width="12" alt=""/> Complete |

**Test Results:** 117 tests, 100% passing

---

## <img src=".github/assets/icons/globe.png" width="16" height="16" alt=""/> Contributing

We welcome contributions! Please see [CONTRIBUTING.md](.github/CONTRIBUTING.md) for guidelines.

### Quick Contribution Guide

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `cargo test && cargo clippy --all-targets -- -D warnings`
5. Commit: `git commit -m "feat: add amazing feature"`
6. Push: `git push origin feature/amazing-feature`
7. Open a Pull Request

---

## <img src=".github/assets/icons/key.png" width="16" height="16" alt=""/> License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## <img src=".github/assets/icons/star.png" width="16" height="16" alt=""/> Acknowledgments

Built with these excellent crates:
- [tree-sitter](https://tree-sitter.github.io/) - Incremental parsing
- [ast-grep](https://ast-grep.github.io/) - Structural code search
- [toml_edit](https://docs.rs/toml_edit) - Format-preserving TOML
- [clap](https://docs.rs/clap) - Command-line parsing

---

<p align="center">
  <sub>Made with care for the Rust community</sub>
</p>
