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

/// Create a minimal mock workspace for e2e testing (alpha.14+ code layout)
fn setup_e2e_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();

    fs::create_dir_all(dir.path().join("otel/src")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/config")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/rollout")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/tools")).unwrap();
    fs::create_dir_all(dir.path().join("state/src")).unwrap();
    fs::create_dir_all(dir.path().join("patches")).unwrap();

    // Cargo.toml — version must satisfy privacy-v0.99.toml range (>=0.99.0-alpha.14)
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["otel", "core"]

[package]
name = "test-codex"
version = "0.99.0-alpha.16"
edition = "2021"
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("otel/src/config.rs"),
        r#"
use std::collections::HashMap;

pub(crate) const STATSIG_OTLP_HTTP_ENDPOINT: &str = "https://ab.chatgpt.com/otlp/v1/metrics";
pub(crate) const STATSIG_API_KEY_HEADER: &str = "statsig-api-key";
pub(crate) const STATSIG_API_KEY: &str = "client-REDACTED";

pub enum OtelExporter {
    None,
    Statsig,
    OtlpHttp { endpoint: String, headers: HashMap<String, String> },
}

impl Clone for OtelExporter {
    fn clone(&self) -> Self {
        match self {
            OtelExporter::None => OtelExporter::None,
            OtelExporter::Statsig => OtelExporter::Statsig,
            OtelExporter::OtlpHttp { endpoint, headers } => OtelExporter::OtlpHttp {
                endpoint: endpoint.clone(), headers: headers.clone(),
            },
        }
    }
}

pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => OtelExporter::OtlpHttp {
            endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
            headers: HashMap::from([(STATSIG_API_KEY_HEADER.to_string(), STATSIG_API_KEY.to_string())]),
        },
        _ => exporter.clone(),
    }
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("core/src/config/types.rs"),
        r#"
pub enum OtelExporterKind { None, Statsig }

pub struct OtelConfig {
    pub log_user_prompt: bool,
    pub environment: String,
    pub exporter: OtelExporterKind,
    pub trace_exporter: OtelExporterKind,
    pub metrics_exporter: OtelExporterKind,
}

const DEFAULT_OTEL_ENVIRONMENT: &str = "production";

