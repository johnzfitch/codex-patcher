# <img src="../.github/assets/icons/rocket.png" width="20" height="20" alt=""/> Getting Started

This guide will help you install and start using Codex Patcher.

## <img src="../.github/assets/icons/toolbox.png" width="16" height="16" alt=""/> Installation

### From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/johnzfitch/codex-patcher
cd codex-patcher

# Build and install
cargo install --path .

# Verify installation
codex-patcher --version
```

### From crates.io

```bash
cargo install codex-patcher
```

### Requirements

- Rust 1.70.0 or later
- Git (for workspace detection)

---

## <img src="../.github/assets/icons/lightning.png" width="16" height="16" alt=""/> Quick Start

### 1. Navigate to Your Workspace

```bash
cd /path/to/codex/codex-rs
```

### 2. Create a Patch Directory

```bash
mkdir -p patches
```

### 3. Create Your First Patch

Create `patches/example.toml`:

```toml
[meta]
name = "example-patch"
description = "Example patch for demonstration"
workspace_relative = true

[[patches]]
id = "replace-hello"
file = "src/main.rs"
description = "Replace hello with HELLO"

[patches.query]
type = "ast-grep"
pattern = 'println!("hello")'

[patches.operation]
type = "replace"
text = 'println!("HELLO")'
```

### 4. Preview Changes

```bash
codex-patcher apply --dry-run --diff
```

### 5. Apply the Patch

```bash
codex-patcher apply
```

---

## <img src="../.github/assets/icons/console.png" width="16" height="16" alt=""/> CLI Overview

### Commands

| Command | Description |
|---------|-------------|
| `apply` | Apply patches to workspace |
| `status` | Show which patches are applied |
| `verify` | Verify patches match expected state |
| `list` | List available patches |

### Common Options

```bash
# Specify workspace explicitly
codex-patcher apply --workspace /path/to/workspace

# Apply specific patch file
codex-patcher apply --patches patches/privacy.toml

# Dry run (preview without applying)
codex-patcher apply --dry-run

# Show diff of changes
codex-patcher apply --diff

# Combine options
codex-patcher apply --dry-run --diff
```

---

## <img src="../.github/assets/icons/folder.png" width="16" height="16" alt=""/> Workspace Detection

Codex Patcher automatically detects your workspace using:

1. **`--workspace` flag** (highest priority)
2. **`CODEX_WORKSPACE` environment variable**
3. **Auto-detection** from current directory
4. **Git remote detection** for known repositories

### Environment Variable

```bash
export CODEX_WORKSPACE=/path/to/codex/codex-rs
codex-patcher apply  # Uses CODEX_WORKSPACE
```

---

## <img src="../.github/assets/icons/layers.png" width="16" height="16" alt=""/> Patch File Structure

Patches are defined in TOML files:

```toml
[meta]
name = "patch-name"
description = "What this patch does"
version_range = ">=0.88.0"  # Optional: only apply to matching versions
workspace_relative = true   # Paths relative to workspace root

[[patches]]
id = "unique-patch-id"
file = "path/to/file.rs"
description = "What this specific patch does"

[patches.query]
type = "ast-grep"  # or "tree-sitter", "toml"
pattern = "pattern to match"

[patches.operation]
type = "replace"  # or "delete", "insert"
text = "replacement text"
```

---

## <img src="../.github/assets/icons/tick.png" width="16" height="16" alt=""/> Verifying Patches

After applying patches, verify they're correct:

```bash
# Check status
codex-patcher status

# Verify all patches are applied
codex-patcher verify
```

### Understanding Status Output

```
Patch Status Report
Workspace: /path/to/codex-rs
Version: 0.88.0

APPLIED (3 patches)
  - privacy.disable-statsig
  - privacy.remove-api-key
  - performance.release-profile

NOT APPLIED (1 patches)
  - experimental.new-feature (target not found)

SKIPPED (1 patches)
  - legacy.old-fix (version mismatch: requires <0.85.0)
```

---

## <img src="../.github/assets/icons/warning-16x16.png" width="16" height="16" alt=""/> Troubleshooting

### Patch Not Found

```
Error: Query matched no locations in src/config.rs
```

**Cause:** The target code doesn't exist or has changed.

**Solution:**
1. Check if the file exists
2. Verify the pattern matches current code
3. Update the pattern if code has changed

### Ambiguous Match

```
Error: Ambiguous query match in src/lib.rs (3 matches, expected 1)
```

**Cause:** The pattern matches multiple locations.

**Solution:**
1. Make the pattern more specific
2. Add context to narrow down the match

### Version Mismatch

```
Skipped: version mismatch (requires >=0.90.0, workspace is 0.88.0)
```

**Cause:** Patch has a version constraint that doesn't match.

**Solution:**
1. Update to a matching version
2. Modify the patch's `version_range`

---

## <img src="../.github/assets/icons/book.png" width="16" height="16" alt=""/> Next Steps

- [API Reference](api.md) - Library usage
- [Patch Authoring](patches.md) - Writing patch definitions
- [Architecture](architecture.md) - Understanding the system
- [TOML Patching](toml.md) - TOML-specific operations
