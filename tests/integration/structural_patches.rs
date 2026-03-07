//! Integration tests for structural patch application — ast-grep and tree-sitter.
//!
//! Mirrors the structure of `unified_patches.rs` (text-based patches), providing
//! equivalent coverage for `Query::AstGrep` and `Query::TreeSitter` through the
//! full `apply_patches` pipeline: parse pattern → compute edit → batch apply.

use codex_patcher::config::schema::{Metadata, Operation, PatchConfig, PatchDefinition, Query};
use codex_patcher::config::{apply_patches, ApplicationError, PatchResult};
use std::fs;
use tempfile::TempDir;

// =============================================================================
// Shared helpers (mirrors unified_patches.rs)
// =============================================================================

fn create_workspace_with_file(relative_path: &str, content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(relative_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file_path, content).unwrap();
    dir
}

fn make_config(patches: Vec<PatchDefinition>) -> PatchConfig {
    PatchConfig {
        meta: Metadata {
            name: "structural-test".to_string(),
            description: None,
            version_range: None,
            workspace_relative: true,
        },
        patches,
    }
}

fn ast_grep_patch(id: &str, file: &str, pattern: &str, operation: Operation) -> PatchDefinition {
    PatchDefinition {
        id: id.to_string(),
        file: file.to_string(),
        query: Query::AstGrep {
            pattern: pattern.to_string(),
        },
        operation,
        verify: None,
        constraint: None,
        version: None,
    }
}

fn tree_sitter_patch(id: &str, file: &str, pattern: &str, operation: Operation) -> PatchDefinition {
    PatchDefinition {
        id: id.to_string(),
        file: file.to_string(),
        query: Query::TreeSitter {
            pattern: pattern.to_string(),
        },
        operation,
        verify: None,
        constraint: None,
        version: None,
    }
}

// =============================================================================
// AST-grep structural matching
// =============================================================================

