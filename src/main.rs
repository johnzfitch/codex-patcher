use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// Path to workspace root
        #[arg(short, long)]
        workspace: PathBuf,

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
        /// Path to workspace root
        #[arg(short, long)]
        workspace: PathBuf,
    },

    /// Verify patches are applicable to current workspace
    Verify {
        /// Path to workspace root
        #[arg(short, long)]
        workspace: PathBuf,
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
        } => {
            println!("Apply command:");
            println!("  Workspace: {}", workspace.display());
            println!("  Patches: {:?}", patches);
            println!("  Dry run: {}", dry_run);
            println!("  Show diff: {}", diff);
            println!("\nNot yet implemented - Phase 1 (core edit primitive) complete.");
            println!("Next: Implement patch config parser and span locators.");
        }

        Commands::Status { workspace } => {
            println!("Status command:");
            println!("  Workspace: {}", workspace.display());
            println!("\nNot yet implemented.");
        }

        Commands::Verify { workspace } => {
            println!("Verify command:");
            println!("  Workspace: {}", workspace.display());
            println!("\nNot yet implemented.");
        }

        Commands::List => {
            println!("List command:");
            println!("\nNot yet implemented.");
        }
    }

    Ok(())
}
