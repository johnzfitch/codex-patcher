//! Integration tests for v0.99 privacy patches
//!
//! These tests focus on:
//! - Correct version_range gating for prerelease versions (alpha tags)
//! - The new coverages added around metrics exporter defaults and per-turn metadata headers

use codex_patcher::config::{apply_patches, load_from_path, PatchResult};
use std::fs;
use tempfile::TempDir;

fn write_alpha12_workspace(dir: &TempDir) {
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

pub enum OtelHttpProtocol {
    Json,
}

pub enum OtelExporter {
    None,
    Statsig,
    OtlpHttp {
        endpoint: String,
        headers: HashMap<String, String>,
        protocol: OtelHttpProtocol,
        tls: Option<()>,
    },
}

impl Clone for OtelExporter {
    fn clone(&self) -> Self {
        match self {
            OtelExporter::None => OtelExporter::None,
            OtelExporter::Statsig => OtelExporter::Statsig,
            OtelExporter::OtlpHttp {
                endpoint,
                headers,
                protocol,
                tls,
            } => OtelExporter::OtlpHttp {
                endpoint: endpoint.clone(),
                headers: headers.clone(),
                protocol: match protocol {
                    OtelHttpProtocol::Json => OtelHttpProtocol::Json,
                },
                tls: *tls,
            },
        }
    }
}

pub(crate) fn resolve_exporter(exporter: &OtelExporter) -> OtelExporter {
    match exporter {
        OtelExporter::Statsig => OtelExporter::OtlpHttp {
            endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
            headers: HashMap::from([(
                STATSIG_API_KEY_HEADER.to_string(),
                STATSIG_API_KEY.to_string(),
            )]),
            protocol: OtelHttpProtocol::Json,
            tls: None,
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

pub struct Turn {
    pub sandbox_policy: SandboxPolicy,
    pub windows_sandbox_level: u8,
}

pub struct ToolInvocation {
    pub turn: Turn,
}

pub fn sandbox_tag(_policy: &SandboxPolicy, _windows_sandbox_level: u8) -> &'static str {
    "none"
}

pub fn sandbox_policy_tag(_policy: &SandboxPolicy) -> &'static str {
    "read_only"
}

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
pub enum OtelExporterKind {
    None,
    Statsig,
}

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

    // alpha.12: metrics_exporter is hard-coded to Statsig during config load
    fs::write(
        dir.path().join("core/src/config/mod.rs"),
        r#"
pub enum OtelExporterKind {
    None,
    Statsig,
}

pub struct OtelConfig {
    pub metrics_exporter: OtelExporterKind,
}

pub struct Config {
    pub otel: OtelConfig,
}

pub fn load_config() -> Config {
    Config {
        otel: OtelConfig {
                    metrics_exporter: OtelExporterKind::Statsig,
        },
    }
}

pub struct Constrained<T> {
    value: T,
}

impl<T: Copy> Constrained<T> {
    pub fn allow_any(value: T) -> Self {
        Self { value }
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn can_set(&self, _candidate: &T) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WebSearchMode {
    Disabled,
    Cached,
    Live,
}

#[derive(Clone, Copy)]
pub enum SandboxPolicy {
    ReadOnly,
    DangerFullAccess,
}

fn resolve_web_search_mode(_cfg: &(), _profile: &(), _features: &()) -> Option<WebSearchMode> {
    None
}

pub fn default_web_search_mode() -> WebSearchMode {
        let web_search_mode = resolve_web_search_mode(&(), &(), &())
            .unwrap_or(WebSearchMode::Cached);
    web_search_mode
}

pub(crate) fn resolve_web_search_mode_for_turn(
    web_search_mode: &Constrained<WebSearchMode>,
    sandbox_policy: &SandboxPolicy,
) -> WebSearchMode {
    let preferred = web_search_mode.value();
    if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) && preferred != WebSearchMode::Disabled
    {
        WebSearchMode::Live
    } else {
        preferred
    }
}

#[test]
fn web_search_mode_for_turn_prefers_live_for_danger_full_access() {
    let web_search_mode = Constrained::allow_any(WebSearchMode::Cached);
    let mode =
        resolve_web_search_mode_for_turn(&web_search_mode, &SandboxPolicy::DangerFullAccess);

    assert_eq!(mode, WebSearchMode::Live);
}
"#,
    )
    .unwrap();

    // alpha.12: build_turn_metadata_header takes PathBuf
    fs::write(
        dir.path().join("core/src/turn_metadata.rs"),
        r#"
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
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
where
    F: Future<Output = Option<String>>,
{
    match tokio::time::timeout(TURN_METADATA_HEADER_TIMEOUT, build_header).await {
        Ok(header) => header,
        Err(_) => {
            warn!(
                "timed out after {}ms while building turn metadata header",
                TURN_METADATA_HEADER_TIMEOUT.as_millis()
            );
            fallback_on_timeout
        }
    }
}

#[derive(Serialize)]
struct TurnMetadataWorkspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    associated_remote_urls: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_git_commit_hash: Option<String>,
}

#[derive(Serialize)]
struct TurnMetadata {
    workspaces: BTreeMap<String, TurnMetadataWorkspace>,
}

pub async fn build_turn_metadata_header(cwd: PathBuf) -> Option<String> {
    let _ = cwd;
    let _ = (get_git_repo_root, get_head_commit_hash, get_git_remote_urls_assume_git_repo);
    None
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

fn write_alpha16_workspace(dir: &TempDir) {
    // Same as alpha.23 config layout for metrics exporter defaulting,
    // but turn_metadata still takes PathBuf (no sandbox field).
    write_alpha12_workspace(dir);

    fs::write(
        dir.path().join("core/src/config/mod.rs"),
        r#"
pub enum OtelExporterKind {
    None,
    Statsig,
}

pub struct OtelConfig {
    pub metrics_exporter: OtelExporterKind,
}

pub struct OtelConfigToml {
    pub metrics_exporter: Option<OtelExporterKind>,
}

pub struct Config {
    pub otel: OtelConfig,
}

pub fn load_config(t: OtelConfigToml) -> Config {
                let metrics_exporter = t.metrics_exporter.unwrap_or(OtelExporterKind::Statsig);
    Config {
        otel: OtelConfig {
            metrics_exporter,
        },
    }
}

pub struct Constrained<T> {
    value: T,
}

impl<T: Copy> Constrained<T> {
    pub fn allow_any(value: T) -> Self {
        Self { value }
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn can_set(&self, _candidate: &T) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WebSearchMode {
    Disabled,
    Cached,
    Live,
}

#[derive(Clone, Copy)]
pub enum SandboxPolicy {
    ReadOnly,
    DangerFullAccess,
}

fn resolve_web_search_mode(_cfg: &(), _profile: &(), _features: &()) -> Option<WebSearchMode> {
    None
}

pub fn default_web_search_mode() -> WebSearchMode {
        let web_search_mode = resolve_web_search_mode(&(), &(), &())
            .unwrap_or(WebSearchMode::Cached);
    web_search_mode
}

pub(crate) fn resolve_web_search_mode_for_turn(
    web_search_mode: &Constrained<WebSearchMode>,
    sandbox_policy: &SandboxPolicy,
) -> WebSearchMode {
    let preferred = web_search_mode.value();
    if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) && preferred != WebSearchMode::Disabled
    {
        WebSearchMode::Live
    } else {
        preferred
    }
}

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
"#,
    )
    .unwrap();
}

fn write_alpha23_workspace(dir: &TempDir) {
    write_alpha16_workspace(dir);

    // alpha.23: turn metadata includes sandbox and takes &Path.
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
where
    F: Future<Output = Option<String>>,
{
    match tokio::time::timeout(TURN_METADATA_HEADER_TIMEOUT, build_header).await {
        Ok(header) => header,
        Err(_) => {
            warn!(
                "timed out after {}ms while building turn metadata header",
                TURN_METADATA_HEADER_TIMEOUT.as_millis()
            );
            fallback_on_timeout
        }
    }
}

#[derive(Serialize)]
struct TurnMetadataWorkspace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    associated_remote_urls: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest_git_commit_hash: Option<String>,
}

#[derive(Serialize)]
struct TurnMetadata {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    workspaces: BTreeMap<String, TurnMetadataWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
fn test_privacy_patches_v0_99_alpha12_apply() {
    let workspace = TempDir::new().unwrap();
    write_alpha12_workspace(&workspace);

    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy-v0.99-alpha1-alpha22.toml");
    let config = load_from_path(&patch_file).expect("Failed to load alpha10-alpha13 privacy patch");

    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.12");
    assert!(
        results.iter().any(|(_, r)| matches!(
            r,
            Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. })
        )),
        "Expected at least one patch to apply for alpha.12"
    );

    let config_mod = fs::read_to_string(workspace.path().join("core/src/config/mod.rs")).unwrap();
    assert!(
        config_mod.contains("metrics_exporter: OtelExporterKind::None"),
        "Expected metrics exporter to be forced to None (hard-coded path)"
    );
    assert!(
        config_mod.contains("WebSearchMode::Disabled"),
        "Expected web search defaults to Disabled"
    );
    assert!(
        config_mod.contains("PRIVACY PATCH: Do not upgrade to Live"),
        "Expected web search turn resolver to be privacy patched"
    );

    let turn_metadata =
        fs::read_to_string(workspace.path().join("core/src/turn_metadata.rs")).unwrap();
    assert!(
        turn_metadata.contains("Disable per-turn metadata headers"),
        "Expected turn metadata header builder to be disabled"
    );

    let tools_registry =
        fs::read_to_string(workspace.path().join("core/src/tools/registry.rs")).unwrap();
    assert!(
        tools_registry.contains("let metric_tags: [(&str, &str); 0] = [];"),
        "Expected tool metric tags to be emptied (drop sandbox tags)"
    );
}

#[test]
fn test_privacy_patches_v0_99_alpha16_apply() {
    let workspace = TempDir::new().unwrap();
    write_alpha16_workspace(&workspace);

    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy-v0.99-alpha14-alpha20.toml");
    let config = load_from_path(&patch_file).expect("Failed to load alpha14-alpha20 privacy patch");

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
        "Expected config to default metrics_exporter to None when missing"
    );
    assert!(
        config_mod.contains("assert_eq!(config.otel.metrics_exporter, OtelExporterKind::None)"),
        "Expected metrics exporter default test to be updated"
    );

    let tools_registry =
        fs::read_to_string(workspace.path().join("core/src/tools/registry.rs")).unwrap();
    assert!(
        tools_registry.contains("let metric_tags: [(&str, &str); 0] = [];"),
        "Expected tool metric tags to be emptied (drop sandbox tags)"
    );
}

#[test]
fn test_privacy_patches_v0_99_alpha23_apply() {
    let workspace = TempDir::new().unwrap();
    write_alpha23_workspace(&workspace);

    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy-v0.99-alpha23.toml");
    let config = load_from_path(&patch_file).expect("Failed to load alpha21+ privacy patch");

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
        "Expected turn metadata header builder to be disabled (sandbox variant)"
    );

    let config_mod = fs::read_to_string(workspace.path().join("core/src/config/mod.rs")).unwrap();
    assert!(
        config_mod.contains("t.metrics_exporter.unwrap_or(OtelExporterKind::None)"),
        "Expected config to default metrics_exporter to None when missing"
    );

    let tools_registry =
        fs::read_to_string(workspace.path().join("core/src/tools/registry.rs")).unwrap();
    assert!(
        tools_registry.contains("let metric_tags: [(&str, &str); 0] = [];"),
        "Expected tool metric tags to be emptied (drop sandbox tags)"
    );
}

#[test]
fn test_privacy_patches_v0_99_version_gating() {
    let workspace = TempDir::new().unwrap();
    write_alpha23_workspace(&workspace);

    let alpha10_13 = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy-v0.99-alpha1-alpha22.toml");
    let config = load_from_path(&alpha10_13).expect("Failed to load alpha10-alpha13 privacy patch");

    let results = apply_patches(&config, workspace.path(), "0.99.0-alpha.23");
    assert!(
        results
            .iter()
            .all(|(_, r)| matches!(r, Ok(PatchResult::SkippedVersion { .. }))),
        "Expected all alpha10-alpha13 patches to be skipped on alpha.23"
    );
}
