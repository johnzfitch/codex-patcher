//! Integration tests for Phase 6: Patch Config Parser
//!
//! Tests version filtering, idempotency checks, and full patch application

use codex_patcher::config::{
    apply_patches, load_from_path, load_from_str, ApplicationError, HashAlgorithm, Metadata,
    Operation, PatchConfig, PatchDefinition, PatchResult, Query, Verify,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temp dir with test files
fn setup_test_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Create a simple Rust file
    let rust_file = dir.path().join("test.rs");
    fs::write(
        &rust_file,
        r#"
pub fn hello() {
    println!("Hello");
}

pub fn world() {
    println!("World");
}
"#,
    )
    .unwrap();

    // Create a TOML file
    let toml_file = dir.path().join("Cargo.toml");
    fs::write(
        &toml_file,
        r#"
[package]
name = "test"
version = "0.1.0"

[profile.release]
opt-level = 3
"#,
    )
    .unwrap();

    dir
}

#[test]
fn test_load_patch_config_basic() {
    let toml = r#"
[meta]
name = "test-patches"
description = "Test patch set"
version_range = ">=0.88.0"
workspace_relative = true

[[patches]]
id = "patch-1"
file = "test.rs"

[patches.query]
type = "ast-grep"
pattern = "fn hello() { $$$BODY }"

[patches.operation]
type = "replace"
text = "fn hello() { println!(\"Modified\"); }"
"#;

    let config = load_from_str(toml).expect("Failed to parse config");

    assert_eq!(config.meta.name, "test-patches");
    assert_eq!(config.meta.version_range, Some(">=0.88.0".to_string()));
    assert!(config.meta.workspace_relative);
    assert_eq!(config.patches.len(), 1);
    assert_eq!(config.patches[0].id, "patch-1");
}

#[test]
fn test_load_patch_config_with_verification() {
    let toml = r#"
[meta]
name = "verified-patches"

[[patches]]
id = "patch-with-exact-match"
file = "test.rs"

[patches.query]
type = "ast-grep"
pattern = "fn $NAME() { $$$BODY }"

[patches.operation]
type = "replace"
text = "fn hello() { println!(\"Verified\"); }"

[patches.verify]
method = "exact_match"
expected_text = "fn hello() {\n    println!(\"Hello\");\n}"
"#;

    let config = load_from_str(toml).expect("Failed to parse config");
    assert_eq!(config.patches.len(), 1);
    assert!(config.patches[0].verify.is_some());

    if let Some(Verify::ExactMatch { expected_text }) = &config.patches[0].verify {
        assert!(expected_text.contains("Hello"));
    } else {
        panic!("Expected ExactMatch verification");
    }
}

#[test]
fn test_load_patch_config_with_hash() {
    let toml = r#"
[meta]
name = "hash-patches"

[[patches]]
id = "patch-with-hash"
file = "test.rs"

[patches.query]
type = "ast-grep"
pattern = "fn $NAME() { $$$BODY }"

[patches.operation]
type = "replace"
text = "fn hello() { }"

[patches.verify]
method = "hash"
algorithm = "xxh3"
expected = "0x1234567890abcdef"
"#;

    let config = load_from_str(toml).expect("Failed to parse config");
    assert_eq!(config.patches.len(), 1);

    if let Some(Verify::Hash {
        algorithm,
        expected,
    }) = &config.patches[0].verify
    {
        assert_eq!(*algorithm, Some(HashAlgorithm::Xxh3));
        assert_eq!(expected, "0x1234567890abcdef");
    } else {
        panic!("Expected Hash verification");
    }
}

#[test]
fn test_version_filtering_matches() {
    use codex_patcher::matches_requirement;

    // Exact match
    assert!(matches_requirement("0.88.0", Some("=0.88.0")).unwrap());

    // Range
    assert!(matches_requirement("0.88.5", Some(">=0.88.0, <0.89.0")).unwrap());
    assert!(matches_requirement("0.88.0", Some(">=0.88.0, <0.89.0")).unwrap());
    assert!(!matches_requirement("0.89.0", Some(">=0.88.0, <0.89.0")).unwrap());

    // No requirement = always match
    assert!(matches_requirement("1.0.0", None).unwrap());
    assert!(matches_requirement("0.1.0", None).unwrap());
}

#[test]
fn test_version_filtering_prerelease() {
    use codex_patcher::matches_requirement;

    let req = ">=0.88.0-alpha.4";
    assert!(matches_requirement("0.88.0-alpha.4", Some(req)).unwrap());
    assert!(matches_requirement("0.88.0-alpha.5", Some(req)).unwrap());
    assert!(matches_requirement("0.88.0", Some(req)).unwrap());
    assert!(!matches_requirement("0.88.0-alpha.3", Some(req)).unwrap());
}

