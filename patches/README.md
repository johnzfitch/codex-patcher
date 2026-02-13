# Codex Patcher: Patch Authoring Guide

This guide explains how to write, test, and maintain patches for the Codex Rust codebase.

## Table of Contents

- [Patch Structure](#patch-structure)
- [Query Types](#query-types)
- [Operation Types](#operation-types)
- [Verification Methods](#verification-methods)
- [Testing Patches](#testing-patches)
- [Best Practices](#best-practices)
- [Examples](#examples)

## Patch Structure

Each patch file is a TOML document with metadata and an array of patch definitions:

```toml
[meta]
name = "patch-set-name"
description = "What these patches do"
version_range = ">=0.88.0"    # Semver constraint
workspace_relative = true      # Paths relative to workspace root

[[patches]]
id = "unique-patch-id"
file = "relative/path/to/file.rs"
query = { ... }
operation = { ... }
verify = { ... }              # Optional
constraint = { ... }          # Optional
```

### Metadata Fields

- **name**: Human-readable patch set name
- **description**: What the patches accomplish
- **version_range**: Semver version constraint (e.g., `">=0.88.0, <0.90.0"`)
- **workspace_relative**: If true, file paths are relative to workspace root

### Patch Fields

- **id**: Unique identifier for this patch (used in output messages)
- **file**: Path to target file (absolute or workspace-relative)
- **query**: How to locate the code to patch
- **operation**: What to do at that location
- **verify**: Optional verification before applying
- **constraint**: Optional additional constraints

## Query Types

Queries locate code to patch. The patcher supports three query types:

### 1. AST-Grep (Recommended for Rust Code)

Pattern-based matching with metavariables:

```toml
[patches.query]
type = "ast-grep"
pattern = '''
pub fn $FUNC_NAME($$$PARAMS) -> $RETURN {
    $$$BODY
}
'''
```

**Metavariables:**
- `$VAR` - Matches any single AST node
- `$$$VAR` - Matches zero or more nodes (like `...` in regex)

**When to use:**
- Matching Rust functions, structs, impl blocks
- Replacing function bodies
- Finding specific code patterns

**Example patterns:**

```toml
# Match any function named "resolve_exporter"
pattern = '''
pub(crate) fn resolve_exporter($$$) -> $$ {
    $$$
}
'''

# Match struct field in Default impl
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

# Match const declaration
pattern = '''
pub(crate) const STATSIG_API_KEY: $$ = $$;
'''
```

### 2. Tree-Sitter (Low-Level Queries)

S-expression queries for precise node matching:

```toml
[patches.query]
type = "tree-sitter"
pattern = '''
(const_item
  name: (identifier) @name
  (#match? @name "^STATSIG_"))
'''
```

**When to use:**
- Complex structural queries
- Matching based on syntax node types
- Need predicates like `#match?`, `#eq?`

### 3. TOML (For Cargo.toml, config files)

Structure-preserving TOML edits:

```toml
[patches.query]
type = "toml"
section = "profile.zack"
key = "opt-level"              # Optional
ensure_absent = true           # Only apply if section doesn't exist
ensure_present = false         # Only apply if section exists
```

**When to use:**
- Modifying Cargo.toml
- Editing .cargo/config.toml
- Any TOML configuration file

## Operation Types

### Rust Code Operations

#### replace

Replace the entire matched region:

```toml
[patches.operation]
type = "replace"
text = '''
pub fn new_implementation() {
    // New code here
}
'''
```

#### replace-capture

Replace only a captured metavariable:

```toml
# Query captures $EXPORTER
[patches.query]
type = "ast-grep"
pattern = '''
metrics_exporter: $EXPORTER,
'''

# Replace just the $EXPORTER part
[patches.operation]
type = "replace-capture"
capture = "EXPORTER"
text = "OtelExporterKind::None"
```

#### delete

Delete the matched code:

```toml
[patches.operation]
type = "delete"
insert_comment = "// PRIVACY PATCH: Removed hardcoded API key"
```

### TOML Operations

#### insert-section

Insert a new section at a specific location:

```toml
[patches.operation]
type = "insert-section"
after_section = "profile.release"   # Position
text = '''
[profile.zack]
lto = "fat"
opt-level = 3
'''
```

Positioning options:
- `after_section = "path.to.section"`
- `before_section = "path.to.section"`
- `at_end = true`
- `at_beginning = true`

#### append-section

Append to end of file:

```toml
[patches.operation]
type = "append-section"
text = '''
[new.section]
key = "value"
'''
```

#### replace-value

Replace a TOML value:

```toml
# Requires query with key specified
[patches.query]
type = "toml"
section = "profile.release"
key = "lto"

[patches.operation]
type = "replace-value"
value = '"thin"'
```

#### replace-key

Rename a TOML key:

```toml
[patches.operation]
type = "replace-key"
new_key = "new_name"
```

#### delete-section

Delete a TOML section:

```toml
[patches.operation]
type = "delete-section"
```

## Verification Methods

Patches can verify expected content before applying:

### exact_match

Verify exact text at location:

```toml
[patches.verify]
method = "exact_match"
expected_text = '''
pub fn old_code() {
    // exact content
}
'''
```

### hash

Verify content hash (faster, less brittle):

```toml
[patches.verify]
method = "hash"
algorithm = "xxh3"
expected = "0xABCD1234567890"
```

**How to compute hash:**

```bash
# Use xxhsum (install: cargo install xxhash-rust)
echo -n "content to hash" | xxhsum -H3

# Or compute in Rust:
use xxhash_rust::xxh3::xxh3_64;
let hash = xxh3_64(content.as_bytes());
println!("{:#x}", hash);
```

## Testing Patches

### 1. Test on Clean Checkout

```bash
# Clone fresh upstream
git clone https://github.com/openai/codex /tmp/test-codex
cd /tmp/test-codex
git checkout rust-v0.88.0-alpha.4

# Apply patches
cd ~/dev/codex-patcher
cargo run -- apply --workspace /tmp/test-codex/codex-rs

# Verify it compiles
cd /tmp/test-codex/codex-rs
cargo check --workspace
```

### 2. Dry Run with Diff

```bash
cargo run -- apply --workspace ~/dev/codex/codex-rs --dry-run --diff
```

Shows what would change without modifying files.

### 3. Check Status

```bash
cargo run -- status --workspace ~/dev/codex/codex-rs
```

Shows which patches are applied, not applied, or skipped.

### 4. Verify Patch State

```bash
cargo run -- verify --workspace ~/dev/codex/codex-rs
```

Checks if all patches are correctly applied. Exits with error if any mismatches.

### 5. Verify Telemetry Removal

After applying privacy patches:

```bash
cd ~/dev/codex/codex-rs
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
strings target/zack/codex | grep -i statsig           # Should be empty
strings target/zack/codex | grep "ab.chatgpt.com"     # Should be empty
```

## Best Practices

### 1. Make Queries Specific

**Bad:** Matches too broadly
```toml
pattern = '''
fn $NAME($$$) {
    $$$
}
'''
```

**Good:** Specific enough to match exactly once
```toml
pattern = '''
pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    $$$
}
'''
```

### 2. Use Comments in Patches

Add context about why the patch exists:

```toml
text = '''
// PRIVACY PATCH: Disabled Statsig telemetry
// Original code sent metrics to ab.chatgpt.com without explicit consent
OtelExporter::None
'''
```

### 3. Version Constraints

Be specific about version ranges:

```toml
# Good - specific range
version_range = ">=0.88.0, <0.90.0"

# Acceptable - open-ended
version_range = ">=0.88.0"

# Avoid - too broad
version_range = "*"
```

### 4. Idempotency

All patches are automatically idempotent. The patcher checks if content already matches the target state before applying.

### 5. Test Before Committing

Always test patches on a clean checkout:

```bash
just test-patches  # If you add this to justfile
```

### 6. Document Breaking Changes

If upstream changes break a patch, document the fix in git:

```bash
git commit -m "fix(patches): update privacy patches for v0.89.0

Upstream refactored resolve_exporter into a trait method.
Updated query pattern to match new trait impl."
```

## Examples

### Example 1: Remove Hardcoded Constant

```toml
[[patches]]
id = "remove-api-key"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
const API_KEY: &str = $VALUE;
'''

[patches.operation]
type = "delete"
insert_comment = "// Removed hardcoded API key"
```

### Example 2: Change Default Value

```toml
[[patches]]
id = "change-default-timeout"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
impl Default for Config {
    fn default() -> Self {
        Config {
            $$$
            timeout: $OLD_VALUE,
            $$$
        }
    }
}
'''

[patches.operation]
type = "replace-capture"
capture = "OLD_VALUE"
text = "Duration::from_secs(30)"
```

### Example 3: Add Cargo Profile

```toml
[[patches]]
id = "add-custom-profile"
file = "Cargo.toml"

[patches.query]
type = "toml"
section = "profile.custom"
ensure_absent = true

[patches.operation]
type = "insert-section"
after_section = "profile.release"
text = '''
[profile.custom]
inherits = "release"
lto = "fat"
opt-level = 3
'''
```

### Example 4: Function Body Replacement

```toml
[[patches]]
id = "simplify-resolver"
file = "src/resolver.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn resolve(&self, name: &str) -> Option<String> {
    $$$
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn resolve(&self, name: &str) -> Option<String> {
    // Simplified implementation
    None
}
'''

[patches.verify]
method = "hash"
algorithm = "xxh3"
expected = "0x1234567890abcdef"
```

## Troubleshooting

### Query Matches Nothing

```
CONFLICT: patch "my-patch"
  Query matched: 0 locations
```

**Solutions:**
1. Check if the code structure changed upstream
2. Use `rg` to find the current code:
   ```bash
   rg "resolve_exporter" ~/dev/codex/codex-rs/
   ```
3. Update the query pattern to match current code

### Query Matches Multiple Locations

```
CONFLICT: patch "my-patch"
  Query matched: 3 locations (expected 1)
```

**Solutions:**
1. Make the pattern more specific (add more context)
2. Add constraints:
   ```toml
   [patches.constraint]
   function_context = "specific_function_name"
   ```
3. Split into multiple patches with unique queries

### Verification Failed

```
MISMATCH: Expected: metrics_exporter: OtelExporterKind::Statsig
          Found:    metrics_exporter: OtelExporterKind::None
```

**This means:**
- Patch is already applied (idempotent success)
- Or upstream already fixed the issue
- Or the patch was manually applied

**Action:** Run with `--dry-run` to see current state

### Parse Error After Applying

If the patcher reports a parse error, the edit introduced invalid syntax:

1. Check the `text` field for syntax errors
2. Test snippet compilation:
   ```bash
   echo "fn test() { YOUR_CODE_HERE }" | rustc -
   ```
3. Use `syn` validation before committing:
   ```rust
   syn::parse_str::<syn::Item>("your code here").unwrap();
   ```

## Advanced Topics

### Contextual Constraints

Limit matches to specific contexts:

```toml
[patches.constraint]
function_context = "load_config"
```

This only matches if the query is inside the `load_config` function.

### Multiple Files

Create multiple patch files for different concerns:

```
patches/
├── privacy.toml       # Privacy patches
├── memory-safety-regressions.toml # Memory controls + destructive-op confirmation for 0.101.x
├── performance.toml   # Performance optimizations
├── bugfixes.toml     # Bug fixes
└── README.md
```

Apply specific sets:

```bash
cargo run -- apply --patches patches/privacy.toml
```

### Version-Specific Patches

Mark patches as obsolete after a version:

```toml
[[patches]]
id = "old-fix"
obsolete_after = "0.89.0"  # Not yet implemented, but planned
# ... rest of patch
```

### Debugging Queries

To see what ast-grep matches:

```bash
# Install ast-grep CLI
cargo install ast-grep

# Test pattern
cat src/config.rs | sg -p 'pub fn $NAME($$$) { $$$ }' --lang rust
```

## Further Reading

- [ast-grep Pattern Syntax](https://ast-grep.github.io/guide/pattern-syntax.html)
- [tree-sitter Query Syntax](https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries)
- [TOML Specification](https://toml.io/en/v1.0.0)
- [Codex Patcher Spec](../spec.md)

## Contributing

When adding new patches:

1. Test on clean checkout
2. Document why the patch exists
3. Add version constraints
4. Include verification if possible
5. Update this README if introducing new patterns
