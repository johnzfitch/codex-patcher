# Rust Codex Patcher: System Specification

Build an automated code patching system for Rust that can apply LLM-generated fixes, refactors, and insertions to Rust source files with high reliability. The system must preserve comments, formatting, and handle Rust's macro/cfg complexity without silent corruption.

## Core Architecture

### Primitive: Byte-Span Replacement

All edit operations compile down to a single primitive:

```rust
struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    /// Hash or exact text of what we expect to find at [byte_start, byte_end)
    expected_before: EditVerification,
}

enum EditVerification {
    ExactMatch(String),
    Hash(u64), // xxhash or similar
}
```

This is non-negotiable. Unified diffs, AST transforms, "insert method" operations—all of them produce `Edit` values. The edit applicator is dumb and simple; intelligence lives in span acquisition.

### Edit Application Rules

1. **Verify before applying.** Read `file[byte_start..byte_end]`, compare against `expected_before`. Mismatch = stale span = hard stop.

2. **Multi-edit span translation.** When applying multiple edits to one file in a single pass:
   - Sort edits by `byte_start` descending
   - Apply from bottom to top
   - This ensures earlier edits don't invalidate later byte offsets
   - Alternative: apply sequentially and recompute spans between each edit (slower, more correct for complex cases)

3. **Never edit outside workspace.** Resolve all paths. If a path is in `~/.cargo/registry`, `~/.rustup`, or outside the workspace root, refuse.

4. **Atomic file writes.** Write to tempfile, fsync, rename. Never partial-write a source file.

## Compiler Integration Layer

### Primary Locator: `cargo check --message-format=json`

The compiler is the semantic oracle. Use its diagnostics as the primary source of edit targets.

```rust
use cargo_metadata::Message;

fn parse_cargo_stream(reader: impl BufRead) -> impl Iterator<Item = Message> {
    reader.lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.starts_with('{'))  // Defensive: proc macros can print garbage
        .filter_map(|line| serde_json::from_str(&line).ok())
}
```

**Why filter on `{`:** Cargo explicitly documents that other tools (including proc macros) can emit arbitrary stdout. The JSON stream is not guaranteed clean. Parse defensively.

### Diagnostic Span Extraction

Compiler messages include:
- `spans[].byte_start`, `spans[].byte_end` — exact file offsets
- `spans[].file_name` — path to source file
- `spans[].expansion` — if present, span is inside macro expansion
- `spans[].suggested_replacement` — optional fix text
- `spans[].suggestion_applicability` — `MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`

#### Handling `MachineApplicable` Suggestions

These are high-confidence compiler fixes. Use them, but with caution:

```rust
enum SuggestionPolicy {
    /// Apply without LLM intervention
    AutoApply,
    /// Present to LLM as strong candidate, let it decide
    Suggest,
    /// Ignore compiler suggestion, let LLM solve from scratch
    Ignore,
}

fn suggestion_policy(applicability: Applicability, code: &str) -> SuggestionPolicy {
    match applicability {
        Applicability::MachineApplicable => {
            // Some "machine applicable" suggestions are context-dependent
            // E.g., lifetime suggestions when multiple valid choices exist
            if is_lifetime_suggestion(code) || is_ambiguous_import(code) {
                SuggestionPolicy::Suggest
            } else {
                SuggestionPolicy::AutoApply
            }
        }
        Applicability::MaybeIncorrect => SuggestionPolicy::Suggest,
        _ => SuggestionPolicy::Ignore,
    }
}
```

#### Macro Expansion Handling

When a span has an `expansion` field:

```rust
struct Expansion {
    span: Span,           // The macro invocation site (call site)
    def_site: Option<Span>, // Where the macro is defined
    macro_decl_name: String,
}
```

**Policy:**
1. If diagnostic points inside expansion, prefer patching at `expansion.span` (call site) when the fix is "change macro arguments" or "change the call"
2. Never patch `def_site` unless explicitly targeting macro definitions
3. Refuse to patch if `file_name` indicates generated code (e.g., paths containing `target/`, `OUT_DIR`)
4. For `derive` macros, the fix is usually on the struct/enum definition, not inside the expansion

### Incremental Compilation Gotchas

After patching a file:

