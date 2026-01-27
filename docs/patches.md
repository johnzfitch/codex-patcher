# <img src="../.github/assets/icons/magic-wand.png" width="20" height="20" alt=""/> Patch Authoring Guide

Complete guide to writing patch definitions for Codex Patcher.

## <img src="../.github/assets/icons/folder.png" width="16" height="16" alt=""/> Patch File Structure

Patches are TOML files with this structure:

```toml
[meta]
name = "patch-name"
description = "What this patch collection does"
version_range = ">=0.88.0, <0.90.0"  # Optional
workspace_relative = true

[[patches]]
id = "unique-id"
file = "path/to/file.rs"
description = "What this patch does"

[patches.query]
type = "ast-grep"
pattern = "pattern to match"

[patches.operation]
type = "replace"
text = "replacement"
```

---

## <img src="../.github/assets/icons/layers.png" width="16" height="16" alt=""/> Metadata Section

### Required Fields

```toml
[meta]
name = "privacy-patches"           # Unique identifier
workspace_relative = true          # Paths relative to workspace root
```

### Optional Fields

```toml
[meta]
description = "Remove telemetry and tracking"
version_range = ">=0.88.0"         # Semver constraint
```

### Version Ranges

| Syntax | Meaning |
|--------|---------|
| `>=0.88.0` | 0.88.0 and later |
| `<0.90.0` | Before 0.90.0 |
| `=0.88.0` | Exactly 0.88.0 |
| `>=0.88.0, <0.90.0` | Range: 0.88.x only |
| `^0.88.0` | Compatible: 0.88.x |
| `~0.88.0` | Patch updates: 0.88.x |

---

## <img src="../.github/assets/icons/search.png" width="16" height="16" alt=""/> Query Types

### ast-grep Queries

Pattern-based matching with metavariables.

```toml
[patches.query]
type = "ast-grep"
pattern = '$EXPR.clone()'
```

#### Metavariable Syntax

| Pattern | Matches |
|---------|---------|
| `$NAME` | Single AST node |
| `$$$NAME` | Zero or more nodes |
| `$_` | Single node (no capture) |

#### Examples

```toml
# Match function calls
pattern = 'println!($$$ARGS)'

# Match method calls
pattern = '$EXPR.unwrap()'

# Match struct fields
pattern = 'pub $NAME: $TYPE'

# Match specific patterns
pattern = 'OtelExporter::Statsig'
```

### tree-sitter Queries

Lower-level structural queries.

```toml
[patches.query]
type = "tree-sitter"
pattern = 'fn main() { $$$BODY }'
```

### TOML Queries

For editing TOML files (Cargo.toml, config files).

```toml
[patches.query]
type = "toml"
section = "profile.release"
key = "opt-level"
```

See [TOML Patching](toml.md) for complete reference.

---

## <img src="../.github/assets/icons/script.png" width="16" height="16" alt=""/> Operation Types

### Replace

Replace matched code with new text.

```toml
[patches.operation]
type = "replace"
text = 'OtelExporter::None'
```

#### Using Captures

```toml
# Pattern captures $EXPR
[patches.query]
type = "ast-grep"
pattern = '$EXPR.clone()'

# Replacement uses the capture
[patches.operation]
type = "replace"
text = '$EXPR.to_owned()'
```

### Delete

Remove matched code entirely.

```toml
[patches.operation]
type = "delete"
```

#### With Comment Marker

```toml
[patches.operation]
type = "delete"
insert_comment = "// Removed by codex-patcher: telemetry disabled"
```

### Insert Section (TOML)

Insert a new TOML section.

```toml
[patches.operation]
type = "insert_section"
text = '''
[profile.zack]
inherits = "release"
lto = true
'''

[patches.operation.positioning]
after_section = "profile.release"
```

### Append Section (TOML)

Append content to existing section.

```toml
[patches.operation]
type = "append_section"
text = '''
opt-level = 3
lto = true
'''
```

### Replace Value (TOML)

Replace a single TOML value.

```toml
[patches.operation]
type = "replace_value"
value = "3"
```

---

## <img src="../.github/assets/icons/tick.png" width="16" height="16" alt=""/> Verification

### Exact Match

Verify expected text before applying.

```toml
[patches.verify]
type = "exact_match"
expected_text = 'OtelExporter::Statsig'
```

### Hash

Verify using xxh3 hash (for large spans).

```toml
[patches.verify]
type = "hash"
algorithm = "xxh3"
expected = "0x1234567890abcdef"
```

---

