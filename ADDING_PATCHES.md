# How to Add New Patches & Customizations

This guide explains how to create your own patches for Codex or any Rust project.

## Quick Start

1. **Create a new TOML file** in `patches/` directory
2. **Write patch definitions** using query + operation patterns
3. **Test on a checkout** with `--dry-run --diff`
4. **Apply** and verify results

## Basic Patch Structure

```toml
[meta]
name = "my-custom-patches"
description = "What these patches do"
version_range = ">=0.88.0"    # Optional: only apply to specific versions
workspace_relative = true      # Paths relative to workspace root

[[patches]]
id = "unique-patch-id"         # Unique identifier for this patch
file = "path/to/file.rs"       # File to patch (relative to workspace)

[patches.query]
type = "ast-grep"              # How to find the code
pattern = "..."                # What to match

[patches.operation]
type = "replace"               # What to do
text = "..."                   # New code
```

## Finding Code to Patch

### Step 1: Locate the Code

```bash
# Search for the code you want to patch
cd /your/codex/checkout
rg "function_name" --type rust
rg "struct SomeName" --type rust
```

### Step 2: Identify the Pattern

Look at the code structure:

```rust
// Example: You want to change this
pub fn problematic_function() -> Result<String> {
    let value = hardcoded_call();
    Ok(value)
}
```

### Step 3: Write the Query Pattern

Use `$VAR` for single nodes, `$$$VAR` for multiple:

```toml
[patches.query]
type = "ast-grep"
pattern = '''
pub fn problematic_function() -> $RETURN {
    $$$BODY
}
'''
```

## Common Patch Patterns

### 1. Replace Entire Function

**Use Case:** Completely rewrite a function

```toml
[[patches]]
id = "replace-function"
file = "src/module.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn old_function($$$PARAMS) -> $RETURN {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn old_function(param: Type) -> Result<String> {
    // Your new implementation
    Ok("fixed".to_string())
}
'''
```

### 2. Remove Hardcoded Constants

**Use Case:** Delete API keys, URLs, etc.

```toml
[[patches]]
id = "remove-api-key"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
const API_KEY: $TYPE = $VALUE;
'''

[patches.operation]
type = "delete"
insert_comment = "// REMOVED: API key (was hardcoded)"
```

### 3. Change Default Values

**Use Case:** Modify struct defaults

```toml
[[patches]]
id = "change-default"
file = "src/types.rs"

[patches.query]
type = "ast-grep"
pattern = '''
impl Default for Config {
    fn default() -> Self {
        $$$BODY
    }
}
'''

[patches.operation]
type = "replace"
text = '''
impl Default for Config {
    fn default() -> Self {
        Config {
            timeout: 30,        // Changed from 10
            retries: 5,         // Changed from 3
            enabled: false,     // Changed from true
        }
    }
}
'''
```

### 4. Modify Match Arms

**Use Case:** Change behavior in match expressions

```toml
[[patches]]
id = "modify-match-arm"
file = "src/handler.rs"

[patches.query]
type = "ast-grep"
pattern = '''
match event {
    Event::Special => {
        $$$OLD_BEHAVIOR
    }
    $$$REST
}
'''

[patches.operation]
type = "replace"
text = '''
match event {
    Event::Special => {
        // New behavior
        log::info!("Special event handled");
        return Ok(());
    }
    _ => event.clone(),
}
'''
```

### 5. Add TOML Sections (Manual for now)

**Use Case:** Add custom Cargo profiles

```toml
[[patches]]
id = "add-profile"
file = "Cargo.toml"

[patches.query]
type = "toml"
section = "profile.custom"

[patches.operation]
type = "insert-section"
text = '''
[profile.custom]
inherits = "release"
opt-level = 3
lto = "fat"
'''
after_section = "profile.release"
```

**Note:** TOML operations currently require manual application:
```bash
tail -n +35 patches/your-profile.toml >> Cargo.toml
```

## Writing Your Own Patches

### Example: Disable Feature Flag

Let's say Codex has a feature you want to disable:

```rust
// Current code in src/features.rs
pub const TELEMETRY_ENABLED: bool = true;
```

**Create `patches/my-customizations.toml`:**

