//! Integration tests for privacy patches
//!
//! Tests that privacy.toml patches correctly disable Statsig telemetry

use codex_patcher::config::{apply_patches, load_from_path, PatchResult};
use std::fs;
use tempfile::TempDir;

/// Create a mock Codex workspace with the files targeted by privacy patches
fn setup_mock_codex_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Create directory structure
    fs::create_dir_all(dir.path().join("otel/src")).unwrap();
    fs::create_dir_all(dir.path().join("core/src/config")).unwrap();

    // Create otel/src/config.rs with Statsig implementation
    let otel_config = dir.path().join("otel/src/config.rs");
    fs::write(
        &otel_config,
        r#"
use std::collections::HashMap;

pub(crate) const STATSIG_OTLP_HTTP_ENDPOINT: &str = "https://ab.chatgpt.com/otlp/v1/metrics";
pub(crate) const STATSIG_API_KEY_HEADER: &str = "statsig-api-key";
pub(crate) const STATSIG_API_KEY: &str = "client-MkRuleRQBd6qakfnDYqJVR9JuXcY57Ljly3vi5JVUIO";

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
        OtelExporter::Statsig => {
            if cfg!(test) || cfg!(feature = "disable-default-metrics-exporter") {
                return OtelExporter::None;
            }

            OtelExporter::OtlpHttp {
                endpoint: STATSIG_OTLP_HTTP_ENDPOINT.to_string(),
                headers: HashMap::from([
                    (STATSIG_API_KEY_HEADER.to_string(), STATSIG_API_KEY.to_string()),
                ]),
            }
        }
        _ => exporter.clone(),
    }
}
"#,
    )
    .unwrap();

    // Create core/src/config/types.rs with Statsig default
    let types = dir.path().join("core/src/config/types.rs");
    fs::write(
        &types,
        r#"
pub enum OtelExporterKind {
    None,
    Statsig,
    OtlpHttp,
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

    dir
}

#[test]
fn test_privacy_patches_apply() {
    let workspace = setup_mock_codex_workspace();
    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy.toml");

    // Load and apply patches
    let config = load_from_path(&patch_file).expect("Failed to load privacy.toml");
    let results = apply_patches(&config, workspace.path(), "0.88.0");

    // Check that patches applied successfully
    let mut applied = 0;
    let mut already_applied = 0;

    for (patch_id, result) in results {
        match result {
            Ok(PatchResult::Applied { .. }) => {
                println!("✓ {}: Applied", patch_id);
                applied += 1;
            }
            Ok(PatchResult::AlreadyApplied { .. }) => {
                println!("⊙ {}: Already applied", patch_id);
                already_applied += 1;
            }
            Ok(PatchResult::SkippedVersion { reason }) => {
                println!("⊘ {}: Skipped - {}", patch_id, reason);
            }
            Ok(PatchResult::Failed { reason, .. }) => {
                panic!("✗ {}: Failed - {}", patch_id, reason);
            }
            Err(e) => {
                panic!("✗ {}: Failed - {}", patch_id, e);
            }
        }
    }

    assert!(applied > 0 || already_applied > 0, "No patches were applied");

    // Verify otel/src/config.rs changes
    let otel_config = fs::read_to_string(workspace.path().join("otel/src/config.rs")).unwrap();

    // Should have removed Statsig constants
    assert!(
        !otel_config.contains("STATSIG_OTLP_HTTP_ENDPOINT")
            || otel_config.contains("// PRIVACY PATCH: Statsig endpoint removed"),
        "Statsig endpoint should be removed or commented"
    );

    // Should have simplified resolve_exporter
    assert!(
        otel_config.contains("OtelExporter::None")
            && otel_config.contains("PRIVACY PATCH"),
        "resolve_exporter should return None with privacy patch comment"
    );

    // Verify core/src/config/types.rs changes
    let types = fs::read_to_string(workspace.path().join("core/src/config/types.rs")).unwrap();

    // Check Default impl has metrics_exporter: None
    assert!(
        types.contains("metrics_exporter: OtelExporterKind::None"),
        "metrics_exporter should be None in Default impl"
    );
}

#[test]
fn test_privacy_patches_idempotent() {
    let workspace = setup_mock_codex_workspace();
    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy.toml");

    let config = load_from_path(&patch_file).expect("Failed to load privacy.toml");

    // Apply patches first time
    let results1 = apply_patches(&config, workspace.path(), "0.88.0");
    let successful1 = results1
        .iter()
        .filter(|(_, r)| matches!(r, Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. })))
        .count();

    println!("First run: {} successful patches", successful1);

    // Apply patches second time
    let results2 = apply_patches(&config, workspace.path(), "0.88.0");
    let successful2 = results2
        .iter()
        .filter(|(_, r)| matches!(r, Ok(PatchResult::Applied { .. }) | Ok(PatchResult::AlreadyApplied { .. })))
        .count();

    println!("Second run: {} successful patches", successful2);

    // Second run should have same number of successful patches (idempotent)
    assert_eq!(
        successful1, successful2,
        "Patches should be idempotent: same number of successful patches on re-run"
    );

    // Most patches on second run should be already applied
    let already_applied2 = results2
        .iter()
        .filter(|(_, r)| matches!(r, Ok(PatchResult::AlreadyApplied { .. })))
        .count();

    println!("Second run: {} already applied", already_applied2);

    // At least some patches should report as already applied
    assert!(already_applied2 > 0, "At least some patches should report as already applied on second run");
}

#[test]
fn test_privacy_patches_no_telemetry_strings() {
    let workspace = setup_mock_codex_workspace();
    let patch_file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("patches/privacy.toml");

    let config = load_from_path(&patch_file).expect("Failed to load privacy.toml");
    apply_patches(&config, workspace.path(), "0.88.0");

    // Read patched file
    let otel_config = fs::read_to_string(workspace.path().join("otel/src/config.rs")).unwrap();

    // Should not contain live telemetry strings (might be in comments)
    let has_live_endpoint = otel_config
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| line.contains("ab.chatgpt.com"));

    let has_live_api_key = otel_config
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| line.contains("MkRuleRQBd6"));

    assert!(!has_live_endpoint, "Live ab.chatgpt.com endpoint should not exist");
    assert!(!has_live_api_key, "Live API key should not exist");
}
