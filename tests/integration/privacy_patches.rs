//! Integration tests for the merged privacy-v0.99 patch set.
//!
//! Covers Codex versions >=0.99.0-alpha.14, <0.105.0-alpha.13.
//! Uses an alpha.16-era mock workspace (unwrap_or metrics_exporter form).

use codex_patcher::config::{apply_patches, load_from_path, PatchResult};
use std::fs;
use tempfile::TempDir;

fn setup_mock_codex_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();

    fs::create_dir_all(dir.path().join("otel/src")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/config")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/rollout")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/tools")).unwrap();
    fs::create_dir_all(dir.path().join("state/src")).unwrap();

    // core/src/turn_metadata.rs — alpha.21+ signature
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

    // otel/src/config.rs — Statsig constants + resolver
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
                endpoint: endpoint.clone(),
                headers: headers.clone(),
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

    // core/src/config/types.rs — Statsig default
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

    // core/src/config/mod.rs — alpha.14+ style (unwrap_or form)
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
    pub fn allow_any(value: T) -> Self { Self { value } }
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

    // core/src/tools/registry.rs — sandbox metric tags
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

    // state/src/extract.rs — git origin URL
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

    // core/src/rollout/metadata.rs
    fs::write(
        dir.path().join("core/src/rollout/metadata.rs"),
        r#"
fn build_metadata(session_meta: &SessionMeta) {
    let mut builder = ThreadMetadataBuilder::default();
    if let Some(git) = session_meta.git.as_ref() {
        builder.git_sha = git.commit_hash.clone();
        builder.git_branch = git.branch.clone();
        builder.git_origin_url = git.repository_url.clone();
    }
}
"#,
    )
    .unwrap();

    dir
}

fn patch_file() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("patches/privacy-v0.99.toml")
}

#[test]
fn test_privacy_patches_apply() {
    let workspace = setup_mock_codex_workspace();
    let config = load_from_path(&patch_file()).expect("Failed to load privacy-v0.99.toml");
    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.16");

    let mut applied = 0;
    let mut already_applied = 0;
    for (patch_id, result) in results {
        match result {
            Ok(PatchResult::Applied { .. }) => { applied += 1; }
            Ok(PatchResult::AlreadyApplied { .. }) => { already_applied += 1; }
            Ok(PatchResult::SkippedVersion { reason }) => {
                println!("⊘ {}: Skipped - {}", patch_id, reason);
            }
            Ok(PatchResult::Failed { reason, .. }) => panic!("✗ {}: Failed - {}", patch_id, reason),
            Err(e) => panic!("✗ {}: Error - {}", patch_id, e),
        }
    }

    assert!(applied > 0 || already_applied > 0, "No patches were applied");

    let otel = fs::read_to_string(workspace.path().join("otel/src/config.rs")).unwrap();
    assert!(
        !otel.contains("STATSIG_OTLP_HTTP_ENDPOINT") || otel.contains("// PRIVACY PATCH"),
        "Statsig endpoint should be removed"
    );
    assert!(
        otel.contains("OtelExporter::None") && otel.contains("PRIVACY PATCH"),
        "resolve_exporter should return None with privacy comment"
    );

    let types = fs::read_to_string(workspace.path().join("core/src/config/types.rs")).unwrap();
    assert!(
        types.contains("metrics_exporter: OtelExporterKind::None"),
        "metrics_exporter should default to None"
    );

    let config_mod = fs::read_to_string(workspace.path().join("core/src/config/mod.rs")).unwrap();
    assert!(
        config_mod.contains("WebSearchMode::Disabled"),
        "web search should default to Disabled"
    );
    assert!(
        config_mod.contains("t.metrics_exporter.unwrap_or(OtelExporterKind::None)"),
        "config loading should default metrics_exporter to None"
    );
}

#[test]
fn test_privacy_patches_idempotent() {
    let workspace = setup_mock_codex_workspace();
    let config = load_from_path(&patch_file()).expect("Failed to load privacy-v0.99.toml");

    let count_success = |results: Vec<(String, Result<PatchResult, _>)>| {
        results
            .into_iter()
            .filter(|(_, r)| {
                matches!(r, Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. }))
            })
            .count()
    };

    let first = count_success(apply_patches(&config, workspace.path(), "0.99.0-alpha.16"));
    let second = count_success(apply_patches(&config, workspace.path(), "0.99.0-alpha.16"));

    assert!(first > 0, "First run should apply patches");
    assert_eq!(first, second, "Patch application must be idempotent");
}

#[test]
fn test_privacy_patches_no_telemetry_strings() {
    let workspace = setup_mock_codex_workspace();
    let config = load_from_path(&patch_file()).expect("Failed to load privacy-v0.99.toml");
    apply_patches(&config, workspace.path(), "0.99.0-alpha.16");

    let otel = fs::read_to_string(workspace.path().join("otel/src/config.rs")).unwrap();

    let has_live_endpoint = otel
        .lines()
        .filter(|l| !l.trim().starts_with("//"))
        .any(|l| l.contains("ab.chatgpt.com"));

    let has_live_key = otel
        .lines()
        .filter(|l| !l.trim().starts_with("//"))
        .any(|l| l.contains("STATSIG_API_KEY:"));

    assert!(!has_live_endpoint, "Live ab.chatgpt.com endpoint must not exist");
    assert!(!has_live_key, "Live API key must not exist");
}
