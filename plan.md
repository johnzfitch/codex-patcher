# Codex Patcher: Automated Patch System for Rust

## Executive Summary

Build `codex-patcher`, a standalone CLI tool that automates applying privacy and performance patches to Codex after upstream releases. Uses byte-span replacement as the primitive operation, with tree-sitter + ast-grep for structural code queries. Preserves formatting and comments, validates before/after, and reports conflicts when upstream changes break patches.

**Timeline**: 4 weeks
**Location**: `~/dev/codex-patcher/` (standalone crate)
**Architecture**: Per `rust-codex-patcher-spec.md`

## Problem Statement

Each time OpenAI releases a new Codex version (e.g., `rust-v0.88.0` -> `rust-v0.89.0`), you need to:
1. Merge the upstream release
2. Manually resolve conflicts in Cargo.toml, config files, telemetry code
3. Re-apply privacy patches (Statsig telemetry removal)
4. Re-apply performance optimizations (zack profile, Zen 5 flags)

This is error-prone and tedious. We need an automated patcher.

## Solution: `codex-patcher` CLI Tool

A Rust CLI tool that:
1. **Defines patches declaratively** in TOML config
2. **Uses byte-span replacement** as the primitive operation (compiler-first approach)
3. **Detects patch locations** using tree-sitter CST + ast-grep patterns (not brittle line numbers)
4. **Applies patches idempotently** (safe to run multiple times)
5. **Reports conflicts** when upstream changes break patch targets
6. **Validates edits** via tree-sitter parse checks and cargo check
7. **Supports version pinning** (patches can target specific version ranges)

## Core Architecture (Per rust-codex-patcher-spec.md)

### Primitive: Byte-Span Replacement

All operations compile to:
```rust
struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    expected_before: EditVerification,
}

enum EditVerification {
    ExactMatch(String),
    Hash(u64),  // xxhash3
}
```

**Key principle**: Intelligence lives in span acquisition, not application. The edit applicator is simple and verifiable.

### Three-Layer Architecture

```
codex-patcher/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Patch definition parsing
│   ├── edit.rs              # Edit primitive + application
│   ├── span_locator/
│   │   ├── mod.rs
│   │   ├── tree_sitter.rs   # CST-based structural queries
│   │   ├── ast_grep.rs      # Pattern-based matching
│   │   └── toml.rs          # TOML structure matching (toml_edit)
│   ├── validator/
│   │   ├── mod.rs
│   │   ├── parse.rs         # Tree-sitter + syn parse validation
│   │   └── compile.rs       # cargo check integration
│   └── safety.rs            # Workspace boundaries, selector uniqueness
└── patches/
    ├── privacy.toml         # Privacy patches (Statsig removal)
    ├── performance.toml     # Build profile optimizations
    └── README.md
```

## Patch Configuration Format

Patches use structural queries that compile to byte-span replacements:

```toml
# patches/privacy.toml
[meta]
name = "privacy-patches"
description = "Remove hardcoded telemetry to ab.chatgpt.com"
version_range = ">=0.88.0"
workspace_relative = true  # Paths relative to workspace root

[[patches]]
id = "disable-statsig-resolver"
file = "codex-rs/otel/src/config.rs"

# ast-grep pattern for function matching
[patches.query]
type = "ast-grep"
pattern = '''
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    $$$BODY
}
'''

# Replace entire matched region
[patches.operation]
type = "replace"
text = '''
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => OtelExporter::None,
        _ => exporter.clone(),
    }
}
'''

# Verification: hash of original function body
[patches.verify]
method = "exact_match"
expected_text = '''
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => {
            if cfg!(test) || cfg!(feature = "disable-default-metrics-exporter") {
                return OtelExporter::None;
            }

            OtelExporter::OtlpHttp {
                endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
                headers: HashMap::from([(
                    STATSIG_API_KEY_HEADER.to_string(),
                    STATSIG_API_KEY.to_string(),
                )]),
                protocol: OtelHttpProtocol::Json,
                tls: None,
            }
        }
        _ => exporter.clone(),
    }
}
'''

[[patches]]
id = "remove-statsig-constants"
file = "codex-rs/otel/src/config.rs"

# Tree-sitter query for const declarations
[patches.query]
type = "tree-sitter"
pattern = '''
(const_item
  name: (identifier) @name
  (#match? @name "^STATSIG_"))
'''

[patches.operation]
type = "delete"
insert_comment = "// PRIVACY PATCH: Statsig telemetry disabled (removed hardcoded constants)"

[[patches]]
id = "default-metrics-none-types"
file = "codex-rs/core/src/config/types.rs"

# ast-grep for field in Default impl
[patches.query]
type = "ast-grep"
pattern = '''
impl Default for OtelConfig {
    fn default() -> Self {
        OtelConfig {
            $$$
            metrics_exporter: $EXPORTER,
            $$$
        }
    }
}
'''

[patches.operation]
type = "replace_capture"
capture = "EXPORTER"
text = "OtelExporterKind::None"
add_comment = "// PRIVACY PATCH: Changed from Statsig to None"

[[patches]]
id = "default-metrics-none-mod"
file = "codex-rs/core/src/config/mod.rs"

[patches.query]
type = "ast-grep"
pattern = '''
OtelConfig {
    $$$
    metrics_exporter: $EXPORTER,
    $$$
}
'''
# Note: This might match multiple locations - we need line/context constraints

[patches.operation]
type = "replace_capture"
capture = "EXPORTER"
text = "OtelExporterKind::None"
add_comment = "// PRIVACY PATCH: Changed from Statsig to None"

# Context filter: only in specific function
[patches.constraint]
function_context = "load_config"
```

