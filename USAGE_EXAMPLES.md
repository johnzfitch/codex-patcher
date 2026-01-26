# Usage Examples - Works For Everyone!

## Smart Workspace Detection

Codex Patcher automatically detects your Codex workspace using multiple strategies. **No hardcoded paths needed!**

### Method 1: Run from Codex Directory (Recommended)

The easiest way - just cd into your Codex checkout:

```bash
# Navigate to wherever YOU cloned Codex
cd /your/path/to/codex/codex-rs

# Apply patches - workspace auto-detected!
codex-patcher apply
```

**How it works:** Walks up from current directory looking for `Cargo.toml` with `otel/` and `core/` subdirectories.

### Method 2: Set Environment Variable (Set Once, Use Anywhere)

Set this once in your shell config (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
export CODEX_WORKSPACE="/your/path/to/codex/codex-rs"
```

Then from anywhere:

```bash
codex-patcher apply
codex-patcher status
codex-patcher verify
```

### Method 3: Git Remote Detection

If you're in a git checkout of OpenAI's Codex repo:

```bash
# From anywhere in the git repo
cd /your/codex/clone/some/subdirectory
codex-patcher apply  # Finds workspace via git remote
```

**How it works:** Checks git remotes for `github.com/openai/codex`, then finds workspace root.

### Method 4: Explicit Path (Always Works)

The traditional way still works:

```bash
codex-patcher apply --workspace /your/path/to/codex/codex-rs
```

## Priority Order

When you run a command, workspace is resolved in this order:

1. ✅ `--workspace` flag (if provided)
2. ✅ `$CODEX_WORKSPACE` environment variable (if set)
3. ✅ Auto-detect from current directory
4. ✅ Git remote detection
5. ❌ Error with helpful message if none work

## Example Workflows

### First Time Setup

```bash
# Clone Codex (wherever you want)
git clone https://github.com/openai/codex /my/projects/codex

# Set environment variable (optional but convenient)
echo 'export CODEX_WORKSPACE="/my/projects/codex/codex-rs"' >> ~/.bashrc
source ~/.bashrc

# Apply patches from anywhere
codex-patcher apply
```

### Daily Usage

```bash
# Option A: Work from Codex directory
cd /my/projects/codex/codex-rs
codex-patcher apply
cargo build --release

# Option B: Work from anywhere (if CODEX_WORKSPACE is set)
codex-patcher status
codex-patcher verify
```

### Handling Multiple Codex Checkouts

If you have multiple Codex versions:

```bash
# Checkout 1: Main development
export CODEX_WORKSPACE="/home/alice/codex-main/codex-rs"
codex-patcher apply

# Checkout 2: Testing
codex-patcher apply --workspace /home/alice/codex-test/codex-rs

# Checkout 3: From within directory
cd /home/alice/codex-fork/codex-rs
codex-patcher apply  # Auto-detected
```

## Testing Without Installing

During development:

```bash
cd /your/codex-patcher/clone
cargo build

# Test on your Codex checkout
cd /your/codex/checkout/codex-rs
/your/codex-patcher/clone/target/debug/codex-patcher apply
```

## Troubleshooting

### Error: "Could not find Codex workspace"

**Cause:** Workspace wasn't auto-detected and no explicit path provided.

**Solutions:**
```bash
# Solution 1: cd into your Codex directory
cd /path/to/your/codex/codex-rs
codex-patcher apply

# Solution 2: Set environment variable
export CODEX_WORKSPACE="/path/to/your/codex/codex-rs"
codex-patcher apply

# Solution 3: Provide explicit path
codex-patcher apply --workspace /path/to/your/codex/codex-rs
```

### Auto-detection Not Working?

Auto-detection requires:
- Current directory is inside Codex workspace (or subdirectory)
- Workspace has `Cargo.toml` at root
- Workspace has both `otel/` and `core/` subdirectories

**Check:**
```bash
# Are you in the right place?
pwd
ls -la | grep -E "Cargo.toml|otel|core"

# Should show:
# Cargo.toml
# otel/
# core/
```

### Git Detection Not Working?

Git detection requires:
- You're in a git repository
- One of the remotes points to `github.com/openai/codex`
- Repository has `codex-rs/` subdirectory

**Check:**
```bash
git remote -v
# Should show: github.com/openai/codex (or github.com:openai/codex)

ls -la | grep codex-rs
# Should show: codex-rs/
```

## Tips & Tricks

### Create an Alias

```bash
# Add to ~/.bashrc or ~/.zshrc
alias patch-codex="codex-patcher apply"
alias check-patches="codex-patcher status"
alias verify-patches="codex-patcher verify"

# Then just:
patch-codex
check-patches
```

### Use Relative Paths

```bash
# If codex-patcher and codex are in the same directory:
codex-patcher apply --workspace ../codex/codex-rs
```

### One-Line Patch and Build

```bash
cd /your/codex/codex-rs
codex-patcher apply && cargo build --release
```

## Documentation Examples

Throughout our documentation, you'll see examples like:

```bash
# This pattern:
cd /your/codex/checkout/codex-rs
codex-patcher apply

# Means: Replace "/your/codex/checkout" with YOUR actual path
# Examples:
cd /home/alice/projects/codex/codex-rs      # Linux
cd /Users/bob/dev/codex/codex-rs            # macOS
cd C:/Users/Carol/codex/codex-rs            # Windows
```

**The key insight:** You don't need to use our specific paths. The tool adapts to YOUR setup!
