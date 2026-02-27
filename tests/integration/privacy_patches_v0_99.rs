//! Integration tests for v0.99 privacy patch version gating and per-version behavior.
//!
//! Tests alpha.16 (unwrap_or era) and alpha.23 (&Path sandbox era) against
//! the consolidated privacy-v0.99.toml (>=0.99.0-alpha.14, <0.105.0-alpha.13).

use codex_patcher::config::{apply_patches, load_from_path, PatchResult};
use std::fs;
use tempfile::TempDir;

fn patch_file() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("patches/privacy-v0.99.toml")
}

/// alpha.14-alpha.20 era: metrics_exporter uses unwrap_or form; turn_metadata takes PathBuf.
fn write_alpha16_workspace(dir: &TempDir) {
    fs::create_dir_all(dir.path().join("otel/src")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/config")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/rollout")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/tools")).unwrap();
    fs::create_dir_all(dir.path().join("state/src")).unwrap();

    fs::write(
        dir.path().join("otel/src/config.rs"),
        r#"
use std::collections::HashMap;

pub(crate) const STATSIG_OTLP_HTTP_ENDPOINT: &str = "https://example.invalid/otlp/v1/metrics";
pub(crate) const STATSIG_API_KEY_HEADER: &str = "statsig-api-key";
pub(crate) const STATSIG_API_KEY: &str = "client-REDACTED";

pub enum OtelExporter { None, Statsig, OtlpHttp { endpoint: String, headers: HashMap<String, String> } }
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
        dir.path().join("core/src/config/types.rs"),
        r#"
pub enum OtelExporterKind { None, Statsig }
pub struct OtelConfig {
    pub log_user_prompt: bool, pub environment: String,
    pub exporter: OtelExporterKind, pub trace_exporter: OtelExporterKind,
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
pub(crate) fn builder_from_session_meta(session_meta: &SessionMetaLine) {
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
}

/// alpha.21+ era: turn_metadata takes (&Path, Option<&str>) with sandbox field.
fn write_alpha23_workspace(dir: &TempDir) {
    write_alpha16_workspace(dir);

    fs::write(
        dir.path().join("core/src/turn_metadata.rs"),
        r#"
use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use serde::Serialize;
use tracing::warn;
use crate::git_info::get_git_remote_urls_assume_git_repo;
use crate::git_info::get_git_repo_root;
use crate::git_info::get_head_commit_hash;

pub(crate) const TURN_METADATA_HEADER_TIMEOUT: Duration = Duration::from_millis(250);

pub(crate) async fn resolve_turn_metadata_header_with_timeout<F>(
    build_header: F,
    fallback_on_timeout: Option<String>,
) -> Option<String>
where F: Future<Output = Option<String>> {
    match tokio::time::timeout(TURN_METADATA_HEADER_TIMEOUT, build_header).await {
        Ok(header) => header,
        Err(_) => { warn!("timed out"); fallback_on_timeout }
    }
}

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
}

#[test]
fn test_privacy_patches_v0_99_alpha16_apply() {
    let workspace = TempDir::new().unwrap();
    write_alpha16_workspace(&workspace);

    let config = load_from_path(patch_file()).expect("Failed to load privacy-v0.99.toml");
    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.16");

    assert!(
        results.iter().any(|(_, r)| matches!(
            r,
            Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. })
        )),
        "Expected at least one patch to apply for alpha.16"
    );

    let config_mod = fs::read_to_string(workspace.path().join("core/src/config/mod.rs")).unwrap();
    assert!(
        config_mod.contains("t.metrics_exporter.unwrap_or(OtelExporterKind::None)"),
        "Expected config to default metrics_exporter to None"
    );
    assert!(
        config_mod.contains("assert_eq!(config.otel.metrics_exporter, OtelExporterKind::None)"),
        "Expected test assertion to be updated"
    );

    let tools = fs::read_to_string(workspace.path().join("core/src/tools/registry.rs")).unwrap();
    assert!(
        tools.contains("let metric_tags: [(&str, &str); 0] = [];"),
        "Expected tool metric tags to be emptied"
    );
}

#[test]
fn test_privacy_patches_v0_99_alpha23_apply() {
    let workspace = TempDir::new().unwrap();
    write_alpha23_workspace(&workspace);

    let config = load_from_path(patch_file()).expect("Failed to load privacy-v0.99.toml");
    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.23");

    assert!(
        results.iter().any(|(_, r)| matches!(
            r,
            Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. })
        )),
        "Expected at least one patch to apply for alpha.23"
    );

    let turn_metadata =
        fs::read_to_string(workspace.path().join("core/src/turn_metadata.rs")).unwrap();
    assert!(
        turn_metadata.contains("Disable per-turn metadata headers"),
        "Expected turn metadata header builder to be disabled"
    );

    let config_mod = fs::read_to_string(workspace.path().join("core/src/config/mod.rs")).unwrap();
    assert!(
        config_mod.contains("t.metrics_exporter.unwrap_or(OtelExporterKind::None)"),
        "Expected metrics_exporter to default to None"
    );

    let tools = fs::read_to_string(workspace.path().join("core/src/tools/registry.rs")).unwrap();
    assert!(
        tools.contains("let metric_tags: [(&str, &str); 0] = [];"),
        "Expected tool metric tags to be emptied"
    );
}

#[test]
fn test_privacy_patches_v0_99_version_gating() {
    // privacy-v0.99.toml must skip on versions outside its range.
    let workspace = TempDir::new().unwrap();
    write_alpha23_workspace(&workspace);

    let config = load_from_path(patch_file()).expect("Failed to load privacy-v0.99.toml");

    // Too new (0.105 range handled by privacy-v0.105-alpha13.toml)
    let results = apply_patches(&config, workspace.path(), "0.105.0-alpha.13");
    assert!(
        results
            .iter()
            .all(|(_, r)| matches!(r, Ok(PatchResult::SkippedVersion { .. }))),
        "Expected all patches skipped on 0.105.0-alpha.13"
    );

    // Too old (below supported range)
    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.11");
    assert!(
        results
            .iter()
            .all(|(_, r)| matches!(r, Ok(PatchResult::SkippedVersion { .. }))),
        "Expected all patches skipped on 0.99.0-alpha.11 (below alpha.14)"
    );
}