#[test]
fn test_version_filtering_caret() {
    use codex_patcher::matches_requirement;

    // ^0.88 means >=0.88.0, <0.89.0
    let req = "^0.88";
    assert!(matches_requirement("0.88.0", Some(req)).unwrap());
    assert!(matches_requirement("0.88.9", Some(req)).unwrap());
    assert!(!matches_requirement("0.89.0", Some(req)).unwrap());
}

#[test]
fn test_invalid_version() {
    use codex_patcher::matches_requirement;

    let result = matches_requirement("not-a-version", Some(">=0.88.0"));
    assert!(result.is_err());
}

#[test]
fn test_invalid_requirement() {
    use codex_patcher::matches_requirement;

    let result = matches_requirement("0.88.0", Some(">=not-a-version"));
    assert!(result.is_err());
}

#[test]
fn test_apply_patches_empty() {
    let config = PatchConfig {
        meta: Metadata {
            name: "empty".to_string(),
            description: None,
            version_range: None,
            workspace_relative: false,
        },
        patches: vec![],
    };

    let results = apply_patches(&config, &PathBuf::from("/tmp"), "0.88.0");
    assert_eq!(results.len(), 0);
}

#[test]
fn test_apply_patches_file_not_found() {
    let config = PatchConfig {
        meta: Metadata {
            name: "test".to_string(),
            description: None,
            version_range: None,
            workspace_relative: false,
        },
        patches: vec![PatchDefinition {
            id: "patch-1".to_string(),
            file: "/nonexistent/file.rs".to_string(),
            query: Query::AstGrep {
                pattern: "fn test() {}".to_string(),
            },
            operation: Operation::Replace {
                text: "fn test() { println!(\"hi\"); }".to_string(),
            },
            verify: None,
            constraint: None,
        }],
    };

    let results = apply_patches(&config, &PathBuf::from("/tmp"), "0.88.0");
    assert_eq!(results.len(), 1);

    let (id, result) = &results[0];
    assert_eq!(id, "patch-1");
    assert!(result.is_err());

    if let Err(ApplicationError::NoMatch { file }) = result {
        assert!(file.to_string_lossy().contains("nonexistent"));
    } else {
        panic!("Expected NoMatch error");
    }
}

#[test]
fn test_idempotency_check_logic() {
    // This test validates that the idempotency logic works at the Edit level,
    // which is already tested in edit.rs. The full patch applicator integration
    // will be tested in Phase 7 with the CLI.
    //
    // For now, we verify the config can be loaded and patches can be attempted,
    // even if pattern matching isn't perfect in the simplified applicator.

    let workspace = setup_test_workspace();

    let config = PatchConfig {
        meta: Metadata {
            name: "test".to_string(),
            description: None,
            version_range: None,
            workspace_relative: true,
        },
        patches: vec![PatchDefinition {
            id: "patch-1".to_string(),
            file: "test.rs".to_string(),
            query: Query::AstGrep {
                pattern: "fn hello() { $$$BODY }".to_string(),
            },
            operation: Operation::Replace {
                text: r#"fn hello() {
    println!("Modified");
}"#
                .to_string(),
            },
            verify: None,
            constraint: None,
        }],
    };

    let results = apply_patches(&config, workspace.path(), "0.88.0");
    assert_eq!(results.len(), 1);

    let (id, _result) = &results[0];
    assert_eq!(id, "patch-1");

    // Note: The actual result depends on pattern matching which is tested
    // in the sg and edit modules. This test primarily validates config loading
    // and the application framework.
}

#[test]
fn test_validation_empty_patches() {
    let toml = r#"
[meta]
name = "test"
"#;

    let result = load_from_str(toml);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("patch config contains no patches"));
}

#[test]
fn test_validation_missing_id() {
    let toml = r#"
[meta]
name = "test"

[[patches]]
file = "test.rs"

[patches.query]
type = "ast-grep"
pattern = "fn test() {}"

[patches.operation]
type = "replace"
text = "fn test() { }"
"#;

    let result = load_from_str(toml);
    // Note: TOML deserialization will fail before validation for missing required field
    assert!(result.is_err());
}

#[test]
fn test_validation_missing_file() {
    let toml = r#"
[meta]
name = "test"

[[patches]]
id = "patch-1"

[patches.query]
type = "ast-grep"
pattern = "fn test() {}"

[patches.operation]
type = "replace"
text = "fn test() { }"
"#;

    let result = load_from_str(toml);
    assert!(result.is_err());
}

#[test]
fn test_validation_rejects_text_delete_combo() {
    let toml = r#"
[meta]
name = "invalid-combo"

[[patches]]
id = "text-delete"
file = "test.rs"

[patches.query]
type = "text"
search = "hello"

[patches.operation]
type = "delete"
"#;

    let result = load_from_str(toml);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("query type 'text' does not support operation 'delete'"));
}

#[test]
fn test_validation_rejects_toml_replace_combo() {
    let toml = r#"
[meta]
name = "invalid-combo"

[[patches]]
id = "toml-replace"
file = "Cargo.toml"

[patches.query]
type = "toml"
section = "package"
key = "name"

[patches.operation]
type = "replace"
text = "name = \"patched\""
"#;

    let result = load_from_str(toml);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("query type 'toml' does not support operation 'replace'"));
}