```toml
# patches/performance.toml
[meta]
name = "zack-performance-profile"
description = "Zen 5 optimized build profile with privacy patches"
version_range = ">=0.88.0"

[[patches]]
id = "add-zack-profile"
file = "codex-rs/Cargo.toml"

# TOML-specific query via toml_edit
[patches.query]
type = "toml"
section = "profile.zack"
ensure_absent = true  # Only apply if section doesn't exist

[patches.operation]
type = "insert_section"
after_section = "profile.ci-test"  # Insert after this section
text = '''
# =============================================================================
# ZACK PROFILE - Maximum performance + full debugging capabilities (2026)
# =============================================================================
# Purpose: Performance testing, profiling, benchmarking on Zen 5
#
# Build:     RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
# Profiling: RUSTFLAGS="-C target-cpu=znver5 -C force-frame-pointers=yes" cargo build --profile zack
# Run:       ./target/zack/codex
#
# Features:
# - Full LTO for maximum cross-crate optimization
# - Debug symbols for flamegraphs/perf/gdb (no runtime cost)
# - Zen 5 CPU targeting with AVX-512 support
# - Panic=unwind for catch_unwind compatibility (exec/tui use it)
# - Optimizes all dependencies aggressively
#
# Privacy Patches Applied (0.88.0):
# ✓ Disabled Statsig telemetry (was: hardcoded API key to ab.chatgpt.com)
# ✓ Changed metrics_exporter default from Statsig to None
# ✓ Removed hardcoded Statsig API key constant (STATSIG_API_KEY)
# ✓ User config respected: [analytics] enabled = false
#
# Additional Privacy Controls:
# - Runtime: Set OTEL_SDK_DISABLED=true environment variable
# - Config: Add [analytics] enabled = false to ~/.codex/config.toml
#
# Optional: Add -C force-frame-pointers=yes when actively profiling with perf
# (1-2% cost but faster profiling - DWARF unwinding works fine without it)
# =============================================================================
[profile.zack]
inherits = "release"
lto = "fat"                    # Maximum link-time optimization
codegen-units = 1              # Best optimization (single codegen unit)
opt-level = 3                  # Maximum optimization level
strip = false                  # Keep symbols for profiling/debugging
debug = 2                      # Full debug info (flamegraphs, perf, gdb)
# panic = "unwind"             # Default - required for catch_unwind in exec/tui
overflow-checks = false        # Disable overflow checks for max speed

[profile.zack.build-override]
opt-level = 3                  # Optimize build scripts and proc-macros

[profile.zack.package."*"]
opt-level = 3                  # Optimize all dependencies (tokio, serde, etc.)
'''

[[patches]]
id = "cargo-config-zen5"
file = "codex-rs/.cargo/config.toml"

[patches.query]
type = "toml"
section = "target.x86_64-unknown-linux-gnu"
ensure_absent = true

[patches.operation]
type = "append_section"
text = '''
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "target-cpu=znver5", "-C", "link-arg=-fuse-ld=mold"]
'''
```

## Key Design Decisions

### 1. Byte-Span Primitive (Per Spec)

**Why**: All high-level operations (AST transforms, structural edits) compile to a single, verifiable primitive.

**How**:
```rust
struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    expected_before: EditVerification,
}
```

**Guarantee**: Verify `file[byte_start..byte_end]` matches `expected_before` before overwriting. No silent corruption.

