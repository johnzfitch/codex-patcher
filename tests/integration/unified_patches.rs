//! Integration tests for unified patch system with per-patch version constraints
//! and fuzzy matching fallback.

use codex_patcher::config::schema::{Metadata, Operation, PatchConfig, PatchDefinition, Query};
use codex_patcher::config::{apply_patches, PatchResult};
use std::fs;
use tempfile::TempDir;

/// Create a minimal workspace with a single file for testing
fn create_workspace_with_file(relative_path: &str, content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(relative_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file_path, content).unwrap();
    dir
}

/// Create a PatchConfig programmatically for testing
fn make_config(patches: Vec<PatchDefinition>) -> PatchConfig {
    PatchConfig {
        meta: Metadata {
            name: "test-config".to_string(),
            description: Some("Test configuration".to_string()),
            version_range: None,
            workspace_relative: true,
        },
        patches,
    }
}

/// Create a text replacement patch definition
fn text_patch(
    id: &str,
    file: &str,
    search: &str,
    replace: &str,
    version: Option<&str>,
    fuzzy_threshold: Option<f64>,
) -> PatchDefinition {
    PatchDefinition {
        id: id.to_string(),
        file: file.to_string(),
        query: Query::Text {
            search: search.to_string(),
            fuzzy_threshold,
            fuzzy_expansion: None,
        },
        operation: Operation::Replace {
            text: replace.to_string(),
        },
        verify: None,
        constraint: None,
        version: version.map(|s| s.to_string()),
    }
}

// =============================================================================
// Per-patch version constraint tests
// =============================================================================

#[test]
fn test_patch_skipped_when_version_constraint_not_met() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}");
    let config = make_config(vec![text_patch(
        "version-gated",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        Some(">=1.0.0"), // Workspace version will be 0.99.0
        None,
    )]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    assert_eq!(results.len(), 1);

    let (id, result) = &results[0];
    assert_eq!(id, "version-gated");
    assert!(
        matches!(result, Ok(PatchResult::SkippedVersion { .. })),
        "Expected patch to be skipped due to version constraint, got {:?}",
        result
    );
}

#[test]
fn test_patch_applies_when_version_constraint_met() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}");
    let config = make_config(vec![text_patch(
        "version-gated",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        Some(">=0.99.0"),
        None,
    )]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    assert_eq!(results.len(), 1);

    let (id, result) = &results[0];
    assert_eq!(id, "version-gated");
    assert!(
        matches!(result, Ok(PatchResult::Applied { .. })),
        "Expected patch to apply, got {:?}",
        result
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("/* patched */"));
}

#[test]
fn test_patch_without_version_applies_to_all_versions() {
    let _workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}");
    let config = make_config(vec![text_patch(
        "no-version",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        None, // No version constraint
        None,
    )]);

    // Should apply to any version
    for version in ["0.1.0", "0.99.0", "1.0.0", "2.5.0"] {
        let ws = create_workspace_with_file("src/lib.rs", "fn hello() {}");
        let results = apply_patches(&config, ws.path(), version);

        let (_, result) = &results[0];
        assert!(
            matches!(result, Ok(PatchResult::Applied { .. })),
            "Expected patch to apply for version {}, got {:?}",
            version,
            result
        );
    }
}

// =============================================================================
// Fuzzy matching tests
// =============================================================================

