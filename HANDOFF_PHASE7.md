# Codex Patcher - Phase 7 Agent Handoff

## Project Location

`/home/zack/dev/codex-patcher/`

## What It Is

Automated code patching system for Rust. Applies privacy/performance patches to OpenAI's Codex CLI after upstream releases. Uses byte-span replacement as the primitive, with tree-sitter + ast-grep for structural queries.

## Current Status: Phase 6 Complete ✅

Phases 1-6 are complete and fully tested:

1. ✅ **Edit primitive** - `src/edit.rs` - byte-span replacement with verification, atomic writes, batch edits
2. ✅ **TOML patching** - `src/toml/` - structure-preserving Cargo.toml edits via toml_edit
3. ✅ **Tree-sitter** - `src/ts/` - parsing, queries, structural locator
4. ✅ **ast-grep** - `src/sg/` - pattern matching with ast-grep-language::SupportLang::Rust
5. ✅ **Validation** - `src/validate.rs` - parse validation, syn validation, selector uniqueness
6. ✅ **Patch Config Parser** - `src/config/` - TOML schema, version filtering, idempotency, applicator

**Test Status**: 105 passing tests (88 library + 17 integration), 0 failures

## Your Task: Phase 7 - CLI & UX

Implement the command-line interface that makes patch application accessible to users.

### Deliverables (from plan.md lines 743-750)

31. Implement CLI with clap (apply, status, verify commands)
32. Add `--dry-run` and `--diff` output (using `similar` crate)
33. Implement conflict detection and reporting
34. Add progress indicators
35. Test: User-facing CLI workflows

**Deliverable**: Full CLI with good UX

## Architecture Overview

### Core Primitive (Already Implemented)

```rust
struct Edit {
    file: PathBuf,
    byte_start: usize,
    byte_end: usize,
    new_text: String,
    expected_before: EditVerification,
}
```

All operations compile to this primitive. Intelligence lives in span acquisition (tree-sitter, ast-grep, compiler diagnostics), not application.

### Key Modules You'll Use

#### Config System (`src/config/`)
```rust
use codex_patcher::config::{load_from_path, apply_patches, PatchResult};

let config = load_from_path("patches/privacy.toml")?;
let results = apply_patches(&config, workspace_root, "0.88.0");

for (patch_id, result) in results {
    match result {
        Ok(PatchResult::Applied { file }) => println!("✓ Applied"),
        Ok(PatchResult::AlreadyApplied { .. }) => println!("⊙ Already applied"),
        Ok(PatchResult::SkippedVersion { reason }) => println!("⊘ Skipped"),
        Err(e) => eprintln!("✗ Failed: {}", e),
    }
}
```

#### Edit System (`src/edit.rs`)
```rust
use codex_patcher::{Edit, EditResult};

let edit = Edit::new(path, start, end, new_text, expected_before);
match edit.apply()? {
    EditResult::Applied { file, bytes_changed } => { /* ... */ }
    EditResult::AlreadyApplied { file } => { /* ... */ }
}
```

#### Validation (`src/validate.rs`)
```rust
use codex_patcher::validate::{ParseValidator, ValidatedEdit};

let validator = ParseValidator::new()?;
validator.validate_edit(&edit)?; // Returns ValidationError on syntax errors
```

## CLI Command Specification

### `codex-patcher apply`

Apply patches from configuration files.

```bash
# Apply all patches in patches/ directory
codex-patcher apply --workspace ~/dev/codex/codex-rs

# Apply specific patch file
codex-patcher apply --workspace ~/dev/codex/codex-rs --patches patches/privacy.toml

# Dry run with diff output
codex-patcher apply --dry-run --diff --workspace ~/dev/codex/codex-rs

# Apply to specific version
codex-patcher apply --workspace ~/dev/codex/codex-rs --version 0.88.0
```

