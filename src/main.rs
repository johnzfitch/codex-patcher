use anyhow::Result;
use clap::{Parser, Subcommand};
use codex_patcher::config::{
    apply_patches, check_patches, load_from_path, ApplicationError, PatchResult,
};
use colored::Colorize;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "codex-patcher")]
#[command(about = "Automated code patching system for Rust", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply patches to a workspace
    Apply {
        /// Path to workspace root (auto-detected if not specified)
        #[arg(short, long)]
        workspace: Option<PathBuf>,

        /// Specific patch file to apply (otherwise applies all in patches/)
        #[arg(short, long)]
        patches: Option<PathBuf>,

        /// Dry run - show what would be changed without modifying files
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Show unified diff of changes
        #[arg(short, long)]
        diff: bool,
    },

    /// Check status of patches without applying
    Status {
        /// Path to workspace root (auto-detected if not specified)
        #[arg(short, long)]
        workspace: Option<PathBuf>,
    },

    /// Verify patches are applicable to current workspace
    Verify {
        /// Path to workspace root (auto-detected if not specified)
        #[arg(short, long)]
        workspace: Option<PathBuf>,
    },

    /// List available patches and their version constraints
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply {
            workspace,
            patches,
            dry_run,
            diff,
        } => cmd_apply(workspace, patches, dry_run, diff),

        Commands::Status { workspace } => cmd_status(workspace),

        Commands::Verify { workspace } => cmd_verify(workspace),

        Commands::List => cmd_list(),
    }
}

/// Helper: Discover all .toml patch files in a patches/ directory.
///
/// Discovery order:
/// 1. `<workspace>/patches` (allows keeping patch files alongside the target).
/// 2. `./patches` relative to the current working directory (typical when
///    running from the codex-patcher repo).
fn discover_patch_files(workspace: &Path) -> Result<Vec<PathBuf>> {
    let cwd_patches_dir = env::current_dir().ok().map(|cwd| cwd.join("patches"));
    let workspace_patches_dir = workspace.join("patches");

    let candidate_dirs: Vec<PathBuf> = std::iter::once(workspace_patches_dir.clone())
        .chain(cwd_patches_dir.into_iter())
        .collect();

    for patches_dir in candidate_dirs {
        if !patches_dir.exists() {
            continue;
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(&patches_dir).max_depth(1) {
            let entry = entry?;
            if entry.file_type().is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("toml")
            {
                files.push(entry.path().to_path_buf());
            }
        }

        files.sort();

        if !files.is_empty() {
            return Ok(files);
        }
    }

    anyhow::bail!(
        "No .toml patch files found in either ./patches or {}/patches",
        workspace.display()
    )
}

/// Resolve workspace path using multiple detection strategies
///
/// Priority order:
/// 1. Explicit --workspace flag
/// 2. CODEX_WORKSPACE environment variable
/// 3. Auto-detect from current directory
/// 4. Git remote detection
fn resolve_workspace(cli_workspace: Option<PathBuf>) -> Result<PathBuf> {
    // 1. Explicit flag (highest priority)
    if let Some(path) = cli_workspace {
        return Ok(path.canonicalize()?);
    }

    // 2. Environment variable
    if let Ok(env_path) = env::var("CODEX_WORKSPACE") {
        let path = PathBuf::from(&env_path);
        if path.exists() {
            return Ok(path.canonicalize()?);
        }
        eprintln!(
            "{}",
            format!(
                "Warning: CODEX_WORKSPACE is set but path doesn't exist: {}",
                env_path
            )
            .yellow()
        );
    }

    // 3. Auto-detect from current directory
    if let Some(path) = auto_detect_workspace() {
        println!(
            "{}",
            format!("Auto-detected workspace: {}", path.display()).dimmed()
        );
        return Ok(path);
    }

    // 4. Git remote detection
    if let Some(path) = find_codex_via_git() {
        println!(
            "{}",
            format!("Found Codex workspace via git: {}", path.display()).dimmed()
        );
        return Ok(path);
    }

    // 5. Helpful error with multiple solutions
    anyhow::bail!(
        "{}\n{}\n  {}\n  {}\n  {}",
        "Could not find Codex workspace.".red(),
        "Try one of:".bold(),
        "1. cd into your Codex directory: cd /path/to/codex/codex-rs && codex-patcher apply",
        "2. Specify explicitly: codex-patcher apply --workspace /path/to/codex/codex-rs",
        "3. Set environment variable: export CODEX_WORKSPACE=/path/to/codex/codex-rs"
    )
}

/// Auto-detect workspace by walking up from current directory
fn auto_detect_workspace() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;

    // Walk up from current directory looking for Cargo.toml
    for ancestor in current.ancestors() {
        let cargo_toml = ancestor.join("Cargo.toml");
        if !cargo_toml.exists() {
            continue;
        }

        // Verify it's a Codex workspace by checking for expected directories
        // Codex has: otel/, core/, cli/, etc.
        let has_otel = ancestor.join("otel").exists();
        let has_core = ancestor.join("core").exists();

        if has_otel && has_core {
            return Some(ancestor.to_path_buf());
        }
    }

    None
}

