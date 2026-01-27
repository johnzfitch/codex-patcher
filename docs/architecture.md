# <img src="../.github/assets/icons/diagram.png" width="20" height="20" alt=""/> Architecture

Deep dive into Codex Patcher's design and implementation.

## <img src="../.github/assets/icons/eye.png" width="16" height="16" alt=""/> Design Philosophy

### Core Principle: Single Primitive

All edit operations compile down to a single primitive: **byte-span replacement**.

```
┌─────────────────────────────────────────────────────────────┐
│                    High-Level Operations                     │
├─────────────┬─────────────┬─────────────┬───────────────────┤
│  ast-grep   │ tree-sitter │    TOML     │  Unified Diff     │
│  Patterns   │   Queries   │  Operations │    (future)       │
└──────┬──────┴──────┬──────┴──────┬──────┴─────────┬─────────┘
       │             │             │                │
       └─────────────┴─────────────┴────────────────┘
                           │
                           ▼
              ┌───────────────────────┐
              │    Edit Primitive     │
              │  (byte-span replace)  │
              └───────────────────────┘
                           │
                           ▼
              ┌───────────────────────┐
              │    Atomic Write       │
              │ (tempfile + rename)   │
              └───────────────────────┘
```

**Why this design?**

1. **Simplicity**: One code path for all edits
2. **Verifiability**: Easy to inspect exactly what changed
3. **Flexibility**: Any span acquisition strategy works
4. **Preservation**: Comments and formatting untouched

---

## <img src="../.github/assets/icons/layers.png" width="16" height="16" alt=""/> Module Structure

```
codex-patcher/
├── src/
│   ├── lib.rs              # Public API re-exports
│   ├── main.rs             # CLI entry point
│   │
│   ├── edit.rs             # Core Edit primitive
│   │   ├── Edit            # Byte-span replacement
│   │   ├── EditVerification # Exact match or hash
│   │   └── atomic_write    # Crash-safe file writes
│   │
│   ├── safety.rs           # Workspace boundaries
│   │   └── WorkspaceGuard  # Path validation
│   │
│   ├── validate.rs         # Parse validation
│   │   ├── ParseValidator  # Tree-sitter error detection
│   │   ├── syn_validate    # syn-based snippet checks
│   │   └── ValidatedEdit   # Edit with validation
│   │
│   ├── config/             # Patch configuration
│   │   ├── schema.rs       # PatchConfig, PatchDefinition
│   │   ├── loader.rs       # TOML parsing
│   │   ├── applicator.rs   # Patch application logic
│   │   └── version.rs      # Semver filtering
│   │
│   ├── ts/                 # Tree-sitter integration
│   │   ├── parser.rs       # RustParser wrapper
│   │   ├── query.rs        # QueryEngine
│   │   ├── locator.rs      # StructuralLocator
│   │   └── validator.rs    # Parse validation
│   │
│   ├── sg/                 # ast-grep integration
│   │   ├── lang.rs         # Language support
│   │   ├── matcher.rs      # PatternMatcher
│   │   └── replacer.rs     # CaptureReplacer
│   │
│   └── toml/               # TOML editing
│       ├── editor.rs       # TomlEditor
│       ├── query.rs        # SectionPath, KeyPath
│       ├── operations.rs   # TomlOperation
│       └── validator.rs    # TOML validation
```

---

## <img src="../.github/assets/icons/shield-security-protection-16x16.png" width="16" height="16" alt=""/> Safety Model

### Workspace Boundaries

```
                    ┌─────────────────────────────┐
                    │     WorkspaceGuard          │
                    └─────────────────────────────┘
                                 │
        ┌────────────────────────┼────────────────────────┐
        │                        │                        │
        ▼                        ▼                        ▼
  ┌───────────┐          ┌───────────┐          ┌───────────┐
  │ Canonicalize│         │   Check   │          │  Check    │
  │   Path     │          │  Inside   │          │ Forbidden │
  └───────────┘          │ Workspace │          │   Paths   │
                         └───────────┘          └───────────┘
```