```toml
[meta]
name = "my-customizations"
description = "Personal customizations for Codex"
version_range = ">=0.88.0"
workspace_relative = true

[[patches]]
id = "disable-telemetry-flag"
file = "features.rs"

[patches.query]
type = "ast-grep"
pattern = "pub const TELEMETRY_ENABLED: bool = $VALUE;"

[patches.operation]
type = "replace"
text = "pub const TELEMETRY_ENABLED: bool = false;"

[patches.verify]
method = "exact_match"
expected_text = "pub const TELEMETRY_ENABLED: bool = true;"
```

### Example: Change Timeout Value

```rust
// Current code in src/http.rs
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
```

**Patch:**

```toml
[[patches]]
id = "increase-timeout"
file = "http.rs"

[patches.query]
type = "ast-grep"
pattern = "const REQUEST_TIMEOUT: Duration = $VALUE;"

[patches.operation]
type = "replace"
text = "const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);"
```

### Example: Remove Unwanted Dependency Call

```rust
// Current code
pub fn process() -> Result<()> {
    analytics::track("event");
    do_work()?;
    Ok(())
}
```

**Patch:**

```toml
[[patches]]
id = "remove-analytics"
file = "processor.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn process() -> Result<()> {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn process() -> Result<()> {
    // analytics::track("event"); // REMOVED
    do_work()?;
    Ok(())
}
'''
```

## Testing Your Patches

### Step 1: Dry Run with Diff

```bash
codex-patcher apply \
  --workspace /your/codex/checkout \
  --patches patches/my-customizations.toml \
  --dry-run --diff
```

This shows you:
- Which patches would apply
- Exact changes (unified diff format)
- Any errors

### Step 2: Apply and Verify

```bash
# Apply patches
codex-patcher apply \
  --workspace /your/codex/checkout \
  --patches patches/my-customizations.toml

# Check if it worked
cd /your/codex/checkout
cargo check --workspace
```

### Step 3: Verify Changes

```bash
# Search for your changes
rg "TELEMETRY_ENABLED.*false"

# Check the modified file
cat src/features.rs | grep -A2 TELEMETRY_ENABLED
```

### Step 4: Test Idempotency

```bash
# Run again - should say "already applied"
codex-patcher apply \
  --workspace /your/codex/checkout \
  --patches patches/my-customizations.toml
```

Expected output:
```
⊙ disable-telemetry-flag: Already applied
```

## Advanced Patterns

### Using Metavariables

Capture and reuse parts of the code:

```toml
[[patches]]
id = "add-logging"
file = "handler.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn handle($PARAM: $TYPE) -> $RETURN {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn handle(request: Request) -> Result<Response> {
    log::debug!("Handling request: {:?}", request);
    // Original implementation would go here
    // (You need to manually write it)
}
'''
```

### Version-Specific Patches

Only apply to certain versions:

```toml
[meta]
version_range = ">=0.88.0, <0.89.0"  # Only 0.88.x

[[patches]]
id = "fix-specific-to-088"
# ...
```

### Multiple Files, One Patch Set

```toml
[meta]
name = "privacy-suite"

[[patches]]
id = "remove-analytics-config"
file = "config.rs"
# ...

[[patches]]
id = "remove-analytics-impl"
file = "analytics.rs"
# ...

[[patches]]
id = "remove-analytics-call"
file = "main.rs"
# ...
```

## Organizing Your Patches

### Recommended Structure

```
patches/
├── privacy.toml           # Remove telemetry, tracking, etc.
├── performance.toml       # Optimization, profiles
├── ui.toml               # UI tweaks, themes
├── security.toml         # Security hardening
└── experimental.toml     # Testing new ideas
```

### Naming Conventions

**Patch IDs:** Use kebab-case, be descriptive
- ✅ `disable-statsig-resolver`
- ✅ `increase-http-timeout`
- ✅ `remove-analytics-tracking`
- ❌ `patch1`, `fix`, `test`

**File Names:** Match the purpose
- ✅ `privacy.toml`, `performance.toml`
- ❌ `my-patches.toml`, `stuff.toml`

## Troubleshooting

### "Query matched no locations"

**Cause:** Pattern doesn't match the actual code

**Fix:**
1. Check the exact file path
2. Verify the pattern matches exactly
3. Use `rg` to find the actual code:
   ```bash
   rg -A5 "function_name" codex-rs/
   ```