#[test]
fn test_fuzzy_match_fallback_on_similar_code() {
    // Code has slight difference (extra space, different formatting)
    let workspace = create_workspace_with_file("src/lib.rs", "fn  hello()  {\n    // comment\n}");
    let config = make_config(vec![text_patch(
        "fuzzy-patch",
        "src/lib.rs",
        "fn hello() {\n    // comment\n}", // Exact won't match (different spacing)
        "fn hello() { /* patched */ }",
        None,
        Some(0.8), // Enable fuzzy with threshold
    )]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    let (_, result) = &results[0];

    assert!(
        matches!(result, Ok(PatchResult::Applied { .. })),
        "Expected fuzzy match to apply, got {:?}",
        result
    );
}

#[test]
fn test_fuzzy_match_respects_threshold() {
    // Code is very different - should NOT fuzzy match at high threshold
    let workspace = create_workspace_with_file(
        "src/lib.rs",
        "fn completely_different_function() { return 42; }",
    );
    let config = make_config(vec![text_patch(
        "no-fuzzy",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        None,
        Some(0.95), // High threshold - should not match
    )]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    let (_, result) = &results[0];

    assert!(
        result.is_err(),
        "Expected no match due to high threshold, got {:?}",
        result
    );
}

#[test]
fn test_exact_match_preferred_over_fuzzy() {
    // Exact match exists - should use it, not fuzzy
    let workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}");
    let config = make_config(vec![text_patch(
        "exact-match",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        None,
        Some(0.8), // Fuzzy enabled but shouldn't be needed
    )]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    let (_, result) = &results[0];

    assert!(
        matches!(result, Ok(PatchResult::Applied { .. })),
        "Expected exact match to apply, got {:?}",
        result
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("/* patched */"));
}

// =============================================================================
// Idempotency tests
// =============================================================================

#[test]
fn test_patches_idempotent_after_application() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}");
    let config = make_config(vec![text_patch(
        "idempotent",
        "src/lib.rs",
        "fn hello() {}",
        "fn hello() { /* patched */ }",
        None,
        None,
    )]);

    // First application
    let results1 = apply_patches(&config, workspace.path(), "0.99.0");
    assert!(matches!(results1[0].1, Ok(PatchResult::Applied { .. })));

    // Second application - should be idempotent
    let results2 = apply_patches(&config, workspace.path(), "0.99.0");
    assert!(
        matches!(results2[0].1, Ok(PatchResult::AlreadyApplied { .. })),
        "Expected AlreadyApplied on second run, got {:?}",
        results2[0].1
    );

    // Content should still be correct
    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("/* patched */"));
    assert_eq!(content.matches("/* patched */").count(), 1);
}

// =============================================================================
// Mixed results tests (realistic scenarios)
// =============================================================================

#[test]
fn test_unified_patch_mixed_results() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn hello() {}\nfn world() {}");

    let config = make_config(vec![
        // This one should apply
        text_patch(
            "applies",
            "src/lib.rs",
            "fn hello() {}",
            "fn hello() { /* applied */ }",
            None,
            None,
        ),
        // This one should be skipped (version constraint)
        text_patch(
            "skipped",
            "src/lib.rs",
            "fn world() {}",
            "fn world() { /* skipped */ }",
            Some(">=1.0.0"),
            None,
        ),
        // This one should fail (file not found)
        text_patch(
            "fails",
            "src/nonexistent.rs",
            "anything",
            "replaced",
            None,
            None,
        ),
    ]);

    let results = apply_patches(&config, workspace.path(), "0.99.0");
    assert_eq!(results.len(), 3);

    let results_map: std::collections::HashMap<_, _> = results.into_iter().collect();

    assert!(
        matches!(results_map["applies"], Ok(PatchResult::Applied { .. })),
        "Expected 'applies' to succeed"
    );
    assert!(
        matches!(
            results_map["skipped"],
            Ok(PatchResult::SkippedVersion { .. })
        ),
        "Expected 'skipped' to be version-gated"
    );
    assert!(
        results_map["fails"].is_err(),
        "Expected 'fails' to error on missing file"
    );

    // Verify only the applied patch modified the file
    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("/* applied */"));
    assert!(!content.contains("/* skipped */"));
}

// =============================================================================
// Elastic fuzzy window (fuzzy_expansion) integration tests
// =============================================================================

/// Build a patch that uses elastic fuzzy matching.
fn elastic_patch(
    id: &str,
    file: &str,
    search: &str,
    replace: &str,
    fuzzy_threshold: f64,
    fuzzy_expansion: usize,
) -> PatchDefinition {
    PatchDefinition {
        id: id.to_string(),
        file: file.to_string(),
        query: Query::Text {
            search: search.to_string(),
            fuzzy_threshold: Some(fuzzy_threshold),
            fuzzy_expansion: Some(fuzzy_expansion),
        },
        operation: Operation::Replace {
            text: replace.to_string(),
        },
        verify: None,
        constraint: None,
        version: None,
    }
}

