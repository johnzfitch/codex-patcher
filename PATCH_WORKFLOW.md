# Patch Creation Workflow

## Visual Workflow

```
┌─────────────────────────────────────────────────────────────┐
│  STEP 1: Identify Code to Change                            │
│                                                              │
│  $ cd /your/codex/checkout                                  │
│  $ rg "function_name" --type rust                           │
│  $ cat src/module.rs  # Read the code                       │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│  STEP 2: Create Patch File                                  │
│                                                              │
│  $ cd /codex-patcher                                        │
│  $ $EDITOR patches/my-patches.toml                          │
│                                                              │
│  Write:                                                      │
│    [meta]                                                    │
│    name = "my-patches"                                       │
│                                                              │
│    [[patches]]                                               │
│    id = "fix-something"                                      │
│    file = "module.rs"                                        │
│    [patches.query]                                           │
│    type = "ast-grep"                                         │
│    pattern = "..."                                           │
│    [patches.operation]                                       │
│    type = "replace"                                          │
│    text = "..."                                              │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│  STEP 3: Test with Dry Run                                  │
│                                                              │
│  $ codex-patcher apply \                                    │
│      --workspace /your/codex/codex-rs \                     │
│      --patches patches/my-patches.toml \                    │
│      --dry-run --diff                                       │
│                                                              │
│  Review the output:                                          │
│    ✓ Patch would apply                                      │
│    Shows exact diff                                          │
└──────────────────────┬───────────────────────────────────────┘
                       │
                   ┌───┴───┐
                   │ Looks │
                   │ good? │
                   └───┬───┘
                       │
            No  ◄──────┼──────► Yes
                       │              │
                       ▼              ▼
          ┌──────────────────┐  ┌──────────────────┐
          │ Fix pattern/text │  │ STEP 4: Apply    │
          │ Go back to Step 2│  │                  │
          └──────────────────┘  │ $ codex-patcher  │
                                │   apply          │
                                └────────┬─────────┘
                                         │
                                         ▼
                       ┌─────────────────────────────────────┐
                       │  STEP 5: Verify                     │
                       │                                     │
                       │  $ cd /your/codex/codex-rs         │
                       │  $ cargo check --workspace         │
                       │  $ cargo test                       │
                       │                                     │
                       │  $ rg "your_change"                │
                       └────────┬────────────────────────────┘
                                │
                            ┌───┴───┐
                            │ Works?│
                            └───┬───┘
                                │
                     No  ◄──────┼──────► Yes
                                │              │
                                ▼              ▼
                   ┌──────────────────┐  ┌──────────────────┐
                   │ Revert and fix   │  │ DONE!            │
                   │ git checkout --  │  │ Commit patches   │
                   │ Go to Step 2     │  │                  │
                   └──────────────────┘  └──────────────────┘
```

## Quick Reference Card

### Anatomy of a Patch

```toml
[meta]
name = "patch-set-name"           # What this file contains
version_range = ">=0.88.0"        # Optional: version filter
workspace_relative = true         # Paths relative to workspace

[[patches]]
id = "unique-id"                  # Identifier for this patch
file = "path/to/file.rs"          # Target file

[patches.query]                   # HOW to find the code
type = "ast-grep"                 # or "tree-sitter", "toml"
pattern = "..."                   # What to match

[patches.operation]               # WHAT to do
type = "replace"                  # or "delete", "insert-section", etc.
text = "..."                      # New code

[patches.verify]                  # Optional: extra safety
method = "exact_match"            # or "hash"
expected_text = "..."             # What you expect to find
```

### Query Type Quick Guide

| Type | Use For | Example |
|------|---------|---------|
| **ast-grep** | Rust code patterns | Functions, structs, match arms |
| **tree-sitter** | Structural queries | Complex AST navigation |
| **toml** | TOML files | Cargo.toml modifications |

### Operation Type Quick Guide

| Type | Purpose | Works On |
|------|---------|----------|
| **replace** | Change code | Rust code |
| **delete** | Remove code | Rust code |
| **replace-capture** | Modify captured part | Rust code |
| **insert-section** | Add TOML section | TOML files |
| **append-section** | Append at end | TOML files |
| **replace-value** | Change TOML value | TOML files |
| **replace-key** | Rename TOML key | TOML files |
| **delete-section** | Remove TOML section | TOML files |

### Pattern Syntax

| Symbol | Meaning | Example |
|--------|---------|---------|
| `$VAR` | Match single node | `fn $NAME()` |
| `$$$VAR` | Match multiple nodes | `fn foo($$$PARAMS)` |
| `$_` | Match any (anonymous) | `fn $_()` |

### Common Patterns

**Match any function:**
```toml
pattern = "fn $NAME($$$PARAMS) -> $RETURN { $$$BODY }"
```

**Match specific function:**
```toml
pattern = "fn specific_name($$$PARAMS) { $$$BODY }"
```

**Match struct:**
```toml
pattern = "struct $NAME { $$$FIELDS }"
```

**Match constant:**
```toml
pattern = "const $NAME: $TYPE = $VALUE;"
```

**Match impl block:**
```toml
pattern = "impl $TRAIT for $TYPE { $$$METHODS }"
```

**Match Default impl:**
```toml
pattern = '''
impl Default for $TYPE {
    fn default() -> Self {
        $$$BODY
    }
}
'''
```

## Command Cheat Sheet

### Apply Patches

```bash
# Basic apply
codex-patcher apply

# With specific patch file
codex-patcher apply --patches patches/my-patches.toml

# Explicit workspace
codex-patcher apply --workspace /path/to/codex/codex-rs

# Preview changes (no modifications)
codex-patcher apply --dry-run --diff

# Combine options
codex-patcher apply \
  --workspace /path/to/codex \
  --patches patches/custom.toml \
  --dry-run --diff
```