4. Copy the exact structure into your pattern

### "Query matched 2 locations (expected 1)"

**Cause:** Pattern is too generic

**Fix:** Make the pattern more specific:

```toml
# Too generic
pattern = "fn process() { $$$BODY }"

# More specific - include return type
pattern = "fn process() -> Result<()> { $$$BODY }"

# Even more specific - include visibility
pattern = "pub(crate) fn process() -> Result<()> { $$$BODY }"
```

### "Patch already applied" but code looks wrong

**Cause:** Idempotency check passed, but replacement wasn't perfect

**Fix:** Delete the patch result and retry:
```bash
# Revert the file
git checkout -- src/problematic_file.rs

# Fix your patch definition

# Retry
codex-patcher apply --patches patches/fixed.toml
```

### Build fails after patching

**Cause:** Syntax error or logic error in replacement code

**Fix:**
1. Check cargo errors:
   ```bash
   cargo check --workspace
   ```
2. Revert patches:
   ```bash
   git checkout -- .
   ```
3. Fix your patch's `text` field
4. Test in isolation:
   ```bash
   # Copy the replacement code
   # Test it compiles before patching
   ```

## Tips & Best Practices

### 1. Start Small
- Patch one thing at a time
- Test each patch individually
- Combine into suites once working

### 2. Use `--dry-run --diff`
Always preview changes before applying:
```bash
codex-patcher apply --dry-run --diff --patches patches/new.toml
```

### 3. Keep Patches Simple
- One logical change per patch
- Don't try to do too much in one replacement
- Multiple small patches > one giant patch

### 4. Document Intent
Use comments in your patches:
```toml
[[patches]]
id = "disable-feature-x"
# Disables Feature X because it conflicts with our internal tools
# See: https://github.com/company/internal-docs/issue/123
file = "features.rs"
```

### 5. Version Control Your Patches
```bash
cd codex-patcher
git add patches/my-customizations.toml
git commit -m "Add custom patches for internal deployment"
```

### 6. Test on Clean Checkout
Before relying on patches:
```bash
# Clone fresh Codex
git clone https://github.com/openai/codex /tmp/test-codex
cd /tmp/test-codex
git checkout rust-v0.88.0-alpha.4

# Apply your patches
codex-patcher apply --workspace /tmp/test-codex/codex-rs

# Build and test
cd /tmp/test-codex/codex-rs
cargo build --release
```

## Real-World Examples

See the included patch files for complete examples:
- `patches/privacy.toml` - Removing telemetry (5 patches)
- `patches/zack-profile.toml` - Custom build profile

## Getting Help

### Debugging Patterns

1. **See what AST-grep matches:**
   ```bash
   # Install ast-grep
   cargo install ast-grep

   # Test your pattern
   ast-grep --pattern 'pub fn $NAME() { $$$BODY }' src/
   ```

2. **Understand tree-sitter structure:**
   ```bash
   # View syntax tree
   tree-sitter parse src/file.rs
   ```

3. **Check patch status:**
   ```bash
   codex-patcher status --workspace /your/codex
   ```

### Common Questions

**Q: Can I patch any Rust project?**
A: Yes! Just point `--workspace` at any Rust project root.

**Q: Do patches persist across Codex updates?**
A: No. Re-apply patches after pulling upstream changes:
```bash
cd /your/codex
git pull origin main
cd /your/codex-patcher
codex-patcher apply --workspace /your/codex/codex-rs
```

**Q: Can I share patches with my team?**
A: Yes! Commit patch files to your team's repo:
```bash
git clone https://github.com/yourcompany/codex-patches
cd codex-patches
# Add patches/
codex-patcher apply --workspace ~/codex/codex-rs
```

**Q: What if upstream code changes?**
A: Patches may fail. Update the pattern to match new code structure, or remove obsolete patches.

## Next Steps

1. **Try the examples** - Test with provided privacy patches
2. **Create your first patch** - Start with a simple constant change
3. **Build a patch suite** - Combine related patches
4. **Automate** - Add to your build scripts or CI

## Reference

- **Full Syntax Guide:** See `patches/README.md`
- **Query Types:** AST-grep, Tree-sitter, TOML
- **Operation Types:** Replace, Delete, Insert, Append
- **Verification:** Exact match, Hash-based