### 2. Tree-Sitter + ast-grep (Not syn)

**Why**:
- `syn` drops comments and normalizes formatting
- Tree-sitter preserves exact source text (CST not AST)
- ast-grep provides pattern-matching DSL on top of tree-sitter
- Codex already depends on tree-sitter

**How**:
- **Locating spans**: ast-grep patterns or tree-sitter queries
- **Validation only**: Use `syn::parse_str` to validate generated snippets
- **TOML**: Use `toml_edit` for structure-preserving edits

### 3. Compiler-First Approach

When available, prefer compiler diagnostics:
```rust
cargo check --message-format=json
```

Compiler provides:
- Exact byte spans (`byte_start`, `byte_end`)
- Suggested fixes (`MachineApplicable`)
- Macro expansion context

For our use case (patching after upstream merge):
- Tree-sitter/ast-grep for structural location
- Compiler diagnostics for post-patch verification

### 4. Idempotent Operations

Every patch checks if already applied:
```rust
fn apply_patch(patch: &Patch) -> Result<PatchResult> {
    // Query for match location
    let span = locate_span(&patch.query)?;

    // Read current text
    let current = read_span(&patch.file, span)?;

    // Check if already patched
    if current == patch.operation.text {
        return Ok(PatchResult::AlreadyApplied);
    }

    // Verify expected before applying
    if let Some(expected) = &patch.verify {
        if !expected.matches(&current) {
            return Err(Error::BeforeTextMismatch { expected, found: current });
        }
    }

    // Apply edit
    apply_edit(Edit { ... })?;
    Ok(PatchResult::Applied)
}
```

### 5. Safety Rails (Hard Rules)

1. **Selector uniqueness**: Query must match exactly 1 location. 0 or >1 = refuse
2. **Before-text verification**: Always verify via `expected_before`
3. **Workspace boundaries**: Never edit files outside workspace root
4. **Parse validation**: Re-parse with tree-sitter after edit. ERROR nodes = rollback
5. **Multi-edit ordering**: Sort by `byte_start` descending, apply bottom-to-top

### 6. Version-Aware Patching

Patches specify version ranges:
```toml
version_range = ">=0.88.0, <0.90.0"
```

The patcher:
1. Reads `workspace.package.version` from Cargo.toml
2. Filters patches by version constraint
3. Skips inapplicable patches (reports as info, not error)

### 7. Dry-Run & Diff Output

```bash
codex-patcher apply --dry-run --diff
```

Shows unified diff of changes before modifying files.

### 8. Conflict Detection

When a patch target is missing or changed:
```
CONFLICT: patch "disable-statsig-resolver"
  File: codex-rs/otel/src/config.rs
  Query matched: 0 locations
  Expected: function resolve_exporter

  Possible causes:
  - Function was renamed/removed
  - Signature changed
  - Moved to different file

  Action required: Update patch query or remove patch
```

When expected text doesn't match:
```
MISMATCH: patch "default-metrics-none-types"
  File: codex-rs/core/src/config/types.rs
  Location: line 403 (byte 12450-12510)
  Expected: metrics_exporter: OtelExporterKind::Statsig
  Found:    metrics_exporter: OtelExporterKind::None

  Status: Patch appears already applied
```

## CLI Interface

```bash
# Apply all patches in patches/
codex-patcher apply --workspace ~/dev/codex/codex-rs

# Apply specific patch file
codex-patcher apply --workspace ~/dev/codex/codex-rs --patches patches/privacy.toml

# Dry run with diff output
codex-patcher apply --dry-run --diff

# Check patch status without applying
codex-patcher status --workspace ~/dev/codex/codex-rs

# Verify patches after upstream merge
codex-patcher verify --workspace ~/dev/codex/codex-rs

# List available patches and their version constraints
codex-patcher list
```

## Workflow: Updating to New Codex Release

### Scenario: OpenAI releases rust-v0.89.0

**Step 1: Fetch upstream**
```bash
cd ~/dev/codex
git fetch origin
git tag | grep rust-v0.89
```

**Step 2: Create merge branch**
```bash
git checkout zack
git checkout -b merge-v0.89.0
git merge rust-v0.89.0
```

**Step 3: Resolve conflicts (if any)**
```bash
# Manual conflict resolution in Cargo.toml, etc.
git add .
git commit -m "Merge rust-v0.89.0 into zack"
```

**Step 4: Apply patches**
```bash
cd ~/dev/codex-patcher
cargo run -- apply --workspace ~/dev/codex/codex-rs
```

