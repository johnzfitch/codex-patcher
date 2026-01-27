//! End-to-end workflow test
//!
//! Tests the complete workflow:
//! 1. Discover patches
//! 2. Apply patches
//! 3. Verify patches
//! 4. Check idempotency

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Create a minimal mock workspace for e2e testing
fn setup_e2e_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Create directory structure
    fs::create_dir_all(dir.path().join("otel/src")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/config")).unwrap();
    fs::create_dir_all(dir.path().join("patches")).unwrap();

    // Create Cargo.toml for workspace
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["otel", "core"]

[package]
name = "test-codex"
version = "0.88.0"
edition = "2021"
"#,
    )
    .unwrap();

    // Create mock otel/src/config.rs
    fs::write(
        dir.path().join("otel/src/config.rs"),
        r#"
pub(crate) const STATSIG_OTLP_HTTP_ENDPOINT: &str = "https://ab.chatgpt.com/otlp/v1/metrics";
pub(crate) const STATSIG_API_KEY_HEADER: &str = "statsig-api-key";
pub(crate) const STATSIG_API_KEY: &str = "client-MkRuleRQBd6qakfnDYqJVR9JuXcY57Ljly3vi5JVUIO";

pub enum OtelExporter {
    None,
    Statsig,
}

impl Clone for OtelExporter {
    fn clone(&self) -> Self {
        match self {
            OtelExporter::None => OtelExporter::None,
            OtelExporter::Statsig => OtelExporter::Statsig,
        }
    }
}

pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => {
            if cfg!(test) {
                return OtelExporter::None;
            }
            OtelExporter::Statsig
        }
        _ => exporter.clone(),
    }
}
"#,
    )
    .unwrap();

    // Create mock core/src/config/types.rs
    fs::write(
        dir.path().join("core/src/config/types.rs"),
        r#"
pub enum OtelExporterKind {
    None,
    Statsig,
}

pub struct OtelConfig {
    pub metrics_exporter: OtelExporterKind,
}

impl Default for OtelConfig {
    fn default() -> Self {
        OtelConfig {
            metrics_exporter: OtelExporterKind::Statsig,
        }
    }
}
"#,
    )
    .unwrap();

    dir
}

#[test]
fn test_e2e_workflow() {
    let workspace = setup_e2e_workspace();
    let workspace_path = workspace.path();

    println!("Created test workspace at: {:?}", workspace_path);

    // Copy privacy patches to workspace
    let privacy_patch = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy.toml");
    fs::copy(
        &privacy_patch,
        workspace_path.join("patches/privacy.toml"),
    )
    .unwrap();

    let binary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug/codex-patcher");

    // Step 1: Apply patches
    println!("\n=== Step 1: Apply patches ===");
    let output = Command::new(&binary)
        .args(["apply", "--workspace", workspace_path.to_str().unwrap()])
        .output()
        .expect("Failed to run apply command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT:\n{}", stdout);
    if !stderr.is_empty() {
        println!("STDERR:\n{}", stderr);
    }

    assert!(
        stdout.contains("Applied") || stdout.contains("already applied"),
        "Should apply or report already applied"
    );

    // Verify files were modified
    let otel_content = fs::read_to_string(workspace_path.join("otel/src/config.rs")).unwrap();
    assert!(
        otel_content.contains("OtelExporter::None") && otel_content.contains("PRIVACY PATCH"),
        "otel/src/config.rs should be patched"
    );

    // Step 2: Verify patches
    println!("\n=== Step 2: Verify patches ===");
    let output = Command::new(&binary)
        .args(["verify", "--workspace", workspace_path.to_str().unwrap()])
        .output()
        .expect("Failed to run verify command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("STDOUT:\n{}", stdout);

    assert!(
        stdout.contains("Verified") || stdout.contains("verified"),
        "Should verify successfully"
    );

    // Step 3: Status check
    println!("\n=== Step 3: Status check ===");
    let output = Command::new(&binary)
        .args(["status", "--workspace", workspace_path.to_str().unwrap()])
        .output()
        .expect("Failed to run status command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("STDOUT:\n{}", stdout);

    assert!(
        stdout.contains("Patch Status Report"),
        "Should show status report"
    );

    // Step 4: Re-apply (idempotency check)
    println!("\n=== Step 4: Re-apply (idempotency) ===");
    let output = Command::new(&binary)
        .args(["apply", "--workspace", workspace_path.to_str().unwrap()])
        .output()
        .expect("Failed to run apply command again");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("STDOUT:\n{}", stdout);

    // Should not fail on re-application
    assert!(output.status.success() || stdout.contains("already applied"));

    println!("\n✓ End-to-end workflow test passed!");
}