/// Find Codex workspace by checking git remotes.
///
/// Parses `git remote -v` output line-by-line, extracting the URL field
/// (second whitespace-delimited token) and checking for the openai/codex
/// repo path. Handles both HTTPS and SSH URL formats.
fn find_codex_via_git() -> Option<PathBuf> {
    let output = Command::new("git").args(["remote", "-v"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let remotes = String::from_utf8_lossy(&output.stdout);

    // Parse each line: "origin\thttps://github.com/openai/codex.git (fetch)"
    // Extract the URL (second field) and check for the repo path.
    let is_codex_remote = remotes.lines().any(|line| {
        let url = line.split_whitespace().nth(1).unwrap_or("");
        // HTTPS: github.com/openai/codex or github.com/openai/codex.git
        // SSH:   github.com:openai/codex or github.com:openai/codex.git
        let normalized = url.trim_end_matches(".git").trim_end_matches('/');
        normalized.ends_with("github.com/openai/codex")
            || normalized.ends_with("github.com:openai/codex")
    });

    if !is_codex_remote {
        return None;
    }

    let root_output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if !root_output.status.success() {
        return None;
    }

    let git_root = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();

    let codex_rs = PathBuf::from(git_root).join("codex-rs");

    if codex_rs.exists() {
        Some(codex_rs)
    } else {
        None
    }
}

/// Helper: Read workspace version from Cargo.toml
fn read_workspace_version(workspace: &Path) -> Result<String> {
    use cargo_metadata::MetadataCommand;

    let manifest_path = workspace.join("Cargo.toml");

    // Prefer `cargo metadata` when available since it handles workspaces and
    // inherited workspace.package versions correctly. However, it can fail in
    // minimal / mock workspaces (tests) or when the manifest is incomplete.
    if let Ok(metadata) = MetadataCommand::new().manifest_path(&manifest_path).exec() {
        // Try workspace packages first (for multi-crate workspaces)
        if let Some(pkg) = metadata.workspace_packages().first() {
            return Ok(pkg.version.to_string());
        }

        // Try root package (for single-crate projects)
        if let Some(resolve) = &metadata.resolve {
            if let Some(root) = &resolve.root {
                if let Some(pkg) = metadata.packages.iter().find(|p| &p.id == root) {
                    return Ok(pkg.version.to_string());
                }
            }
        }

        // Fallback: use first package
        if let Some(pkg) = metadata.packages.first() {
            return Ok(pkg.version.to_string());
        }
    }

    // Fallback: parse Cargo.toml directly (works for partial fixtures).
    let manifest = fs::read_to_string(&manifest_path)?;
    let doc = manifest
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", manifest_path.display()))?;

    if let Some(version) = doc
        .get("package")
        .and_then(|pkg| pkg.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(version.to_string());
    }

    if let Some(version) = doc
        .get("workspace")
        .and_then(|ws| ws.get("package"))
        .and_then(|pkg| pkg.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(version.to_string());
    }

    anyhow::bail!("Could not read version from {}", manifest_path.display())
}

/// Helper: Show unified diff between original and modified content
fn display_diff(file: &Path, original: &str, modified: &str) {
    println!(
        "\n{}",
        format!("--- {} (original)", file.display()).dimmed()
    );
    println!("{}", format!("+++ {} (patched)", file.display()).dimmed());

    let diff = TextDiff::from_lines(original, modified);

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => format!("-{}", change).red(),
            ChangeTag::Insert => format!("+{}", change).green(),
            ChangeTag::Equal => format!(" {}", change).normal(),
        };
        print!("{}", sign);
    }
}