**Forbidden directories:**
- `~/.cargo/registry` - Dependency sources
- `~/.cargo/git` - Git dependencies
- `~/.rustup` - Toolchains
- `{workspace}/target` - Build artifacts

**Symlink handling:**
- All paths canonicalized before validation
- Symlinks pointing outside workspace rejected
- Directory traversal (../) blocked

### Edit Verification

```
     ┌──────────────┐
     │  Read File   │
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │ Extract Span │
     │ [start:end]  │
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐     NO     ┌──────────────┐
     │   Matches    │──────────►│    ABORT     │
     │  Expected?   │            │  (stale span)│
     └──────┬───────┘            └──────────────┘
            │ YES
            ▼
     ┌──────────────┐     YES    ┌──────────────┐
     │   Already    │──────────►│   SKIP       │
     │  Applied?    │            │ (idempotent) │
     └──────┬───────┘            └──────────────┘
            │ NO
            ▼
     ┌──────────────┐
     │  Apply Edit  │
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │ Atomic Write │
     └──────────────┘
```

### Atomic Writes

```rust
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    // 1. Create tempfile in same directory (same filesystem)
    let temp = NamedTempFile::new_in(parent)?;

    // 2. Write content
    temp.write_all(content)?;

    // 3. Flush to disk (fsync)
    temp.as_file().sync_all()?;

    // 4. Atomic rename
    temp.persist(path)?;

    Ok(())
}
```

**Why this matters:**
- Power failure during write won't corrupt file
- Either old content or new content, never partial
- File system guarantees atomicity of rename

---

## <img src="../.github/assets/icons/tree.png" width="16" height="16" alt=""/> Span Acquisition

### Strategy 1: ast-grep Patterns

```
Source Code                 Pattern                    Matches
───────────────────────────────────────────────────────────────
let x = foo.clone();   +   $EXPR.clone()    →    [(4, 16)]
let y = bar.clone();                              [(23, 35)]
```

**Advantages:**
- Semantic matching (not text-based)
- Captures for templated replacement
- Robust to whitespace changes

### Strategy 2: Tree-sitter Queries

```
Source Code                 Query                      Span
───────────────────────────────────────────────────────────────
fn main() {             +   (function_item         →    (0, 45)
    println!("hi");         name: (identifier)
}                           @name)
```

**Advantages:**
- Full AST access
- Precise node selection
- Language-aware

### Strategy 3: TOML Sections

```
TOML Content                Query                      Span
───────────────────────────────────────────────────────────────
[package]               +   section="profile.release" →  (42, 78)
name = "foo"                key="opt-level"

[profile.release]
opt-level = 2
```

**Advantages:**
- Format-preserving
- Structure-aware
- Handles nested tables

---

## <img src="../.github/assets/icons/search.png" width="16" height="16" alt=""/> Pattern Matching

### ast-grep Metavariables

| Pattern | Matches | Captures |
|---------|---------|----------|
| `$NAME` | Single node | Named capture |
| `$$$NAME` | Zero+ nodes | Variadic capture |
| `$_` | Single node | No capture |
| Literal | Exact text | - |

### Example: Function Refactoring

```
Pattern: fn $NAME($$$PARAMS) -> $RET { $$$BODY }

Matches:
  fn process(data: &[u8], len: usize) -> Result<(), Error> {
      validate(data)?;
      compute(data, len)
  }

Captures:
  $NAME = "process"
  $$$PARAMS = "data: &[u8], len: usize"
  $RET = "Result<(), Error>"
  $$$BODY = "validate(data)?;\ncompute(data, len)"
```

---

## <img src="../.github/assets/icons/gear-24x24.png" width="16" height="16" alt=""/> Configuration Flow