**Step 5: Handle conflicts**
If patcher reports conflicts:
```
CONFLICT: patch "disable-statsig-resolver"
  File: codex-rs/otel/src/config.rs
  Query matched: 0 locations
```

Options:
a. **Upstream removed the code**: Patch is obsolete, remove it
b. **Upstream renamed/moved**: Update patch query
c. **Upstream refactored**: Rewrite patch for new structure

**Step 6: Verify**
```bash
cd ~/dev/codex/codex-rs
cargo check --workspace
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
strings target/zack/codex | grep -v "ab.chatgpt.com"  # Verify no telemetry
```

**Step 7: Commit**
```bash
cd ~/dev/codex
git add .
git commit -m "Apply privacy and performance patches to v0.89.0"
```

**Step 8: Merge to zack**
```bash
git checkout zack
git merge --ff-only merge-v0.89.0
git branch -d merge-v0.89.0
```

## Complete Example: Privacy Patch Walkthrough

Let's trace how the "disable-statsig-resolver" patch works:

### 1. Original Code (rust-v0.88.0-alpha.4)

`codex-rs/otel/src/config.rs`:
```rust
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => {
            if cfg!(test) || cfg!(feature = "disable-default-metrics-exporter") {
                return OtelExporter::None;
            }

            OtelExporter::OtlpHttp {
                endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
                headers: HashMap::from([(
                    STATSIG_API_KEY_HEADER.to_string(),
                    STATSIG_API_KEY.to_string(),
                )]),
                protocol: OtelHttpProtocol::Json,
                tls: None,
            }
        }
        _ => exporter.clone(),
    }
}
```

### 2. Patch Definition

`patches/privacy.toml`:
```toml
[[patches]]
id = "disable-statsig-resolver"
file = "codex-rs/otel/src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub(crate) fn resolve_exporter($$$PARAMS) -> $RET {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => OtelExporter::None,
        _ => exporter.clone(),
    }
}
'''

[patches.verify]
method = "hash"
hash = "0xABCD1234"  # xxh3 of original function body
```

### 3. Patcher Execution

```rust
// Step 1: Load patch definition
let patch = Patch::from_file("patches/privacy.toml")?;

// Step 2: Read target file
let source = fs::read_to_string("codex-rs/otel/src/config.rs")?;

// Step 3: Run ast-grep query
let matcher = AstGrepMatcher::new(&patch.query.pattern)?;
let matches = matcher.find_all(&source)?;

// Check uniqueness
if matches.len() != 1 {
    return Err(Error::SelectorNotUnique {
        expected: 1,
        found: matches.len()
    });
}

let span = matches[0].byte_range();  // e.g., (450, 750)

// Step 4: Verify expected text
let current_text = &source[span.start..span.end];
let expected_hash = xxh3_64(current_text.as_bytes());

if expected_hash != patch.verify.hash {
    return Err(Error::BeforeTextMismatch {
        expected: format!("hash {}", patch.verify.hash),
        found: format!("hash {}", expected_hash),
    });
}

// Step 5: Create Edit
let edit = Edit {
    file: PathBuf::from("codex-rs/otel/src/config.rs"),
    byte_start: span.start,
    byte_end: span.end,
    new_text: patch.operation.text.clone(),
    expected_before: EditVerification::Hash(patch.verify.hash),
};

// Step 6: Apply atomically
apply_edit(&edit)?;

// Step 7: Validate
let new_source = fs::read_to_string(&edit.file)?;
let tree = parse_rust(&new_source)?;
if tree.root_node().has_error() {
    rollback(&edit)?;
    return Err(Error::ParseErrorIntroduced);
}
```

### 4. Result

Same code structure, modified logic:
```rust
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => OtelExporter::None,  // ← Changed
        _ => exporter.clone(),
    }
}
```

### 5. Idempotency Check

When run again:
```rust
let current_text = &source[span.start..span.end];
if current_text == patch.operation.text {
    return Ok(PatchResult::AlreadyApplied);
}
```

Output: `✓ disable-statsig-resolver: Already applied`

### Scenario: Patch Becomes Obsolete

If upstream fixes telemetry themselves:

**Step 1: Check status**
```bash
codex-patcher status
```

Output:
```
OBSOLETE: patch "disable-statsig-resolver"
  Reason: Expected code not found (may already be fixed upstream)
```

**Step 2: Verify manually**
```bash
grep "OtelExporter::Statsig" ~/dev/codex/codex-rs/otel/src/config.rs
# If not found, patch is obsolete
```