#[test]
fn test_elastic_matches_when_lines_inserted_inside_needle_span() {
    // Simulates a common version-bump failure: one short line was inserted
    // between anchor lines that the needle spans.
    //
    // Verified Levenshtein scores for this exact scenario:
    //   fixed 6-line window [0..6] = 0.785
    //   elastic 7-line window [0..7] = 0.798
    //   Both exceed threshold=0.75, confirming elastic finds the match.
    let original = "fn handle(req: &Request) -> Response {\n    validate(req);\n    let ctx = prepare(req);\n    tracing::info!(\"context ready\");\n    let result = execute(&ctx);\n    build_response(result)\n}";
    let needle = "fn handle(req: &Request) -> Response {\n    validate(req);\n    let ctx = prepare(req);\n    let result = execute(&ctx);\n    build_response(result)\n}";
    let replacement = "fn handle(req: &Request) -> Response {\n    validate(req);\n    let ctx = prepare(req);\n    let result = execute(&ctx);\n    build_response(result) // PATCHED\n}";

    let workspace = create_workspace_with_file("src/lib.rs", original);
    let config = make_config(vec![elastic_patch(
        "elastic-insertion",
        "src/lib.rs",
        needle,
        replacement,
        0.75,
        2,
    )]);

    let results = apply_patches(&config, workspace.path(), "0.112.0");
    let (_, result) = &results[0];
    assert!(
        matches!(result, Ok(PatchResult::Applied { .. })),
        "elastic patch should apply when one line inserted in needle span, got {result:?}"
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(
        content.contains("// PATCHED"),
        "replacement text should be present"
    );
}

#[test]
fn test_elastic_idempotent_after_application() {
    // Verified scores: fixed w3 best=0.562, elastic w4[0..4]=0.628.
    // At threshold=0.60: fixed → None (0.562 < 0.60), elastic → Some (0.628 > 0.60).
    let original = "fn foo() {\n    // inserted\n    let x = 1;\n}";
    let needle = "fn foo() {\n    let x = 1;\n}";
    let replacement = "fn foo() {\n    let x = 1; // patched\n}";

    let workspace = create_workspace_with_file("src/lib.rs", original);
    let config = make_config(vec![elastic_patch(
        "elastic-idempotent",
        "src/lib.rs",
        needle,
        replacement,
        0.60,
        1,
    )]);

    let results1 = apply_patches(&config, workspace.path(), "0.99.0");
    assert!(
        matches!(results1[0].1, Ok(PatchResult::Applied { .. })),
        "first application should succeed, got {:?}",
        results1[0].1
    );

    let results2 = apply_patches(&config, workspace.path(), "0.99.0");
    assert!(
        matches!(results2[0].1, Ok(PatchResult::AlreadyApplied { .. })),
        "second application should be idempotent, got {:?}",
        results2[0].1
    );
}

// =============================================================================
// Schema validation tests for fuzzy_expansion
// =============================================================================

#[test]
fn test_validate_rejects_fuzzy_expansion_over_200() {
    let config = make_config(vec![PatchDefinition {
        id: "bad-expansion".to_string(),
        file: "src/lib.rs".to_string(),
        query: Query::Text {
            search: "fn foo() {}".to_string(),
            fuzzy_threshold: Some(0.85),
            fuzzy_expansion: Some(201),
        },
        operation: Operation::Replace {
            text: "fn foo() { /* patched */ }".to_string(),
        },
        verify: None,
        constraint: None,
        version: None,
    }]);

    let err = config.validate().expect_err("should fail validation");
    let msg = err.to_string();
    assert!(
        msg.contains("fuzzy_expansion"),
        "error should mention fuzzy_expansion, got: {msg}"
    );
    assert!(
        msg.contains("201"),
        "error should include the bad value, got: {msg}"
    );
}

#[test]
fn test_validate_accepts_fuzzy_expansion_at_boundary() {
    let config = make_config(vec![PatchDefinition {
        id: "max-expansion".to_string(),
        file: "src/lib.rs".to_string(),
        query: Query::Text {
            search: "fn foo() {}".to_string(),
            fuzzy_threshold: Some(0.85),
            fuzzy_expansion: Some(200),
        },
        operation: Operation::Replace {
            text: "fn foo() { /* patched */ }".to_string(),
        },
        verify: None,
        constraint: None,
        version: None,
    }]);

    assert!(
        config.validate().is_ok(),
        "fuzzy_expansion = 200 should pass validation"
    );
}

#[test]
fn test_validate_accepts_fuzzy_expansion_without_explicit_threshold() {
    // fuzzy_expansion without fuzzy_threshold is valid; applicator defaults threshold to 0.85.
    let config = make_config(vec![PatchDefinition {
        id: "expansion-no-threshold".to_string(),
        file: "src/lib.rs".to_string(),
        query: Query::Text {
            search: "fn foo() {}".to_string(),
            fuzzy_threshold: None,
            fuzzy_expansion: Some(10),
        },
        operation: Operation::Replace {
            text: "fn foo() { /* patched */ }".to_string(),
        },
        verify: None,
        constraint: None,
        version: None,
    }]);

    assert!(
        config.validate().is_ok(),
        "fuzzy_expansion without explicit fuzzy_threshold should pass validation"
    );
}