```
┌─────────────────┐
│  patches/*.toml │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   load_from_    │
│     path()      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  PatchConfig    │
│  ┌───────────┐  │
│  │   Meta    │  │
│  │───────────│  │
│  │  Patches  │  │
│  └───────────┘  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  apply_patches  │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌───────┐ ┌───────┐
│Version│ │ Apply │
│Filter │ │ Patch │
└───────┘ └───────┘
              │
         ┌────┴────┐
         │         │
         ▼         ▼
    ┌─────────┐ ┌─────────┐
    │ast-grep │ │  TOML   │
    │ Query   │ │  Query  │
    └────┬────┘ └────┬────┘
         │           │
         └─────┬─────┘
               │
               ▼
         ┌───────────┐
         │   Edit    │
         │ Primitive │
         └───────────┘
```

---

## <img src="../.github/assets/icons/lightning.png" width="16" height="16" alt=""/> Performance Considerations

### Memory Usage

| Component | Memory | Notes |
|-----------|--------|-------|
| Tree-sitter parser | ~2 MB | Reused across parses |
| Parsed tree | ~10x source | Freed after use |
| Edit buffer | 1x file size | One file at a time |
| Pattern compilation | ~1 KB/pattern | Cached |

### Optimization Strategies

1. **Parser reuse**: Single parser instance for multiple files
2. **Lazy loading**: Patterns compiled on demand
3. **Batch edits**: Multiple edits to same file in one pass
4. **Bottom-up application**: Avoids offset recalculation

### Batch Edit Algorithm

```
Given edits: [(10, 15, "X"), (5, 8, "Y"), (20, 25, "Z")]

1. Sort by byte_start descending:
   [(20, 25, "Z"), (10, 15, "X"), (5, 8, "Y")]

2. Apply from bottom to top:
   - Apply (20, 25, "Z") → offsets 0-19 unchanged
   - Apply (10, 15, "X") → offsets 0-9 unchanged
   - Apply (5, 8, "Y")   → offsets 0-4 unchanged

Result: No offset invalidation!
```

---

## <img src="../.github/assets/icons/tick.png" width="16" height="16" alt=""/> Validation Layers

### Layer 1: Before-text Verification

```rust
// Check expected text matches
if !verification.matches(&current_text) {
    return Err(EditError::BeforeTextMismatch { ... });
}
```

### Layer 2: UTF-8 Validation

```rust
// Ensure result is valid UTF-8
std::str::from_utf8(&new_content)?;
```

### Layer 3: Parse Validation

```rust
// Check for new parse errors
let mut validator = ParseValidator::new()?;
validator.validate_edit(original, edited)?;
```

### Layer 4: syn Validation (snippets)

```rust
// Validate generated code
syn_validate::validate_item("fn foo() {}")?;
```

---

## <img src="../.github/assets/icons/book.png" width="16" height="16" alt=""/> Design Decisions

### Why Not Unified Diffs?

| Aspect | Unified Diff | Byte-Span |
|--------|--------------|-----------|
| Fragility | High (line-based) | Low (verified) |
| Merge conflicts | Frequent | Impossible |
| Idempotency | Manual | Automatic |
| Semantic understanding | None | Pattern-based |

### Why Not syn for Editing?

| Aspect | syn | Tree-sitter |
|--------|-----|-------------|
| Comments | Dropped | Preserved |
| Formatting | Normalized | Preserved |
| Complexity | Over-engineered | Right-sized |
| Use case | Parsing | Editing |

### Why Both ast-grep and Tree-sitter?

| Use Case | Tool |
|----------|------|
| Pattern matching | ast-grep |
| Low-level queries | Tree-sitter |
| Parse validation | Tree-sitter |
| Structural location | Both |

---

## <img src="../.github/assets/icons/folder.png" width="16" height="16" alt=""/> See Also

- [API Reference](api.md)
- [Patch Authoring](patches.md)
- [TOML Patching](toml.md)
- [CLAUDE.md](../CLAUDE.md) - Full technical specification