```rust
fn invalidate_incremental(file: &Path, workspace_root: &Path) -> Result<()> {
    // Touch the file to update mtime
    filetime::set_file_mtime(file, filetime::FileTime::now())?;
    
    // For tight patch-check loops, sometimes this isn't enough.
    // Option 1: Disable incremental for patcher runs
    //   CARGO_INCREMENTAL=0 cargo check
    // Option 2: Clean the specific crate
    //   cargo clean -p <crate_name>
    // Option 3: Accept the occasional spurious "no change detected"
    
    Ok(())
}
```

For maximum reliability in tight loops, run with `CARGO_INCREMENTAL=0`. It's slower but deterministic.

### Quick Pass/Fail Checks

When you just need "did it compile?" without parsing full diagnostics:

```bash
cargo check --message-format=short 2>&1
```

Returns minimal output, exit code tells you pass/fail. Useful for validation after edits.

## Structural Editing Layer

### Tree-sitter as CST Engine

Use tree-sitter for structural operations when compiler diagnostics don't provide a span:
- Locating insertion points (end of impl block, after use statements)
- Batch refactors across files
- Sanity-checking edits (parse before/after, detect disaster)

```rust
use tree_sitter::{Parser, Tree};

struct RustParser {
    parser: Parser,
    /// Must match target crate's edition
    grammar_edition: RustEdition,
}

enum RustEdition {
    E2015,
    E2018,
    E2021,
    E2024,
}
```

**Edition matters.** Tree-sitter grammars track syntax. If targeting edition 2024 code with new syntax (e.g., `gen` blocks), ensure your grammar supports it. Pin `tree-sitter-rust` version deliberately.

### ast-grep as Rewrite DSL

Layer ast-grep on tree-sitter for pattern-based matching:

```rust
use ast_grep_core::{Pattern, Matcher, Node};

// Example: Find all structs missing Debug derive
let pattern = Pattern::new("#[derive($DERIVES)] struct $NAME", RustLang);

for matched in pattern.find_all(&root) {
    let derives = matched.get_env().get("DERIVES");
    if !derives.contains("Debug") {
        // Compute edit to add Debug
    }
}
```

ast-grep handles the query language so you're not hand-rolling tree-sitter queries for every operation.

### When to Use Each Layer

| Situation | Tool |
|-----------|------|
| Compiler error with span | Use span directly |
| Compiler suggestion (MachineApplicable) | Apply suggestion |
| "Add derive to struct X" | ast-grep pattern match |
| "Insert method into impl" | Tree-sitter: find impl block, locate closing brace |
| "Add import" | Tree-sitter: find last use statement or module start |
| Validate edit didn't corrupt syntax | Tree-sitter parse, check for ERROR nodes |
| LLM says "replace function body" | Extract function span via tree-sitter, replace |

### syn for Validation Only

Use `syn` to validate generated snippets parse as expected syntactic category:

```rust
fn validate_item(code: &str) -> bool {
    syn::parse_str::<syn::Item>(code).is_ok()
}

fn validate_expr(code: &str) -> bool {
    syn::parse_str::<syn::Expr>(code).is_ok()
}
```

**Do not** use syn for editing. It drops comments and normalizes formatting.

## cfg Handling

`#[cfg(...)]` means the same item can exist multiple times with different bodies.

**Policy:**
1. Compiler diagnostics point at the active cfg variant for the current build. This is usually what you want.
2. For cross-platform correctness, run a build matrix:
   ```bash
   cargo check --target x86_64-unknown-linux-gnu
   cargo check --target x86_64-pc-windows-msvc
   cargo check --target aarch64-apple-darwin
   ```
3. If patching one cfg variant, consider whether the same fix applies to others. This is a policy decision, not something the patcher can infer.

## Workspace Handling

In a Cargo workspace:

```rust
fn resolve_diagnostic_path(
    diag_path: &str,
    workspace_root: &Path,
    current_crate_root: &Path,
) -> Option<PathBuf> {
    let path = Path::new(diag_path);
    
    // Absolute path
    if path.is_absolute() {
        return if path.starts_with(workspace_root) {
            Some(path.to_owned())
        } else {
            None // Outside workspace, refuse
        };
    }
    
    // Relative path - resolve against workspace root, not CWD
    let resolved = workspace_root.join(path);
    if resolved.exists() && resolved.starts_with(workspace_root) {
        Some(resolved)
    } else {
        None
    }
}
```