#[test]
fn ast_grep_replaces_function_body() {
    let workspace =
        create_workspace_with_file("src/lib.rs", r#"fn greet() { println!("hello"); }"#);
    let config = make_config(vec![ast_grep_patch(
        "greet",
        "src/lib.rs",
        "fn greet() { $$$ }",
        Operation::Replace {
            text: r#"fn greet() { println!("world"); }"#.to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ast-grep function replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("world"), "replacement text should appear");
    assert!(!content.contains("hello"), "original text should be gone");
}

#[test]
fn ast_grep_delete_removes_function() {
    let workspace = create_workspace_with_file(
        "src/lib.rs",
        "fn dead() { unimplemented!(); }\n\nfn live() {}",
    );
    let config = make_config(vec![ast_grep_patch(
        "del",
        "src/lib.rs",
        "fn dead() { $$$ }",
        Operation::Delete {
            insert_comment: None,
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ast-grep delete should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(!content.contains("dead"), "deleted function should be gone");
    assert!(content.contains("live"), "surviving function must remain");
}

#[test]
fn ast_grep_delete_with_comment_marker() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn removed() { unimplemented!(); }");
    let config = make_config(vec![ast_grep_patch(
        "del-comment",
        "src/lib.rs",
        "fn removed() { $$$ }",
        Operation::Delete {
            insert_comment: Some("// fn removed — deleted by patch".to_string()),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ast-grep delete-with-comment should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(
        content.contains("deleted by patch"),
        "comment marker should appear"
    );
    assert!(!content.contains("unimplemented"), "body should be gone");
}

#[test]
fn ast_grep_idempotent_after_application() {
    let workspace =
        create_workspace_with_file("src/lib.rs", r#"fn greet() { println!("hello"); }"#);
    let config = make_config(vec![ast_grep_patch(
        "greet",
        "src/lib.rs",
        "fn greet() { $$$ }",
        Operation::Replace {
            text: r#"fn greet() { println!("world"); }"#.to_string(),
        },
    )]);

    let r1 = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(matches!(r1[0].1, Ok(PatchResult::Applied { .. })));

    let r2 = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(r2[0].1, Ok(PatchResult::AlreadyApplied { .. })),
        "second apply should be idempotent: {:?}",
        r2[0].1
    );
}

#[test]
fn ast_grep_no_match_returns_error() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn other() {}");
    let config = make_config(vec![ast_grep_patch(
        "missing",
        "src/lib.rs",
        "fn nonexistent() { $$$ }",
        Operation::Replace {
            text: "fn nonexistent() {}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Err(ApplicationError::NoMatch { .. })),
        "no match should produce NoMatch error: {:?}",
        results[0].1
    );
}

#[test]
fn ast_grep_ambiguous_match_returns_error() {
    // Pattern `{ $$$ }` with value `42` matches both function bodies
    let workspace = create_workspace_with_file("src/lib.rs", "fn a() { 42 }\nfn b() { 42 }");
    let config = make_config(vec![ast_grep_patch(
        "ambiguous",
        "src/lib.rs",
        "{ 42 }",
        Operation::Replace {
            text: "{ 0 }".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Err(ApplicationError::AmbiguousMatch { .. })),
        "ambiguous pattern should produce AmbiguousMatch: {:?}",
        results[0].1
    );
}

// =============================================================================
// Tree-sitter DSL patterns
// =============================================================================

#[test]
fn ts_fn_replaces_function() {
    let workspace =
        create_workspace_with_file("src/lib.rs", "fn compute(x: i32) -> i32 {\n    x * 2\n}");
    let config = make_config(vec![tree_sitter_patch(
        "compute",
        "src/lib.rs",
        "fn compute",
        Operation::Replace {
            text: "fn compute(x: i32) -> i32 {\n    x * 3\n}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ts fn replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("x * 3"), "replacement should appear");
    assert!(!content.contains("x * 2"), "original should be gone");
}

#[test]
fn ts_struct_replaces_struct() {
    let workspace =
        create_workspace_with_file("src/lib.rs", "struct Point {\n    x: f64,\n    y: f64,\n}");
    let config = make_config(vec![tree_sitter_patch(
        "point",
        "src/lib.rs",
        "struct Point",
        Operation::Replace {
            text: "struct Point {\n    x: f64,\n    y: f64,\n    z: f64,\n}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ts struct replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("z: f64"), "new field should appear");
}

#[test]
fn ts_impl_replaces_impl_block() {
    let workspace = create_workspace_with_file(
        "src/lib.rs",
        "struct Counter { n: u32 }\nimpl Counter {\n    fn get(&self) -> u32 { self.n }\n}",
    );
    let config = make_config(vec![tree_sitter_patch(
        "counter",
        "src/lib.rs",
        "impl Counter",
        Operation::Replace {
            text: "impl Counter {\n    fn get(&self) -> u32 { self.n }\n    fn reset(&mut self) { self.n = 0; }\n}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ts impl replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("reset"), "new method should appear");
    assert!(content.contains("get"), "original method should survive");
}

#[test]
fn ts_method_replaces_specific_method() {
    let workspace = create_workspace_with_file(
        "src/lib.rs",
        "struct Greeter;\nimpl Greeter {\n    fn hello(&self) -> &str { \"hi\" }\n    fn bye(&self) -> &str { \"bye\" }\n}",
    );
    let config = make_config(vec![tree_sitter_patch(
        "hello",
        "src/lib.rs",
        "fn Greeter::hello",
        Operation::Replace {
            text: "fn hello(&self) -> &str { \"hey\" }".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ts method replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("hey"), "replaced method should appear");
    assert!(content.contains("bye"), "unrelated method should survive");
    assert!(
        !content.contains("\"hi\""),
        "original method body should be gone"
    );
}

#[test]
fn ts_const_replaces_constant() {
    let workspace = create_workspace_with_file("src/lib.rs", "const MAX_SIZE: usize = 100;");
    let config = make_config(vec![tree_sitter_patch(
        "max",
        "src/lib.rs",
        "const MAX_SIZE",
        Operation::Replace {
            text: "const MAX_SIZE: usize = 256;".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "ts const replace should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("256"), "new value should appear");
    assert!(!content.contains("100"), "old value should be gone");
}

#[test]
fn ts_const_regex_matches_multiple_consts() {
    // const /regex/ uses ConstMatching — verify it finds exactly the targeted consts
    // (this test only checks that the patch produces an AmbiguousMatch when >1 match,
    // exercising the ConstMatching path end-to-end)
    let workspace = create_workspace_with_file(
        "src/lib.rs",
        "const STATSIG_KEY: &str = \"k1\";\nconst STATSIG_URL: &str = \"u1\";\nconst OTHER: &str = \"o\";",
    );
    let config = make_config(vec![tree_sitter_patch(
        "statsig",
        "src/lib.rs",
        "const /^STATSIG_/",
        Operation::Replace {
            text: "const STATSIG_KEY: &str = \"k2\";".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    // Two consts match ^STATSIG_ → AmbiguousMatch
    assert!(
        matches!(
            results[0].1,
            Err(ApplicationError::AmbiguousMatch { count: 2, .. })
        ),
        "two STATSIG_ consts should produce AmbiguousMatch(2): {:?}",
        results[0].1
    );
}

// =============================================================================
// Tree-sitter raw S-expression query
// =============================================================================

#[test]
fn ts_custom_sexpr_replaces_function() {
    let workspace =
        create_workspace_with_file("src/lib.rs", r#"fn raw_fn() { println!("original"); }"#);
    // Raw S-expression query with outer @fn capture spanning the whole function item
    let query = r#"(function_item name: (identifier) @name (#eq? @name "raw_fn")) @fn"#;
    let config = make_config(vec![tree_sitter_patch(
        "raw",
        "src/lib.rs",
        query,
        Operation::Replace {
            text: r#"fn raw_fn() { println!("replaced"); }"#.to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Ok(PatchResult::Applied { .. })),
        "raw S-expression patch should apply: {:?}",
        results[0].1
    );

    let content = fs::read_to_string(workspace.path().join("src/lib.rs")).unwrap();
    assert!(content.contains("replaced"), "replacement should appear");
    assert!(!content.contains("original"), "original should be gone");
}

// =============================================================================
// Shared behavioural contracts (idempotency, errors)
// =============================================================================

#[test]
fn ts_idempotent_after_application() {
    let workspace =
        create_workspace_with_file("src/lib.rs", "fn compute(x: i32) -> i32 {\n    x * 2\n}");
    let config = make_config(vec![tree_sitter_patch(
        "compute",
        "src/lib.rs",
        "fn compute",
        Operation::Replace {
            text: "fn compute(x: i32) -> i32 {\n    x * 3\n}".to_string(),
        },
    )]);

    let r1 = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(r1[0].1, Ok(PatchResult::Applied { .. })),
        "first apply should succeed: {:?}",
        r1[0].1
    );

    let r2 = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(r2[0].1, Ok(PatchResult::AlreadyApplied { .. })),
        "second apply should be idempotent: {:?}",
        r2[0].1
    );
}

#[test]
fn ts_no_match_returns_error() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn other() {}");
    let config = make_config(vec![tree_sitter_patch(
        "missing",
        "src/lib.rs",
        "fn nonexistent",
        Operation::Replace {
            text: "fn nonexistent() {}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Err(ApplicationError::NoMatch { .. })),
        "missing function should produce NoMatch: {:?}",
        results[0].1
    );
}

#[test]
fn ts_ambiguous_match_returns_error() {
    // Two functions with the same name — tree-sitter finds both
    let workspace = create_workspace_with_file("src/lib.rs", "fn dup() { 1 }\nfn dup() { 2 }");
    let config = make_config(vec![tree_sitter_patch(
        "dup",
        "src/lib.rs",
        "fn dup",
        Operation::Replace {
            text: "fn dup() { 0 }".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        matches!(results[0].1, Err(ApplicationError::AmbiguousMatch { .. })),
        "duplicate function should produce AmbiguousMatch: {:?}",
        results[0].1
    );
}

#[test]
fn ts_unrecognized_pattern_returns_error() {
    let workspace = create_workspace_with_file("src/lib.rs", "fn foo() {}");
    let config = make_config(vec![tree_sitter_patch(
        "bad",
        "src/lib.rs",
        "xyz unknown_keyword something",
        Operation::Replace {
            text: "fn foo() {}".to_string(),
        },
    )]);

    let results = apply_patches(&config, workspace.path(), "1.0.0");
    assert!(
        results[0].1.is_err(),
        "unrecognized pattern should produce an error: {:?}",
        results[0].1
    );
}