### Check Status

```bash
# See which patches are applied
codex-patcher status

# With explicit workspace
codex-patcher status --workspace /path/to/codex/codex-rs
```

### Verify Patches

```bash
# Check if patches are correctly applied
codex-patcher verify

# With explicit workspace
codex-patcher verify --workspace /path/to/codex/codex-rs
```

### Environment Setup

```bash
# Set default workspace (add to ~/.bashrc or ~/.zshrc)
export CODEX_WORKSPACE="/your/path/to/codex/codex-rs"

# Then just use:
codex-patcher apply
codex-patcher status
codex-patcher verify
```

## Testing Checklist

Before finalizing patches:

- [ ] Dry run shows correct changes
  ```bash
  codex-patcher apply --dry-run --diff
  ```

- [ ] Patches apply cleanly
  ```bash
  codex-patcher apply
  ```

- [ ] Code compiles
  ```bash
  cd /codex && cargo check --workspace
  ```

- [ ] Tests pass (if applicable)
  ```bash
  cargo test
  ```

- [ ] Idempotency works
  ```bash
  codex-patcher apply  # Second time should say "already applied"
  ```

- [ ] Verified on clean checkout
  ```bash
  git clone https://github.com/openai/codex /tmp/test
  codex-patcher apply --workspace /tmp/test/codex-rs
  cd /tmp/test/codex-rs && cargo check
  ```

## Example: Full Workflow

Let's create a patch to disable a feature:

### 1. Find the code

```bash
cd ~/codex/codex-rs
rg "FEATURE_ENABLED" --type rust
```

Output:
```
src/features.rs:
10:pub const FEATURE_ENABLED: bool = true;
```

### 2. Create patch file

```bash
cd ~/codex-patcher
cat > patches/disable-feature.toml << 'EOF'
[meta]
name = "disable-feature"
description = "Disable unwanted feature"
workspace_relative = true

[[patches]]
id = "disable-feature-const"
file = "features.rs"

[patches.query]
type = "ast-grep"
pattern = "pub const FEATURE_ENABLED: bool = $VALUE;"

[patches.operation]
type = "replace"
text = "pub const FEATURE_ENABLED: bool = false;"
EOF
```

### 3. Test with dry run

```bash
codex-patcher apply \
  --patches patches/disable-feature.toml \
  --dry-run --diff
```

Output:
```
✓ disable-feature-const: Would apply to features.rs

--- features.rs (original)
+++ features.rs (patched)
@@ -7,1 +7,1 @@
-pub const FEATURE_ENABLED: bool = true;
+pub const FEATURE_ENABLED: bool = false;
```

### 4. Apply

```bash
codex-patcher apply --patches patches/disable-feature.toml
```

Output:
```
✓ disable-feature-const: Applied to features.rs
```

### 5. Verify

```bash
cd ~/codex/codex-rs
cargo check --workspace
rg "FEATURE_ENABLED.*false"
```

Output:
```
✓ Build successful
features.rs:10:pub const FEATURE_ENABLED: bool = false;
```

### 6. Test idempotency

```bash
cd ~/codex-patcher
codex-patcher apply --patches patches/disable-feature.toml
```

Output:
```
⊙ disable-feature-const: Already applied
```

✅ **Done!** Patch is working correctly.

## Troubleshooting Guide

### Problem: "Query matched no locations"

**Check 1:** File path correct?
```bash
cd ~/codex/codex-rs
ls -la features.rs  # Does it exist?
```

**Check 2:** Pattern matches?
```bash
rg "pub const FEATURE_ENABLED" features.rs
# Compare exact text with your pattern
```

**Fix:** Update pattern to match exactly:
```toml
# If actual code is:
pub(crate) const FEATURE_ENABLED: bool = true;

# Your pattern needs:
pattern = "pub(crate) const FEATURE_ENABLED: bool = $VALUE;"
```

### Problem: "Query matched 2 locations"

**Check:** Multiple matches
```bash
rg "FEATURE_ENABLED" --type rust -n
```

**Fix:** Add more context to pattern:
```toml
# Too generic
pattern = "const FEATURE_ENABLED: bool = $VALUE;"

# More specific
pattern = "pub const FEATURE_ENABLED: bool = $VALUE;"

# Even more specific (include surrounding code)
pattern = '''
// Feature toggle
pub const FEATURE_ENABLED: bool = $VALUE;
'''
```

### Problem: Build fails after patching

**Check:** Syntax error
```bash
cd ~/codex/codex-rs
cargo check 2>&1 | head -20
```

**Fix:** Check your replacement text compiles:
```bash
# Copy your replacement code to a test file
cat > /tmp/test.rs << 'EOF'
pub const FEATURE_ENABLED: bool = false;
EOF

# Test syntax
rustc --crate-type lib /tmp/test.rs
```

### Problem: Patch doesn't apply on upstream update

**Check:** Has upstream code changed?
```bash
git diff origin/main -- features.rs
```

**Fix:** Update your pattern to match new structure:
1. Look at new code
2. Update pattern in patch file
3. Test with `--dry-run --diff`

## Tips for Success

1. **Start with exact patterns** - Copy code exactly, then generalize
2. **Test incrementally** - One patch at a time
3. **Use version constraints** - Protect against breaking changes
4. **Document reasoning** - Future you will thank you
5. **Version control patches** - Commit to git
6. **Share with team** - Standardize modifications

## Further Reading

- **Complete Syntax:** `patches/README.md`
- **Advanced Examples:** `ADDING_PATCHES.md`
- **Query Reference:** AST-grep documentation
- **TOML Operations:** `docs/toml.md`