## <img src="../.github/assets/icons/star.png" width="16" height="16" alt=""/> Complete Examples

### Example 1: Disable Telemetry

```toml
[meta]
name = "privacy-patches"
description = "Disable Statsig telemetry"
version_range = ">=0.88.0"
workspace_relative = true

[[patches]]
id = "disable-statsig-exporter"
file = "otel/src/config.rs"
description = "Replace Statsig exporter with None"

[patches.query]
type = "ast-grep"
pattern = 'OtelExporter::Statsig'

[patches.operation]
type = "replace"
text = 'OtelExporter::None'

[[patches]]
id = "remove-api-key"
file = "otel/src/config.rs"
description = "Remove Statsig API key"

[patches.query]
type = "ast-grep"
pattern = 'pub(crate) const STATSIG_API_KEY: &str = $VALUE;'

[patches.operation]
type = "delete"
insert_comment = "// API key removed by codex-patcher"
```

### Example 2: Build Optimization

```toml
[meta]
name = "performance-patches"
description = "Optimize release builds"
workspace_relative = true

[[patches]]
id = "add-release-profile"
file = "Cargo.toml"
description = "Add optimized release profile"

[patches.query]
type = "toml"
section = "profile.zack"

[patches.operation]
type = "insert_section"
text = '''
[profile.zack]
inherits = "release"
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
'''

[patches.operation.positioning]
after_section = "profile.release"

[[patches]]
id = "enable-lto"
file = "Cargo.toml"
description = "Enable LTO in release"

[patches.query]
type = "toml"
section = "profile.release"
key = "lto"

[patches.operation]
type = "replace_value"
value = '"fat"'
```

### Example 3: Refactoring

```toml
[meta]
name = "refactor-patches"
description = "Code cleanup and modernization"
workspace_relative = true

[[patches]]
id = "replace-unwrap-with-expect"
file = "src/lib.rs"
description = "Replace .unwrap() with .expect()"

[patches.query]
type = "ast-grep"
pattern = '$EXPR.unwrap()'

[patches.operation]
type = "replace"
text = '$EXPR.expect("unexpected None")'

[[patches]]
id = "replace-clone-with-to-owned"
file = "src/lib.rs"
description = "Replace .clone() on strings with .to_owned()"

[patches.query]
type = "ast-grep"
pattern = '$STRING.clone()'

[patches.operation]
type = "replace"
text = '$STRING.to_owned()'
```

---

## <img src="../.github/assets/icons/warning-16x16.png" width="16" height="16" alt=""/> Best Practices

### 1. Use Unique IDs

```toml
# Good: descriptive and unique
id = "privacy.disable-statsig-exporter"

# Bad: too generic
id = "fix1"
```

### 2. Include Descriptions

```toml
description = "Replace Statsig telemetry exporter with None to disable phone-home"
```

### 3. Be Specific with Patterns

```toml
# Good: specific enough to match uniquely
pattern = 'pub(crate) const STATSIG_API_KEY: &str = $VALUE;'

# Bad: too generic, may match multiple
pattern = 'const $NAME = $VALUE;'
```

### 4. Test with Dry Run

```bash
codex-patcher apply --dry-run --diff
```

### 5. Use Version Constraints

```toml
# Only apply to compatible versions
version_range = ">=0.88.0, <0.90.0"
```

### 6. Document Your Patches

Include a header comment explaining the patch collection:

```toml
# Privacy Patches for Codex
#
# These patches remove telemetry and tracking from the Codex CLI.
# They are designed to be idempotent and version-aware.
#
# Usage:
#   codex-patcher apply --workspace /path/to/codex-rs
#
# Compatibility: Codex 0.88.0+

[meta]
name = "privacy-patches"
...
```

---

## <img src="../.github/assets/icons/error.png" width="16" height="16" alt=""/> Troubleshooting

### No Match Found

```
Error: Query matched no locations
```

**Solutions:**
1. Check file path is correct
2. Verify pattern matches current code
3. Use `--dry-run` to debug

### Ambiguous Match

```
Error: Ambiguous query match (3 matches, expected 1)
```

**Solutions:**
1. Make pattern more specific
2. Add surrounding context
3. Use unique identifiers in pattern

### Version Mismatch

```
Skipped: version mismatch
```

**Solutions:**
1. Check `version_range` constraint
2. Update patch for new version
3. Remove constraint if not needed

---

## <img src="../.github/assets/icons/book.png" width="16" height="16" alt=""/> See Also

- [API Reference](api.md)
- [TOML Patching](toml.md)
- [Architecture](architecture.md)