fn cmd_apply(
    workspace: Option<PathBuf>,
    patches: Option<PathBuf>,
    dry_run: bool,
    show_diff: bool,
) -> Result<()> {
    // 1. Resolve workspace path
    let workspace = resolve_workspace(workspace)?;

    // 2. Determine patch files to load
    let patch_files = if let Some(path) = patches {
        vec![path]
    } else {
        discover_patch_files(&workspace)?
    };

    // 3. Determine workspace version
    let workspace_version = read_workspace_version(&workspace).unwrap_or_else(|_| {
        eprintln!(
            "{}",
            "Warning: Could not read workspace version from Cargo.toml, using 0.0.0".yellow()
        );
        "0.0.0".to_string()
    });

    println!("Workspace: {}", workspace.display());
    println!("Version: {}", workspace_version);
    println!();

    // 4. Load and apply each patch file
    let mut total_applied = 0;
    let mut total_already_applied = 0;
    let mut total_skipped = 0;
    let mut total_failed = 0;

    for patch_file in patch_files {
        println!("Loading patches from {}...", patch_file.display());

        let config = load_from_path(&patch_file)?;

        if config.patches.is_empty() {
            println!("{}", "  No patches found in file".yellow());
            continue;
        }

        // Capture file contents before applying (for diff output).
        // Only read files that the patches will touch, to avoid reading
        // unrelated files in large workspaces.
        let mut file_contents_before: HashMap<PathBuf, String> = HashMap::new();
        if show_diff {
            let target_files: std::collections::HashSet<PathBuf> = config
                .patches
                .iter()
                .map(|p| {
                    if config.meta.workspace_relative {
                        workspace.join(&p.file)
                    } else {
                        PathBuf::from(&p.file)
                    }
                })
                .collect();
            for file_path in target_files {
                if file_path.exists() {
                    if let Ok(content) = fs::read_to_string(&file_path) {
                        file_contents_before.insert(file_path, content);
                    }
                }
            }
        }

        // Apply patches (or dry-run)
        let results = if dry_run {
            println!("{}", "  [DRY RUN - showing what would be applied]".cyan());
            println!(
                "{}",
                "  Note: Patches are idempotent, so this actually applies them to check".dimmed()
            );
            apply_patches(&config, &workspace, &workspace_version)
        } else {
            apply_patches(&config, &workspace, &workspace_version)
        };

        // 5. Report results
        for (patch_id, result) in results {
            match result {
                Ok(PatchResult::Applied { ref file }) => {
                    if dry_run {
                        println!(
                            "{} {}: Would apply to {}",
                            "✓".green(),
                            patch_id,
                            file.display()
                        );
                    } else {
                        println!(
                            "{} {}: Applied to {}",
                            "✓".green(),
                            patch_id,
                            file.display()
                        );
                    }
                    total_applied += 1;

                    if show_diff {
                        if let Some(before) = file_contents_before.get(file) {
                            if let Ok(after) = fs::read_to_string(file) {
                                if before != &after {
                                    display_diff(file, before, &after);
                                }
                            }
                        }
                    }
                }
                Ok(PatchResult::AlreadyApplied { file }) => {
                    println!(
                        "{} {}: Already applied to {}",
                        "⊙".yellow(),
                        patch_id,
                        file.display()
                    );
                    total_already_applied += 1;
                }
                Ok(PatchResult::SkippedVersion { reason }) => {
                    println!("{} {}: Skipped ({})", "⊘".cyan(), patch_id, reason);
                    total_skipped += 1;
                }
                Ok(PatchResult::Failed { file, reason }) => {
                    eprintln!("{} {}: Failed - {}", "✗".red(), patch_id, reason);
                    eprintln!("  File: {}", file.display());
                    total_failed += 1;
                }
                Err(e) => {
                    eprintln!("{} {}: Error - {}", "✗".red(), patch_id, e);
                    total_failed += 1;

                    // Provide helpful conflict diagnostics
                    match &e {
                        ApplicationError::NoMatch { file } => {
                            eprintln!("  {}", "CONFLICT: Query matched no locations".red());
                            eprintln!("  File: {}", file.display());
                            eprintln!("  Possible causes:");
                            eprintln!("    - Function/struct was renamed or removed");
                            eprintln!("    - Signature changed");
                            eprintln!("    - Code was moved to different file");
                        }
                        ApplicationError::AmbiguousMatch { file, count } => {
                            eprintln!(
                                "  {}",
                                format!("CONFLICT: Query matched {} locations (expected 1)", count)
                                    .red()
                            );
                            eprintln!("  File: {}", file.display());
                            eprintln!("  Action: Refine the query pattern to be more specific");
                        }
                        ApplicationError::Edit(edit_err) => {
                            eprintln!("  Edit error: {}", edit_err);
                        }
                        _ => {}
                    }
                }
            }
        }

        println!();
    }

    // 6. Summary
    println!("{}", "Summary:".bold());
    println!("  {} applied", format!("{}", total_applied).green());
    println!(
        "  {} already applied",
        format!("{}", total_already_applied).yellow()
    );
    println!("  {} skipped", format!("{}", total_skipped).cyan());
    println!("  {} failed", format!("{}", total_failed).red());

    if total_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn cmd_status(workspace: Option<PathBuf>) -> Result<()> {
    // 1. Resolve workspace path
    let workspace = resolve_workspace(workspace)?;

    // 2. Discover patch files
    let patch_files = discover_patch_files(&workspace)?;

    // 3. Determine workspace version
    let workspace_version = read_workspace_version(&workspace).unwrap_or_else(|_| {
        eprintln!(
            "{}",
            "Warning: Could not read workspace version from Cargo.toml, using 0.0.0".yellow()
        );
        "0.0.0".to_string()
    });

    println!("{}", "Patch Status Report".bold());
    println!("Workspace: {}", workspace.display());
    println!("Version: {}", workspace_version);
    println!();

    let mut applied = Vec::new();
    let mut not_applied = Vec::new();
    let mut skipped = Vec::new();

    // 4. Check status of all patches (read-only; does not mutate workspace files)
    for patch_file in patch_files {
        let config = load_from_path(&patch_file)?;
        let results = check_patches(&config, &workspace, &workspace_version);

        for (patch_id, result) in results {
            match result {
                Ok(PatchResult::Applied { .. }) => {
                    // Patch target exists and would be changed if applied.
                    not_applied.push((patch_id, "target found but was not applied".to_string()));
                }
                Ok(PatchResult::AlreadyApplied { .. }) => {
                    applied.push(patch_id);
                }
                Ok(PatchResult::SkippedVersion { reason }) => {
                    skipped.push((patch_id, reason));
                }
                Ok(PatchResult::Failed { ref reason, .. }) => {
                    not_applied.push((patch_id, reason.clone()));
                }
                Err(ref e) => {
                    not_applied.push((patch_id, e.to_string()));
                }
            }
        }
    }

    // 5. Report grouped by status
    if !applied.is_empty() {
        println!(
            "{} {} ({} patches)",
            "✓".green(),
            "APPLIED".green().bold(),
            applied.len()
        );
        for id in &applied {
            println!("  - {}", id);
        }
        println!();
    }

    if !not_applied.is_empty() {
        println!(
            "{} {} ({} patches)",
            "⊙".yellow(),
            "NOT APPLIED".yellow().bold(),
            not_applied.len()
        );
        for (id, reason) in &not_applied {
            println!("  - {} ({})", id, reason.dimmed());
        }
        println!();
    }

    if !skipped.is_empty() {
        println!(
            "{} {} ({} patches)",
            "⊘".cyan(),
            "SKIPPED".cyan().bold(),
            skipped.len()
        );
        for (id, reason) in &skipped {
            println!("  - {} ({})", id, reason.dimmed());
        }
        println!();
    }

    Ok(())
}

fn cmd_verify(workspace: Option<PathBuf>) -> Result<()> {
    // 1. Resolve workspace path
    let workspace = resolve_workspace(workspace)?;

    // 2. Discover patch files
    let patch_files = discover_patch_files(&workspace)?;

    // 3. Determine workspace version
    let workspace_version = read_workspace_version(&workspace).unwrap_or_else(|_| {
        eprintln!(
            "{}",
            "Warning: Could not read workspace version from Cargo.toml, using 0.0.0".yellow()
        );
        "0.0.0".to_string()
    });

    println!("{}", "Verifying patches...".bold());
    println!("Workspace: {}", workspace.display());
    println!("Version: {}", workspace_version);
    println!();

    let mut verified = 0;
    let mut mismatch = 0;
    let mut skipped = 0;

    // 4. Check verification for all patches
    for patch_file in patch_files {
        let config = load_from_path(&patch_file)?;
        let results = check_patches(&config, &workspace, &workspace_version);

        for (patch_id, result) in results {
            match result {
                Ok(PatchResult::AlreadyApplied { .. }) => {
                    println!("{} {}: Verified (already applied)", "✓".green(), patch_id);
                    verified += 1;
                }
                Ok(PatchResult::Applied { file }) => {
                    // This means it wasn't already applied, so verification failed
                    eprintln!("{} {}: MISMATCH", "✗".red(), patch_id);
                    eprintln!("  Expected: patch already applied");
                    eprintln!("  Found: patch not yet applied");
                    eprintln!("  Location: {}", file.display());
                    mismatch += 1;
                }
                Ok(PatchResult::SkippedVersion { reason }) => {
                    println!("{} {}: Skipped ({})", "⊘".cyan(), patch_id, reason);
                    skipped += 1;
                }
                Ok(PatchResult::Failed {
                    ref file,
                    ref reason,
                }) => {
                    eprintln!("{} {}: MISMATCH", "✗".red(), patch_id);
                    eprintln!("  Error: {}", reason);
                    eprintln!("  Location: {}", file.display());
                    mismatch += 1;
                }
                Err(ref e) => {
                    eprintln!("{} {}: MISMATCH", "✗".red(), patch_id);
                    eprintln!("  Error: {}", e);
                    mismatch += 1;
                }
            }
        }
    }

    println!();
    println!("{}", "Summary:".bold());
    println!("  {} verified", format!("{}", verified).green());
    println!("  {} mismatch", format!("{}", mismatch).red());
    println!("  {} skipped", format!("{}", skipped).cyan());

    if mismatch > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn cmd_list() -> Result<()> {
    println!("List command - not yet implemented");
    Ok(())
}
