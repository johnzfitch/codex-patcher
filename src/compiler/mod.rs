//! Compiler integration for parsing diagnostics and applying auto-fixes.
//!
//! This module provides the bridge between `cargo check` output and the
//! codex-patcher edit system. It can:
//!
//! 1. Run `cargo check --message-format=json` and parse diagnostics
//! 2. Apply machine-applicable compiler suggestions automatically
//! 3. Generate fixes for common error patterns (E0063 missing fields)
//!
//! # Example
//!
//! ```no_run
//! use codex_patcher::compiler::{run_cargo_check, try_autofix_all};
//! use std::path::Path;
//!
//! let workspace = Path::new("/path/to/crate");
//! let diagnostics = run_cargo_check(workspace, None).unwrap();
//!
//! for diag in &diagnostics {
//!     println!("Error: {}", diag.message);
//! }
//!
//! // Attempt to auto-fix all diagnostics
//! let fixes = try_autofix_all(&diagnostics, workspace);
//! ```

pub mod autofix;
pub mod diagnostic;

pub use autofix::{try_autofix, AutofixError, AutofixResult};
pub use diagnostic::{CompileDiagnostic, DiagnosticError, SourceSpan, Suggestion};

use cargo_metadata::diagnostic::DiagnosticLevel;
use cargo_metadata::Message;
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::edit::Edit;

/// Run `cargo check` and collect all error diagnostics.
///
/// # Arguments
///
/// * `workspace` - Path to the Cargo workspace root
/// * `package` - Optional package name to check (for workspaces with multiple crates)
///
/// # Returns
///
/// Vector of error-level diagnostics with parsed spans and suggestions.
pub fn run_cargo_check(
    workspace: &Path,
    package: Option<&str>,
) -> Result<Vec<CompileDiagnostic>, DiagnosticError> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace)
        .args(["check", "--message-format=json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(pkg) = package {
        cmd.args(["-p", pkg]);
    }

    // Disable incremental compilation for deterministic results
    cmd.env("CARGO_INCREMENTAL", "0");

    let output = cmd.output().map_err(|e| {
        DiagnosticError::CargoFailed(format!("Failed to spawn cargo: {}", e))
    })?;

    let reader = std::io::BufReader::new(output.stdout.as_slice());
    parse_cargo_output(reader, workspace)
}

/// Run `cargo check` on a specific file (using workspace context).
///
/// This is useful when you only want to check one file after an edit.
pub fn check_file(
    workspace: &Path,
    _file: &Path,
) -> Result<Vec<CompileDiagnostic>, DiagnosticError> {
    // Cargo doesn't support single-file checks, so we check the whole workspace
    // but filter to errors in the specific file.
    // For now, just run the full check.
    run_cargo_check(workspace, None)
}

/// Parse cargo check JSON output into diagnostics.
fn parse_cargo_output<R: BufRead>(
    reader: R,
    workspace: &Path,
) -> Result<Vec<CompileDiagnostic>, DiagnosticError> {
    let mut diagnostics = Vec::new();

    for line in reader.lines() {
        let line = line?;

        // Defensive: proc macros can print garbage, only parse JSON lines
        if !line.starts_with('{') {
            continue;
        }

        let message: Message = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => continue, // Skip malformed JSON
        };

        if let Message::CompilerMessage(msg) = message {
            // Only collect errors (not warnings, notes, etc.)
            if matches!(msg.message.level, DiagnosticLevel::Error) {
                let diag = CompileDiagnostic::from_cargo(&msg.message, workspace);
                diagnostics.push(diag);
            }
        }
    }

    Ok(diagnostics)
}

/// Attempt to auto-fix all diagnostics and return the combined edits.
///
/// Returns a tuple of (successful edits, unfixable diagnostics).
#[must_use]
pub fn try_autofix_all<'a>(
    diagnostics: &'a [CompileDiagnostic],
    workspace: &Path,
) -> (Vec<Edit>, Vec<&'a CompileDiagnostic>) {
    let mut edits = Vec::new();
    let mut unfixable = Vec::new();

    for diag in diagnostics {
        match try_autofix(diag, workspace) {
            AutofixResult::Fixed(fixes) => {
                edits.extend(fixes);
            }
            AutofixResult::CannotFix { reason: _ } => {
                unfixable.push(diag);
            }
        }
    }

    (edits, unfixable)
}

/// Run cargo check, attempt auto-fixes, and return results.
///
/// This is the high-level entry point for the fix loop.
///
/// # Returns
///
/// - `Ok(Vec::new())` if build succeeds with no errors
/// - `Ok(edits)` with auto-fixes to apply
/// - `Err` with diagnostics that couldn't be fixed
pub fn check_and_fix(
    workspace: &Path,
    package: Option<&str>,
) -> Result<Vec<Edit>, (Vec<Edit>, Vec<CompileDiagnostic>)> {
    let diagnostics = match run_cargo_check(workspace, package) {
        Ok(d) => d,
        Err(e) => {
            return Err((
                Vec::new(),
                vec![CompileDiagnostic {
                    code: None,
                    message: format!("Cargo check failed: {}", e),
                    level: DiagnosticLevel::Error,
                    spans: Vec::new(),
                    suggestions: Vec::new(),
                    rendered: None,
                }],
            ));
        }
    };

    if diagnostics.is_empty() {
        return Ok(Vec::new());
    }

    let (edits, unfixable) = try_autofix_all(&diagnostics, workspace);

    if unfixable.is_empty() {
        Ok(edits)
    } else {
        // Return owned diagnostics for the error case
        let owned_unfixable: Vec<CompileDiagnostic> = unfixable.into_iter().cloned().collect();
        Err((edits, owned_unfixable))
    }
}

/// Quick pass/fail check - returns true if cargo check succeeds.
pub fn check_passes(workspace: &Path, package: Option<&str>) -> bool {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace)
        .args(["check", "--message-format=short"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(pkg) = package {
        cmd.args(["-p", pkg]);
    }

    match cmd.status() {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("warning: failed to run cargo check: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_output() {
        let output = "";
        let reader = std::io::BufReader::new(output.as_bytes());
        let diagnostics = parse_cargo_output(reader, Path::new("/tmp")).unwrap();
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_garbage_output() {
        // Simulate proc macro garbage mixed with JSON
        let output = "some random text\n{\"reason\":\"build-finished\",\"success\":true}\nmore garbage";
        let reader = std::io::BufReader::new(output.as_bytes());
        let diagnostics = parse_cargo_output(reader, Path::new("/tmp")).unwrap();
        // Should not crash, just return empty (build-finished isn't an error)
        assert!(diagnostics.is_empty());
    }
}