**Options**:
- `--workspace <PATH>` (required) - Root directory of the workspace
- `--patches <FILE>` (optional) - Specific patch file (default: `patches/*.toml`)
- `--dry-run` - Show what would be changed without modifying files
- `--diff` - Show unified diffs of changes
- `--version <VERSION>` - Workspace version for filtering (default: read from Cargo.toml)
- `--verbose` / `-v` - Verbose output

**Expected Output**:
```
Loading patches from patches/privacy.toml...
Workspace: /home/zack/dev/codex/codex-rs
Version: 0.88.0
Applying 4 patches...

✓ disable-statsig-resolver: Applied to codex-rs/otel/src/config.rs
⊙ remove-statsig-constants: Already applied
✓ default-metrics-none-types: Applied to codex-rs/core/src/config/types.rs
✗ patch-missing-file: Failed - file not found: nonexistent.rs

Summary: 2 applied, 1 already applied, 1 failed
```

### `codex-patcher status`

Check status of patches without applying.

```bash
codex-patcher status --workspace ~/dev/codex/codex-rs
```

**Expected Output**:
```
Patch Status Report
Workspace: /home/zack/dev/codex/codex-rs
Version: 0.88.0

✓ APPLIED (2 patches)
  - disable-statsig-resolver
  - default-metrics-none-types

⊙ NOT APPLIED (1 patch)
  - remove-statsig-constants (target not found - may be obsolete)

⊘ SKIPPED (1 patch)
  - future-patch (version constraint: >=0.90.0)
```

### `codex-patcher verify`

Verify patches are correctly applied.

```bash
codex-patcher verify --workspace ~/dev/codex/codex-rs
```

**Expected Output**:
```
Verifying patches...

✓ disable-statsig-resolver: Verified
✗ default-metrics-none-types: MISMATCH
  Expected: OtelExporterKind::None
  Found:    OtelExporterKind::Statsig
  Location: codex-rs/core/src/config/types.rs:403

Summary: 1 verified, 1 mismatch
```

## Diff Output Format

When `--diff` is specified, show unified diffs using the `similar` crate:

```diff
--- codex-rs/otel/src/config.rs (original)
+++ codex-rs/otel/src/config.rs (patched)
@@ -45,15 +45,7 @@
 pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
     match exporter {
-        OtelExporter::Statsig => {
-            if cfg!(test) || cfg!(feature = "disable-default-metrics-exporter") {
-                return OtelExporter::None;
-            }
-
-            OtelExporter::OtlpHttp {
-                endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
-                headers: HashMap::from([...]),
-            }
-        }
+        OtelExporter::Statsig => OtelExporter::None,
         _ => exporter.clone(),
     }
 }
```

## Implementation Guide

### Step 1: CLI Structure with Clap

