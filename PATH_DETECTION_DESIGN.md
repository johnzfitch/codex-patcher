# Smart Path Detection Design

## Problem

Documentation uses hardcoded paths like `~/dev/codex/codex-rs` which won't work for other users. Need a solution that works for everyone without manual path specification.

## Solution Options

### Option 1: Auto-detect from Current Directory ⭐ RECOMMENDED

**Concept:** CLI detects workspace automatically based on current directory.

```rust
// Add to main.rs
fn auto_detect_workspace() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;

    // Walk up from current directory looking for Cargo.toml
    for ancestor in current.ancestors() {
        let cargo_toml = ancestor.join("Cargo.toml");
        if cargo_toml.exists() {
            // Verify it's a Codex workspace (has expected structure)
            if ancestor.join("otel").exists() && ancestor.join("core").exists() {
                return Some(ancestor.to_path_buf());
            }
        }
    }
    None
}
```

**Usage:**
```bash
# User just cd's into Codex directory
cd /wherever/they/cloned/codex/codex-rs
codex-patcher apply  # No --workspace needed!

# Or from codex-patcher directory
cd /wherever/codex-patcher/is
codex-patcher apply --workspace ../codex/codex-rs  # Relative paths work

# Still supports explicit paths
codex-patcher apply --workspace /home/alice/projects/codex/codex-rs
```

**Benefits:**
- ✅ Works from any directory
- ✅ No hardcoded paths needed
- ✅ Intuitive UX (works like cargo, git)
- ✅ Backwards compatible (--workspace still works)

### Option 2: Environment Variable

```bash
# User sets once in their shell config
export CODEX_WORKSPACE="/home/alice/codex/codex-rs"

# Then just:
codex-patcher apply
```

**Implementation:**
```rust
fn get_workspace_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }

    if let Ok(env_path) = env::var("CODEX_WORKSPACE") {
        return Ok(PathBuf::from(env_path));
    }

    if let Some(detected) = auto_detect_workspace() {
        return Ok(detected);
    }

    anyhow::bail!("Could not find Codex workspace. Use --workspace or set CODEX_WORKSPACE")
}
```

### Option 3: Config File

Create `~/.config/codex-patcher/config.toml`:

```toml
[workspace]
# Default Codex workspace path
codex_rs = "/home/alice/projects/codex/codex-rs"

# Can have multiple workspaces
codex_dev = "/home/alice/dev/codex-fork/codex-rs"
```

**Usage:**
```bash
codex-patcher apply                    # Uses default
codex-patcher apply --profile codex_dev  # Uses alternate
```

### Option 4: Git-based Detection

**Concept:** Look for git remotes pointing to OpenAI's Codex repo.

```rust
fn find_codex_via_git() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["remote", "-v"])
        .output()
        .ok()?;

    let remotes = String::from_utf8_lossy(&output.stdout);

    if remotes.contains("github.com/openai/codex") {
        let root = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()?;

        let path = String::from_utf8_lossy(&root.stdout).trim().to_string();
        return Some(PathBuf::from(path).join("codex-rs"));
    }

    None
}
```

## Recommended Implementation: Hybrid Approach

**Priority order:**
1. Explicit `--workspace` flag (highest priority)
2. `CODEX_WORKSPACE` environment variable
3. Auto-detection from current directory
4. Git remote detection
5. Error with helpful message

```rust
fn resolve_workspace(cli_workspace: Option<PathBuf>) -> Result<PathBuf> {
    // 1. Explicit flag
    if let Some(path) = cli_workspace {
        return Ok(path.canonicalize()?);
    }

    // 2. Environment variable
    if let Ok(env_path) = env::var("CODEX_WORKSPACE") {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return Ok(path.canonicalize()?);
        }
        eprintln!("Warning: CODEX_WORKSPACE set but path doesn't exist: {}", env_path);
    }

    // 3. Auto-detect from current directory
    if let Some(path) = auto_detect_workspace() {
        println!("Auto-detected workspace: {}", path.display());
        return Ok(path);
    }

    // 4. Git remote detection
    if let Some(path) = find_codex_via_git() {
        println!("Found Codex workspace via git: {}", path.display());
        return Ok(path);
    }

    // 5. Helpful error
    anyhow::bail!(
        "Could not find Codex workspace. Try one of:\n\
         1. cd into your Codex directory and run: codex-patcher apply\n\
         2. Specify explicitly: codex-patcher apply --workspace /path/to/codex/codex-rs\n\
         3. Set environment variable: export CODEX_WORKSPACE=/path/to/codex/codex-rs"
    )
}
```

## Documentation Changes

### Before (hardcoded):
```bash
cargo run -- apply --workspace ~/dev/codex/codex-rs
```

### After (works for everyone):

**Option A - From Codex directory:**
```bash
cd /your/path/to/codex/codex-rs
codex-patcher apply
```

**Option B - Set once, use anywhere:**
```bash
export CODEX_WORKSPACE="/your/path/to/codex/codex-rs"
codex-patcher apply
```

**Option C - Explicit path (always works):**
```bash
codex-patcher apply --workspace /your/path/to/codex/codex-rs
```

## Benefits of This Approach

✅ **Works for everyone** - No hardcoded paths
✅ **Intuitive** - Follows cargo/git patterns (works from within directory)
✅ **Flexible** - Multiple ways to specify, user chooses what's convenient
✅ **No breaking changes** - Existing `--workspace` flag still works
✅ **Better UX** - Less typing, smart defaults
✅ **Clear errors** - Tells user exactly what to do if detection fails

## Implementation Plan

1. Add `resolve_workspace()` function to `src/main.rs`
2. Make `--workspace` optional in clap args
3. Update all commands to use `resolve_workspace()`
4. Update documentation with new patterns
5. Add tests for detection logic