**Step 3: Update patch definitions**
Remove obsolete patch from `patches/privacy.toml`, or mark it:
```toml
[[patches]]
id = "disable-statsig-resolver"
obsolete_after = "0.89.0"  # Patcher will skip this
```

## Implementation Steps

### Phase 1: Core Edit Primitive (Week 1)
1. Create `codex-patcher` crate in `~/dev/codex-patcher/`
2. Implement `Edit` struct with byte-span replacement
3. Implement `EditVerification` (ExactMatch + Hash)
4. Implement atomic file writes (tempfile + fsync + rename)
5. Add workspace boundary checks
6. Test: Golden file tests for basic byte-span replacement

**Deliverable**: Can apply raw byte-span edits with verification

### Phase 2: TOML Patching (Week 1-2)
7. Integrate `toml_edit` for structure-preserving TOML edits
8. Implement TOML query language (section path, key matching)
9. Add operations: insert_section, append_section, replace_value
10. Compile TOML queries → byte spans
11. Test: Add/modify Cargo.toml profiles preserving formatting

**Deliverable**: Can patch Cargo.toml and .cargo/config.toml

### Phase 3: Tree-Sitter Span Locator (Week 2)
12. Integrate `tree-sitter` + `tree-sitter-rust` (pin edition-aware version)
13. Implement tree-sitter query engine
14. Add span extraction from query matches
15. Test: Locate function/const/impl blocks by name

**Deliverable**: Can locate Rust code structures and extract byte spans

### Phase 4: ast-grep Integration (Week 2-3)
16. Integrate `ast-grep-core` for pattern matching
17. Implement pattern → byte span conversion
18. Add capture group replacement (e.g., `$EXPORTER`)
19. Handle context constraints (function_context, etc.)
20. Test: Pattern-based matching for field assignments

**Deliverable**: Can match complex Rust patterns and replace captures

### Phase 5: Validation & Safety (Week 3)
21. Implement parse validation (tree-sitter ERROR node detection)
22. Add `syn` validation for generated snippets
23. Implement selector uniqueness checks (0 or >1 matches = refuse)
24. Add multi-edit span translation (bottom-to-top application)
25. Test: Catch syntax errors, ambiguous selectors

**Deliverable**: Patcher refuses unsafe operations

### Phase 6: Patch Config Parser (Week 3)
26. Design TOML schema for patch definitions
27. Implement patch config parser (serde)
28. Add version range filtering (semver)
29. Implement idempotency checks
30. Test: Load patch suite, filter by version

**Deliverable**: Can load declarative patch definitions

### Phase 7: CLI & UX (Week 4)
31. Implement CLI with clap (apply, status, verify commands)
32. Add `--dry-run` and `--diff` output (using `similar` crate)
33. Implement conflict detection and reporting
34. Add progress indicators
35. Test: User-facing CLI workflows

**Deliverable**: Full CLI with good UX

### Phase 8: Initial Patch Definitions (Week 4)
36. Create `patches/privacy.toml` from current patches
37. Create `patches/performance.toml` for zack profile
38. Test on clean `rust-v0.88.0-alpha.4` checkout
39. Verify: patches apply cleanly, build succeeds
40. Document: Patch authoring guide

**Deliverable**: Production-ready patch definitions

### Phase 9: Integration & Documentation (Week 4)
41. Add `justfile` recipe: `just patch`
42. Create workflow doc: "Updating to New Codex Release"
43. Add CI test: Apply patches to upstream tags
44. Test: Full workflow from upstream merge to patched build

**Deliverable**: Documented, tested integration

## Files to Create

### Crate Structure
```
~/dev/codex-patcher/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Public API exports
│   ├── edit.rs              # Edit primitive + application
│   ├── config.rs            # Patch definition schema + parser
│   ├── span_locator/
│   │   ├── mod.rs
│   │   ├── tree_sitter.rs   # Tree-sitter query → spans
│   │   ├── ast_grep.rs      # ast-grep pattern → spans
│   │   └── toml.rs          # toml_edit query → spans
│   ├── validator/
│   │   ├── mod.rs
│   │   ├── parse.rs         # Tree-sitter + syn validation
│   │   └── compile.rs       # cargo check integration (future)
│   └── safety.rs            # Workspace checks, uniqueness
├── patches/
│   ├── privacy.toml         # Statsig removal patches
│   ├── performance.toml     # Build profile patches
│   └── README.md            # Patch authoring guide
└── tests/
    ├── integration_tests.rs
    └── fixtures/
        ├── input/           # Clean codex sources
        └── expected/        # Expected patched output
```

