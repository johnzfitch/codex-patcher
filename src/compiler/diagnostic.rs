//! Diagnostic types for parsing cargo check output.
//!
//! Wraps cargo_metadata diagnostics with additional context for auto-fix.

use cargo_metadata::diagnostic::{
    Applicability, Diagnostic as CargoDiagnostic, DiagnosticLevel, DiagnosticSpan,
};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// A compile diagnostic with extracted auto-fix information.
#[derive(Debug, Clone)]
pub struct CompileDiagnostic {
    /// Error code (e.g., "E0063", "E0599")
    pub code: Option<String>,
    /// Human-readable message
    pub message: String,
    /// Diagnostic level (Error, Warning, etc.)
    pub level: DiagnosticLevel,
    /// Source spans where the error occurs
    pub spans: Vec<SourceSpan>,
    /// Compiler-suggested fixes
    pub suggestions: Vec<Suggestion>,
    /// Rendered output (for display)
    pub rendered: Option<String>,
}

/// A source location with byte offsets.
#[derive(Debug, Clone)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    /// If this span is inside a macro expansion
    pub is_macro_expansion: bool,
    /// The actual text at this span (if available)
    pub text: Option<String>,
}

/// A compiler-suggested fix.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub file: PathBuf,
    pub byte_start: usize,
    pub byte_end: usize,
    pub replacement: String,
    pub applicability: Applicability,
    /// Human-readable description of the fix
    pub message: String,
}

#[derive(Error, Debug)]
pub enum DiagnosticError {
    #[error("Failed to run cargo check: {0}")]
    CargoFailed(String),

    #[error("Failed to parse cargo output: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl CompileDiagnostic {
    /// Convert from cargo_metadata diagnostic.
    pub fn from_cargo(diag: &CargoDiagnostic, workspace_root: &Path) -> Self {
        let code = diag.code.as_ref().map(|c| c.code.clone());

        let spans: Vec<SourceSpan> = diag
            .spans
            .iter()
            .filter_map(|span| SourceSpan::from_cargo(span, workspace_root))
            .collect();

        // Extract suggestions from child diagnostics
        let mut suggestions = Vec::new();
        collect_suggestions(&diag.children, workspace_root, &mut suggestions);

        // Also collect from the main diagnostic's spans
        for span in &diag.spans {
            if let Some(replacement) = &span.suggested_replacement {
                if let Some(source_span) = SourceSpan::from_cargo(span, workspace_root) {
                    suggestions.push(Suggestion {
                        file: source_span.file,
                        byte_start: source_span.byte_start,
                        byte_end: source_span.byte_end,
                        replacement: replacement.clone(),
                        applicability: span
                            .suggestion_applicability
                            .clone()
                            .unwrap_or(Applicability::Unspecified),
                        message: diag.message.clone(),
                    });
                }
            }
        }

        CompileDiagnostic {
            code,
            message: diag.message.clone(),
            level: diag.level.clone(),
            spans,
            suggestions,
            rendered: diag.rendered.clone(),
        }
    }

    /// Check if this is a specific error code.
    pub fn is_error_code(&self, code: &str) -> bool {
        self.code.as_deref() == Some(code)
    }

    /// Check if this diagnostic has machine-applicable suggestions.
    pub fn has_machine_applicable_fix(&self) -> bool {
        self.suggestions
            .iter()
            .any(|s| s.applicability == Applicability::MachineApplicable)
    }

    /// Get all machine-applicable suggestions.
    #[must_use]
    pub fn machine_applicable_suggestions(&self) -> Vec<&Suggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.applicability == Applicability::MachineApplicable)
            .collect()
    }
}

impl SourceSpan {
    /// Convert from cargo_metadata span.
    fn from_cargo(span: &DiagnosticSpan, workspace_root: &Path) -> Option<Self> {
        let file_path = Path::new(&span.file_name);

        // Resolve path against workspace root
        let file = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            workspace_root.join(file_path)
        };

        // Skip files outside workspace (e.g., stdlib, dependencies)
        if !file.starts_with(workspace_root) {
            return None;
        }

        // Skip generated/macro-expanded files
        if span.file_name.contains("target/")
            || span.file_name.contains(".cargo/registry")
            || span.file_name.contains(".rustup")
        {
            return None;
        }

        let text = span.text.first().map(|t| t.text.clone());

        Some(SourceSpan {
            file,
            byte_start: span.byte_start as usize,
            byte_end: span.byte_end as usize,
            line_start: span.line_start as usize,
            line_end: span.line_end as usize,
            column_start: span.column_start as usize,
            column_end: span.column_end as usize,
            is_macro_expansion: span.expansion.is_some(),
            text,
        })
    }
}

/// Recursively collect suggestions from child diagnostics.
fn collect_suggestions(children: &[CargoDiagnostic], workspace_root: &Path, out: &mut Vec<Suggestion>) {
    for child in children {
        for span in &child.spans {
            if let Some(replacement) = &span.suggested_replacement {
                if let Some(source_span) = SourceSpan::from_cargo(span, workspace_root) {
                    out.push(Suggestion {
                        file: source_span.file,
                        byte_start: source_span.byte_start,
                        byte_end: source_span.byte_end,
                        replacement: replacement.clone(),
                        applicability: span
                            .suggestion_applicability
                            .clone()
                            .unwrap_or(Applicability::Unspecified),
                        message: child.message.clone(),
                    });
                }
            }
        }

        // Recurse into children
        collect_suggestions(&child.children, workspace_root, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_error_code() {
        let diag = CompileDiagnostic {
            code: Some("E0063".to_string()),
            message: "missing field".to_string(),
            level: DiagnosticLevel::Error,
            spans: vec![],
            suggestions: vec![],
            rendered: None,
        };

        assert!(diag.is_error_code("E0063"));
        assert!(!diag.is_error_code("E0599"));
    }
}