## Safety Rails

### Hard Rules (Never Violate)

1. **Selector uniqueness.** If a structural query matches 0 or >1 locations, refuse to edit. No guessing.

2. **Before-text verification.** Always verify the text at `[byte_start, byte_end)` matches expectations before overwriting.

3. **No external edits.** Never modify files outside the workspace root. Check resolved paths.

4. **Parse validation.** After editing, re-parse with tree-sitter. If the file has ERROR nodes that weren't there before, roll back.

5. **Compile validation.** After editing, run `cargo check`. If new errors appear (that weren't in the original diagnostic set), flag for review.

6. **Proc-macro stdout resilience.** When parsing cargo JSON output, filter to lines starting with `{`. Expect garbage.

### Soft Rules (Override with Explicit Flag)

1. **Don't edit macro definitions** unless `--allow-macro-def-edit` is set.

2. **Don't edit files with parse errors** unless `--allow-broken-input` is set. (Sometimes you're trying to fix the parse error.)

3. **Single edit per file per pass** unless `--batch-edits` is set. Reduces blast radius.

## Interface with LLM/Codex

### Input to Patcher

```rust
enum PatchRequest {
    /// LLM provides exact byte range (e.g., from prior diagnostic)
    ExactSpan {
        file: PathBuf,
        byte_start: usize,
        byte_end: usize,
        new_text: String,
    },
    
    /// LLM provides structural description
    Structural {
        file: PathBuf,
        target: StructuralTarget,
        operation: StructuralOp,
    },
    
    /// LLM provides unified diff (fallback, least reliable)
    UnifiedDiff {
        diff: String,
    },
}

enum StructuralTarget {
    Function { name: String, impl_of: Option<String> },
    Struct { name: String },
    Impl { for_type: String, trait_name: Option<String> },
    Module { path: Vec<String> },
    // etc.
}

enum StructuralOp {
    Replace { new_text: String },
    InsertBefore { text: String },
    InsertAfter { text: String },
    Delete,
    AddDerive { derive: String },
    AddAttribute { attr: String },
    // etc.
}
```

### Output from Patcher

```rust
enum PatchResult {
    Applied {
        file: PathBuf,
        edits: Vec<AppliedEdit>,
    },
    Refused {
        reason: RefusalReason,
    },
}

enum RefusalReason {
    SelectorAmbiguous { matches: usize },
    BeforeTextMismatch { expected: String, found: String },
    OutsideWorkspace { path: PathBuf },
    ParseErrorIntroduced { error_nodes: Vec<String> },
    CompileErrorIntroduced { diagnostics: Vec<String> },
    MacroExpansionTarget,
    GeneratedCodeTarget,
}
```

## Dependencies

```toml
[dependencies]
tree-sitter = "0.22"
tree-sitter-rust = "0.21"  # Pin deliberately, check edition support
ast-grep-core = "0.20"
cargo_metadata = "0.18"
syn = { version = "2", features = ["full", "parsing"] }  # Validation only
serde = { version = "1", features = ["derive"] }
serde_json = "1"
filetime = "0.2"
tempfile = "3"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
```

## Testing Strategy

1. **Golden file tests.** Input file + patch request → expected output file. Lots of these.

2. **Macro edge cases.** Files with `#[derive]`, `macro_rules!`, proc macros. Ensure patcher refuses or handles correctly.

3. **cfg variants.** Files with `#[cfg(unix)]` / `#[cfg(windows)]` duplicate items.

4. **Multi-edit ordering.** Multiple edits to same file, verify span translation works.

5. **Adversarial inputs.** Malformed JSON in cargo output, paths outside workspace, stale spans.

6. **Round-trip preservation.** Edit → compile → no change to untouched regions (comments, whitespace).

## Non-Goals

- **Semantic analysis.** No type inference, trait resolution, or borrow checking. Compiler does that.
- **Cross-file refactors.** Single-file edits only. Multi-file coordination is a higher-level concern.
- **IDE integration.** This is a batch/CLI tool, not an LSP server.
- **Formatting.** Run `rustfmt` after edits if desired. Patcher preserves existing formatting, doesn't impose.