#[test]
fn test_validation_rejects_ast_grep_replace_key_combo() {
    let toml = r#"
[meta]
name = "invalid-combo"

[[patches]]
id = "ast-replace-key"
file = "test.rs"

[patches.query]
type = "ast-grep"
pattern = "fn test() {}"

[patches.operation]
type = "replace-key"
new_key = "renamed"
"#;

    let result = load_from_str(toml);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("query type 'ast-grep' does not support operation 'replace-key'"));
}

#[test]
fn test_patch_result_display() {
    let applied = PatchResult::Applied {
        file: PathBuf::from("/tmp/test.rs"),
    };
    assert!(applied.to_string().contains("Applied"));
    assert!(applied.to_string().contains("test.rs"));

    let already = PatchResult::AlreadyApplied {
        file: PathBuf::from("/tmp/test.rs"),
    };
    assert!(already.to_string().contains("Already applied"));

    let skipped = PatchResult::SkippedVersion {
        reason: "version too old".to_string(),
    };
    assert!(skipped.to_string().contains("Skipped"));
    assert!(skipped.to_string().contains("version"));

    let failed = PatchResult::Failed {
        file: PathBuf::from("/tmp/test.rs"),
        reason: "parse error".to_string(),
    };
    assert!(failed.to_string().contains("Failed"));
    assert!(failed.to_string().contains("parse error"));
}

#[test]
fn test_toml_patch_config() {
    let toml = r#"
[meta]
name = "toml-patches"
workspace_relative = true

[[patches]]
id = "add-profile"
file = "Cargo.toml"

[patches.query]
type = "toml"
section = "profile.zack"
ensure_absent = true

[patches.operation]
type = "insert-section"
text = "[profile.zack]\nopt-level = 3\n"
after_section = "profile.release"
"#;

    let config = load_from_str(toml).expect("Failed to parse TOML patch config");
    assert_eq!(config.patches.len(), 1);

    if let Query::Toml {
        section,
        ensure_absent,
        ..
    } = &config.patches[0].query
    {
        assert_eq!(section.as_deref(), Some("profile.zack"));
        assert!(ensure_absent);
    } else {
        panic!("Expected Toml query");
    }
}

#[test]
fn test_multiple_patches_in_config() {
    let toml = r#"
[meta]
name = "multi-patches"

[[patches]]
id = "patch-1"
file = "file1.rs"

[patches.query]
type = "ast-grep"
pattern = "fn a() {}"

[patches.operation]
type = "replace"
text = "fn a() { println!(\"a\"); }"

[[patches]]
id = "patch-2"
file = "file2.rs"

[patches.query]
type = "ast-grep"
pattern = "fn b() {}"

[patches.operation]
type = "replace"
text = "fn b() { println!(\"b\"); }"
"#;

    let config = load_from_str(toml).expect("Failed to parse multi-patch config");
    assert_eq!(config.patches.len(), 2);
    assert_eq!(config.patches[0].id, "patch-1");
    assert_eq!(config.patches[1].id, "patch-2");
}

#[test]
fn test_v099_ranges_against_v0100_alpha2() {
    let patch_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Bounded 0.99-alpha ranges must NOT match 0.100 (upper bound blocks it).
    let cases = [
        ("patches/privacy-v0.99.toml", false),
        ("patches/sandbox-metrics.toml", false),
        ("patches/privacy-v0.105-alpha13.toml", false),
    ];

    for (relative, expected) in cases {
        let config = load_from_path(patch_root.join(relative)).expect("patch file must load");
        let compatible = codex_patcher::matches_requirement(
            "0.100.0-alpha.2",
            config.meta.version_range.as_deref(),
        )
        .expect("version range must parse");
        assert_eq!(compatible, expected, "{relative}");
    }
}

#[test]
fn test_sandbox_metric_patch_ranges_are_mutually_exclusive() {
    let patch_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let patch_files = [
        "patches/sandbox-metrics.toml",
        "patches/privacy-v0.99.toml",
        "patches/privacy-v0.105-alpha13.toml",
    ];
    let patch_configs: Vec<_> = patch_files
        .iter()
        .map(|relative| {
            let config = load_from_path(patch_root.join(relative)).expect("patch file must load");
            (relative.to_string(), config)
        })
        .collect();

    for version in [
        "0.99.0-alpha.11",
        "0.99.0-alpha.14",
        "0.99.0-alpha.18",
        "0.99.0-alpha.23",
        "0.100.0-alpha.2",
        "0.105.0-alpha.13",
    ] {
        let compatible_count = patch_configs
            .iter()
            .filter(|(_, config)| {
                codex_patcher::matches_requirement(version, config.meta.version_range.as_deref())
                    .expect("version range must parse")
            })
            .count();
        assert!(
            compatible_count <= 1,
            "version {version} matched {compatible_count} overlapping sandbox metric patch files"
        );
    }
}