Create the CLI framework in `src/main.rs`:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codex-patcher")]
#[command(about = "Automated code patching system for Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply patches to a workspace
    Apply {
        /// Workspace root directory
        #[arg(short, long)]
        workspace: PathBuf,

        /// Specific patch file (default: patches/*.toml)
        #[arg(short, long)]
        patches: Option<PathBuf>,

        /// Show what would change without applying
        #[arg(long)]
        dry_run: bool,

        /// Show unified diffs
        #[arg(long)]
        diff: bool,

        /// Workspace version for filtering
        #[arg(long)]
        version: Option<String>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Check status of patches
    Status {
        #[arg(short, long)]
        workspace: PathBuf,
    },

    /// Verify patches are correctly applied
    Verify {
        #[arg(short, long)]
        workspace: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply { workspace, patches, dry_run, diff, version, verbose } => {
            cmd_apply(workspace, patches, dry_run, diff, version, verbose)?;
        }
        Commands::Status { workspace } => {
            cmd_status(workspace)?;
        }
        Commands::Verify { workspace } => {
            cmd_verify(workspace)?;
        }
    }

    Ok(())
}
```

### Step 2: Implement `apply` Command

```rust
fn cmd_apply(
    workspace: PathBuf,
    patches: Option<PathBuf>,
    dry_run: bool,
    show_diff: bool,
    version: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use codex_patcher::config::{load_from_path, apply_patches};

    // 1. Determine patch files to load
    let patch_files = if let Some(path) = patches {
        vec![path]
    } else {
        // Find all .toml files in patches/ directory
        discover_patch_files(&workspace)?
    };

    // 2. Determine workspace version
    let workspace_version = version.or_else(|| {
        read_workspace_version(&workspace).ok()
    }).unwrap_or_else(|| "0.0.0".to_string());

    println!("Workspace: {}", workspace.display());
    println!("Version: {}", workspace_version);

    // 3. Load and apply each patch file
    let mut total_applied = 0;
    let mut total_already_applied = 0;
    let mut total_failed = 0;

    for patch_file in patch_files {
        if verbose {
            println!("\nLoading patches from {}...", patch_file.display());
        }

        let config = load_from_path(&patch_file)?;

        if verbose {
            println!("Applying {} patches...", config.patches.len());
        }

        let results = if dry_run {
            // TODO: Implement dry-run mode that doesn't modify files
            apply_patches_dry_run(&config, &workspace, &workspace_version)?
        } else {
            apply_patches(&config, &workspace, &workspace_version)
        };

        // 4. Report results
        for (patch_id, result) in results {
            match result {
                Ok(PatchResult::Applied { file }) => {
                    println!("✓ {}: Applied to {}", patch_id, file.display());
                    total_applied += 1;

                    if show_diff {
                        // TODO: Show diff
                    }
                }
                Ok(PatchResult::AlreadyApplied { .. }) => {
                    println!("⊙ {}: Already applied", patch_id);
                    total_already_applied += 1;
                }
                Ok(PatchResult::SkippedVersion { reason }) => {
                    println!("⊘ {}: Skipped ({})", patch_id, reason);
                }
                Err(e) => {
                    eprintln!("✗ {}: Failed - {}", patch_id, e);
                    total_failed += 1;
                }
            }
        }
    }

    // 5. Summary
    println!("\nSummary: {} applied, {} already applied, {} failed",
        total_applied, total_already_applied, total_failed);

    if total_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
```

### Step 3: Helper Functions

```rust
fn discover_patch_files(workspace: &Path) -> anyhow::Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    let patches_dir = workspace.join("patches");
    if !patches_dir.exists() {
        return Err(anyhow::anyhow!("patches/ directory not found"));
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(patches_dir).max_depth(1) {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
            files.push(entry.path().to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

fn read_workspace_version(workspace: &Path) -> anyhow::Result<String> {
    use cargo_metadata::MetadataCommand;

    let metadata = MetadataCommand::new()
        .manifest_path(workspace.join("Cargo.toml"))
        .exec()?;

    if let Some(pkg) = metadata.workspace_packages().first() {
        Ok(pkg.version.to_string())
    } else {
        Err(anyhow::anyhow!("No workspace package found"))
    }
}
```

### Step 4: Diff Output (using `similar` crate)

```rust
fn show_diff(file: &Path, original: &str, modified: &str) {
    use similar::{ChangeTag, TextDiff};
    use colored::Colorize;

    println!("\n--- {} (original)", file.display());
    println!("+++ {} (patched)", file.display());

    let diff = TextDiff::from_lines(original, modified);

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-".red(),
            ChangeTag::Insert => "+".green(),
            ChangeTag::Equal => " ".normal(),
        };
        print!("{}{}", sign, change);
    }
}
```

### Step 5: Status Command

```rust
fn cmd_status(workspace: PathBuf) -> anyhow::Result<()> {
    // Load patches, check status without applying
    // Group by: Applied, Not Applied, Skipped
    unimplemented!("status command")
}
```

### Step 6: Verify Command

```rust
fn cmd_verify(workspace: PathBuf) -> anyhow::Result<()> {
    // Load patches, verify expected_before matches current state
    unimplemented!("verify command")
}
```

## Conflict Detection and Reporting

When a patch cannot be applied, provide detailed diagnostics:

```rust
use codex_patcher::config::ApplicationError;

match error {
    ApplicationError::NoMatch { file } => {
        eprintln!("CONFLICT: Query matched no locations");
        eprintln!("  File: {}", file.display());
        eprintln!("  Possible causes:");
        eprintln!("  - Function/struct was renamed or removed");
        eprintln!("  - Signature changed");
        eprintln!("  - Code was moved to different file");
    }
    ApplicationError::AmbiguousMatch { file, count } => {
        eprintln!("CONFLICT: Query matched {} locations (expected 1)", count);
        eprintln!("  File: {}", file.display());
        eprintln!("  Action: Refine the query pattern to be more specific");
    }
    ApplicationError::Edit(edit_error) => {
        // Handle edit-specific errors (verification mismatch, etc.)
    }
    _ => { /* ... */ }
}
```

## Testing Strategy

Create `tests/cli_integration.rs`:

```rust
#[test]
fn test_apply_dry_run() {
    let output = Command::new("cargo")
        .args(&["run", "--", "apply", "--workspace", "fixtures/test-workspace", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("would apply"));
}

#[test]
fn test_apply_with_diff() {
    // Test that --diff flag shows unified diffs
}

#[test]
fn test_status_command() {
    // Test status output format
}

#[test]
fn test_verify_command() {
    // Test verification logic
}
```

## Dependencies Already Available

All dependencies are already in `Cargo.toml`:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }    # CLI framework
similar = "2"                                        # Unified diff
colored = "2"                                        # Terminal colors
walkdir = "2"                                        # Directory traversal
cargo_metadata = "0.18"                             # Read Cargo.toml version
anyhow = "1"                                        # Error handling
```

## Example Workflow

Once complete, the user workflow will be:

```bash
# After merging upstream Codex release
cd ~/dev/codex
git checkout -b merge-v0.89.0
git merge rust-v0.89.0

# Apply patches
cd ~/dev/codex-patcher
cargo run -- apply --workspace ~/dev/codex/codex-rs

# Check status
cargo run -- status --workspace ~/dev/codex/codex-rs

# Verify
cargo run -- verify --workspace ~/dev/codex/codex-rs
```

## Key Files

- `src/main.rs` - CLI entry point (currently minimal)
- `src/lib.rs` - Public API (already exports everything you need)
- `src/config/` - Patch loading and application (complete)
- `src/edit.rs` - Edit primitive (complete)
- `src/validate.rs` - Validation (complete)

## Important Notes

1. **Don't reinvent the wheel** - All core functionality exists. You're building a CLI wrapper.
2. **Use the existing API** - `apply_patches()`, `load_from_path()`, etc. are ready to use.
3. **Focus on UX** - Clear output, helpful error messages, progress indicators.
4. **Dry-run mode** - May need to add a dry-run variant to the applicator.
5. **Colored output** - Use `colored` crate for terminal output.
6. **Error handling** - Use `anyhow` for CLI-level errors, preserve detailed errors from library.

## Success Criteria

- ✅ `codex-patcher apply` works with all flags
- ✅ `--dry-run` shows what would change without modifying files
- ✅ `--diff` shows unified diffs using `similar` crate
- ✅ `codex-patcher status` reports patch status
- ✅ `codex-patcher verify` validates patches
- ✅ Clear, colored output with progress indicators
- ✅ Helpful error messages and conflict detection
- ✅ Integration tests for CLI commands

## Questions?

Check these resources:
- `plan.md` - Full implementation plan
- `spec.md` - Technical specification
- `CLAUDE.md` - Architecture overview
- `PHASE6_COMPLETE.md` - What was just completed
- Existing code - Everything you need is already implemented

Run tests with: `cargo test --quiet`

Good luck! 🚀