### Integration (Optional)
- `~/dev/codex/codex-rs/justfile` - Add `just patch` recipe
- `~/dev/codex/.github/workflows/test-patches.yml` - CI test for patches

## Dependencies (Per Spec)

```toml
[dependencies]
# Core patching
tree-sitter = "0.22"
tree-sitter-rust = "0.21"      # Pin deliberately, check edition 2024 support
ast-grep-core = "0.20"          # Pattern-based matching
toml_edit = "0.24"              # Structure-preserving TOML edits

# Validation
syn = { version = "2", features = ["full", "parsing"] }  # Validation only, not editing
cargo_metadata = "0.18"         # Parse Cargo.toml workspace metadata

# Utilities
serde = { version = "1", features = ["derive"] }
serde_json = "1"
semver = "1"                    # Version range filtering
filetime = "0.2"                # Touch mtimes after edits
tempfile = "3"                  # Atomic file writes
xxhash-rust = { version = "0.8", features = ["xxh3"] }  # Fast hashing

# CLI
clap = { version = "4", features = ["derive"] }
similar = "2"                   # Unified diff generation
colored = "2"                   # Pretty output
walkdir = "2"                   # Recursive file traversal

# Error handling
thiserror = "2"
anyhow = "1"
```

**Note on tree-sitter-rust**: Verify it supports Rust edition 2024 syntax. If not, may need to:
- Use codex's bundled tree-sitter (it already has tree-sitter-rust 0.25)
- Or wait for tree-sitter-rust update
- Or patch tree-sitter-rust grammar ourselves

## End-to-End Verification

### Test Plan

**Setup**: Clean upstream checkout
```bash
cd ~/dev/codex
git checkout rust-v0.88.0-alpha.4
git reset --hard
git clean -fdx
```

**Apply patches**:
```bash
cd ~/dev/codex-patcher
cargo run -- apply --workspace ~/dev/codex/codex-rs
```

**Verify structural changes**:
```bash
# Privacy patches
grep -c "OtelExporter::None" ~/dev/codex/codex-rs/otel/src/config.rs  # Should be 2
! grep "STATSIG_OTLP_HTTP_ENDPOINT" ~/dev/codex/codex-rs/otel/src/config.rs  # Should not exist
grep -c "OtelExporterKind::None" ~/dev/codex/codex-rs/core/src/config/types.rs  # Should be 1
grep -c "OtelExporterKind::None" ~/dev/codex/codex-rs/core/src/config/mod.rs  # Should be 1

# Performance patches
grep -c "\[profile.zack\]" ~/dev/codex/codex-rs/Cargo.toml  # Should be 1
grep -c "opt-level = 3" ~/dev/codex/codex-rs/Cargo.toml  # Should be 3 (profile, build-override, package.*)
```

**Verify parse validity**:
```bash
cd ~/dev/codex/codex-rs
cargo check --workspace --all-features
```

**Verify telemetry disabled**:
```bash
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
strings target/zack/codex | grep -c "ab.chatgpt.com"  # Should be 0
```

**Verify idempotency**:
```bash
cd ~/dev/codex-patcher
cargo run -- apply --workspace ~/dev/codex/codex-rs  # Run again
# Should report: All patches already applied
```

### Success Criteria

1. ✅ All patches apply cleanly (no conflicts)
2. ✅ Structural verification grep checks pass
3. ✅ `cargo check` succeeds (no syntax errors)
4. ✅ Binary doesn't contain telemetry strings
5. ✅ `profile.zack` present and correct in Cargo.toml
6. ✅ Second application reports "already applied"
7. ✅ Comments and formatting preserved

## Alternatives Considered

### Git Patch Files (`git format-patch`)

**Rejected** because:
- Line-number based (fragile with upstream changes)
- Merge conflicts require manual resolution
- No semantic understanding
- Can't detect "already applied"

### Manual Sed Scripts

**Rejected** because:
- Extremely brittle
- No verification
- Line-based
- Can't handle structural queries

### Full AST Rewrite (syn)

**Rejected** because:
- Drops comments
- Normalizes formatting
- Over-engineered for our needs

### The Chosen Approach (Byte-Span + Tree-Sitter + ast-grep)

**Advantages**:
- Structural queries (robust to whitespace/line changes)
- Preserves formatting and comments
- Verification before application
- Idempotent
- Compiler-aware (can integrate diagnostics)
- Proven architecture (per rust-codex-patcher-spec.md)