impl Default for OtelConfig {
    fn default() -> Self {
        OtelConfig {
            log_user_prompt: false,
            environment: DEFAULT_OTEL_ENVIRONMENT.to_owned(),
            exporter: OtelExporterKind::None,
            trace_exporter: OtelExporterKind::None,
            metrics_exporter: OtelExporterKind::Statsig,
        }
    }
}
"#,
    )
    .unwrap();

    // alpha.14+ style: unwrap_or form + Constrained<WebSearchMode>
    // Text must match patch search strings exactly (indentation, blank lines, line breaks).
    fs::write(
        dir.path().join("core/src/config/mod.rs"),
        r#"
pub enum OtelExporterKind { None, Statsig }
pub struct OtelConfigToml { pub metrics_exporter: Option<OtelExporterKind> }
pub struct OtelConfig { pub metrics_exporter: OtelExporterKind }
pub struct Config { pub otel: OtelConfig }

pub fn load_config(t: OtelConfigToml) -> Config {
                let metrics_exporter = t.metrics_exporter.unwrap_or(OtelExporterKind::Statsig);
    Config { otel: OtelConfig { metrics_exporter } }
}

pub struct Constrained<T> { value: T }
impl<T: Copy> Constrained<T> {
    pub fn allow_any(v: T) -> Self { Self { value: v } }
    pub fn value(&self) -> T { self.value }
    pub fn can_set(&self, _: &T) -> Result<(), ()> { Ok(()) }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WebSearchMode { Disabled, Cached, Live }
#[derive(Clone, Copy)]
pub enum SandboxPolicy { ReadOnly, DangerFullAccess }

fn resolve_web_search_mode(_cfg: &(), _profile: &(), _features: &()) -> Option<WebSearchMode> { None }

pub fn default_web_search_mode(cfg: (), config_profile: (), features: ()) -> WebSearchMode {
        let web_search_mode = resolve_web_search_mode(&cfg, &config_profile, &features)
            .unwrap_or(WebSearchMode::Cached);
    web_search_mode
}

pub(crate) fn resolve_web_search_mode_for_turn(
    web_search_mode: &Constrained<WebSearchMode>,
    sandbox_policy: &SandboxPolicy,
) -> WebSearchMode {
    let preferred = web_search_mode.value();
    if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) { WebSearchMode::Live } else { preferred }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_mode_for_turn_prefers_live_for_danger_full_access() {
        let web_search_mode = Constrained::allow_any(WebSearchMode::Cached);
        let mode =
            resolve_web_search_mode_for_turn(&web_search_mode, &SandboxPolicy::DangerFullAccess);

        assert_eq!(mode, WebSearchMode::Live);
    }

    #[test]
    fn metrics_exporter_defaults_to_statsig_when_missing() {
        let config = load_config(OtelConfigToml { metrics_exporter: None });
        assert_eq!(config.otel.metrics_exporter, OtelExporterKind::Statsig);
    }
}
"#,
    )
    .unwrap();

    // core/src/turn_metadata.rs — alpha.21+ signature (targeted by privacy-v0.99)
    fs::write(
        dir.path().join("core/src/turn_metadata.rs"),
        r#"
use std::collections::BTreeMap;
use std::path::Path;
use serde::Serialize;

use crate::git_info::get_git_remote_urls_assume_git_repo;
use crate::git_info::get_git_repo_root;
use crate::git_info::get_head_commit_hash;

#[derive(Serialize)]
struct TurnMetadataBag {
    turn_id: Option<String>,
    workspaces: BTreeMap<String, ()>,
    sandbox: Option<String>,
}

pub async fn build_turn_metadata_header(cwd: &Path, sandbox: Option<&str>) -> Option<String> {
    let _ = (cwd, sandbox);
    let _ = (get_git_repo_root, get_head_commit_hash, get_git_remote_urls_assume_git_repo);
    None
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("core/src/tools/registry.rs"),
        r#"
pub struct SandboxPolicy;
pub struct Turn { pub sandbox_policy: SandboxPolicy, pub windows_sandbox_level: u8 }
pub struct ToolInvocation { pub turn: Turn }
pub fn sandbox_tag(_: &SandboxPolicy, _: u8) -> &'static str { "none" }
pub fn sandbox_policy_tag(_: &SandboxPolicy) -> &'static str { "read_only" }

pub fn dispatch(invocation: ToolInvocation) {
        let metric_tags = [
            (
                "sandbox",
                sandbox_tag(
                    &invocation.turn.sandbox_policy,
                    invocation.turn.windows_sandbox_level,
                ),
            ),
            (
                "sandbox_policy",
                sandbox_policy_tag(&invocation.turn.sandbox_policy),
            ),
        ];
    let _ = metric_tags;
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("state/src/extract.rs"),
        r#"
fn apply_meta_line(metadata: &mut ThreadMetadata, meta_line: &MetaLine) {
    if let Some(git) = meta_line.git.as_ref() {
        metadata.git_sha = git.commit_hash.clone();
        metadata.git_branch = git.branch.clone();
        metadata.git_origin_url = git.repository_url.clone();
    }
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("core/src/rollout/metadata.rs"),
        r#"
fn build_metadata(session_meta: &SessionMeta) -> Option<ThreadMetadataBuilder> {
    let mut builder = ThreadMetadataBuilder::default();
    if let Some(git) = session_meta.git.as_ref() {
        builder.git_sha = git.commit_hash.clone();
        builder.git_branch = git.branch.clone();
        builder.git_origin_url = git.repository_url.clone();
    }
    Some(builder)
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
    let privacy_patch =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("patches/privacy-v0.99.toml");
    fs::copy(&privacy_patch, workspace_path.join("patches/privacy-v0.99.toml")).unwrap();

    let binary =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/codex-patcher");

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
    assert!(output.status.success(), "Apply command should succeed");

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

    assert!(output.status.success(), "Verify command should succeed");
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

    assert!(output.status.success(), "Status command should succeed");
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
    assert!(output.status.success(), "Re-apply should succeed");

    println!("\n✓ End-to-end workflow test passed!");
}
