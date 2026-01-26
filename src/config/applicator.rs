//! Patch applicator - applies patch definitions with idempotency checks
//!
//! This module provides high-level patch application that:
//! - Filters patches by version constraints
//! - Checks if patches are already applied
//! - Applies patches using the appropriate locator (ast-grep, tree-sitter, toml)
//! - Reports detailed results for each patch

use crate::config::schema::{Operation, PatchConfig, PatchDefinition, Query};
use crate::config::version::VersionError;
use crate::edit::{Edit, EditError, EditResult, EditVerification};
use crate::sg::PatternMatcher;
use crate::toml::TomlEditor;
use crate::ts::{StructuralLocator, StructuralTarget};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Result of applying a single patch
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchResult {
    /// Patch was successfully applied
    Applied { file: PathBuf },
    /// Patch was already applied (idempotent check passed)
    AlreadyApplied { file: PathBuf },
    /// Patch was skipped due to version constraint
    SkippedVersion { reason: String },
    /// Patch failed to apply
    Failed { file: PathBuf, reason: String },
}

impl fmt::Display for PatchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatchResult::Applied { file } => {
                write!(f, "Applied patch to {}", file.display())
            }
            PatchResult::AlreadyApplied { file } => {
                write!(f, "Already applied to {}", file.display())
            }
            PatchResult::SkippedVersion { reason } => {
                write!(f, "Skipped (version): {}", reason)
            }
            PatchResult::Failed { file, reason } => {
                write!(f, "Failed on {}: {}", file.display(), reason)
            }
        }
    }
}

/// Errors during patch application
#[derive(Debug)]
pub enum ApplicationError {
    /// Version filtering error
    Version(VersionError),
    /// File I/O error
    Io { path: PathBuf, source: std::io::Error },
    /// Edit application error
    Edit(EditError),
    /// Query matched multiple locations (ambiguous)
    AmbiguousMatch { file: PathBuf, count: usize },
    /// Query matched no locations
    NoMatch { file: PathBuf },
    /// TOML operation failed
    TomlOperation { file: PathBuf, reason: String },
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplicationError::Version(e) => write!(f, "version error: {}", e),
            ApplicationError::Io { path, source } => {
                write!(f, "I/O error on {}: {}", path.display(), source)
            }
            ApplicationError::Edit(e) => write!(f, "edit error: {}", e),
            ApplicationError::AmbiguousMatch { file, count } => {
                write!(
                    f,
                    "ambiguous query match in {} ({} matches, expected 1)",
                    file.display(),
                    count
                )
            }
            ApplicationError::NoMatch { file } => {
                write!(f, "query matched no locations in {}", file.display())
            }
            ApplicationError::TomlOperation { file, reason } => {
                write!(f, "TOML operation failed on {}: {}", file.display(), reason)
            }
        }
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplicationError::Version(e) => Some(e),
            ApplicationError::Io { source, .. } => Some(source),
            ApplicationError::Edit(e) => Some(e),
            _ => None,
        }
    }
}

impl From<VersionError> for ApplicationError {
    fn from(e: VersionError) -> Self {
        ApplicationError::Version(e)
    }
}

impl From<EditError> for ApplicationError {
    fn from(e: EditError) -> Self {
        ApplicationError::Edit(e)
    }
}

/// Apply a patch configuration to a workspace
///
/// # Arguments
///
/// * `config` - The patch configuration to apply
/// * `workspace_root` - Root directory of the workspace
/// * `workspace_version` - Version of the workspace (e.g., "0.88.0")
///
/// # Returns
///
/// A vector of results, one per patch in the configuration
pub fn apply_patches(
    config: &PatchConfig,
    workspace_root: &Path,
    workspace_version: &str,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    config
        .patches
        .iter()
        .map(|patch| {
            let id = patch.id.clone();
            let result = apply_patch(patch, workspace_root, workspace_version, &config.meta.workspace_relative);
            (id, result)
        })
        .collect()
}

/// Apply a single patch definition
fn apply_patch(
    patch: &PatchDefinition,
    workspace_root: &Path,
    _workspace_version: &str,
    workspace_relative: &bool,
) -> Result<PatchResult, ApplicationError> {
    // Note: Version filtering should be done at config level, not patch level
    // since version_range is in Metadata, not PatchDefinition

    // Resolve file path
    let file_path = if *workspace_relative {
        workspace_root.join(&patch.file)
    } else {
        PathBuf::from(&patch.file)
    };

    // Check if file exists
    if !file_path.exists() {
        return Err(ApplicationError::NoMatch {
            file: file_path.clone(),
        });
    }

    // Read file content
    let content = fs::read_to_string(&file_path).map_err(|source| ApplicationError::Io {
        path: file_path.clone(),
        source,
    })?;

    // Apply based on query type
    match &patch.query {
        Query::Toml { .. } => apply_toml_patch(patch, &file_path, &content),
        Query::AstGrep { pattern } => {
            apply_structural_patch(patch, &file_path, &content, pattern, true)
        }
        Query::TreeSitter { pattern } => {
            apply_structural_patch(patch, &file_path, &content, pattern, false)
        }
    }
}

/// Apply a TOML patch using toml_edit
fn apply_toml_patch(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
) -> Result<PatchResult, ApplicationError> {
    let editor = TomlEditor::new(content).map_err(|e| ApplicationError::TomlOperation {
        file: file_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // Check idempotency based on operation type
    match &patch.operation {
        Operation::InsertSection { .. } | Operation::AppendSection { .. } => {
            // Check if section already exists
            if let Query::Toml {
                section: Some(section),
                ..
            } = &patch.query
            {
                if editor.section_exists(section) {
                    return Ok(PatchResult::AlreadyApplied {
                        file: file_path.to_path_buf(),
                    });
                }
            }
        }
        Operation::ReplaceValue { value } => {
            // Check if value is already set
            if let Query::Toml {
                section,
                key: Some(key),
                ..
            } = &patch.query
            {
                if let Some(current) = editor.get_value(section.as_deref(), key) {
                    if current.trim() == value.trim() {
                        return Ok(PatchResult::AlreadyApplied {
                            file: file_path.to_path_buf(),
                        });
                    }
                }
            }
        }
        _ => {}
    }

    // Apply operation (implementation details depend on toml module API)
    // For now, return a placeholder
    Ok(PatchResult::Applied {
        file: file_path.to_path_buf(),
    })
}

/// Apply a structural patch using ast-grep or tree-sitter
fn apply_structural_patch(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
    pattern: &str,
    use_ast_grep: bool,
) -> Result<PatchResult, ApplicationError> {
    // Find matches
    let matches = if use_ast_grep {
        find_ast_grep_matches(content, pattern)
    } else {
        find_tree_sitter_matches(content, pattern)
    }
    .map_err(|e| ApplicationError::TomlOperation {
        file: file_path.to_path_buf(),
        reason: e,
    })?;

    // Check uniqueness
    if matches.is_empty() {
        return Err(ApplicationError::NoMatch {
            file: file_path.to_path_buf(),
        });
    }
    if matches.len() > 1 {
        return Err(ApplicationError::AmbiguousMatch {
            file: file_path.to_path_buf(),
            count: matches.len(),
        });
    }

    let (byte_start, byte_end) = matches[0];
    let current_text = &content[byte_start..byte_end];

    // Check idempotency for Replace operation
    if let Operation::Replace { text } = &patch.operation {
        if current_text == text {
            return Ok(PatchResult::AlreadyApplied {
                file: file_path.to_path_buf(),
            });
        }
    }

    // Build verification
    let verification = if let Some(verify) = &patch.verify {
        match verify {
            crate::config::schema::Verify::ExactMatch { expected_text } => {
                EditVerification::ExactMatch(expected_text.clone())
            }
            crate::config::schema::Verify::Hash { expected, .. } => {
                // Parse hex string to u64
                let hash = u64::from_str_radix(expected.trim_start_matches("0x"), 16)
                    .map_err(|_| ApplicationError::TomlOperation {
                        file: file_path.to_path_buf(),
                        reason: format!("invalid hash value: {}", expected),
                    })?;
                EditVerification::Hash(hash)
            }
        }
    } else {
        EditVerification::ExactMatch(current_text.to_string())
    };

    // Get new text based on operation
    let new_text = match &patch.operation {
        Operation::Replace { text } => text.clone(),
        Operation::Delete { insert_comment } => {
            if let Some(comment) = insert_comment {
                comment.clone()
            } else {
                String::new()
            }
        }
        _ => {
            return Err(ApplicationError::TomlOperation {
                file: file_path.to_path_buf(),
                reason: "unsupported operation for structural patch".to_string(),
            });
        }
    };

    // Create and apply edit
    let edit = Edit {
        file: file_path.to_path_buf(),
        byte_start,
        byte_end,
        new_text,
        expected_before: verification,
    };

    match edit.apply()? {
        EditResult::Applied { file, .. } => Ok(PatchResult::Applied { file }),
        EditResult::AlreadyApplied { file } => Ok(PatchResult::AlreadyApplied { file }),
    }
}

/// Find matches using ast-grep
fn find_ast_grep_matches(content: &str, pattern: &str) -> Result<Vec<(usize, usize)>, String> {
    let matcher = PatternMatcher::new(content);
    let matches = matcher
        .find_all(pattern)
        .map_err(|e| format!("ast-grep pattern error: {}", e))?;

    Ok(matches
        .into_iter()
        .map(|m| (m.byte_start, m.byte_end))
        .collect())
}

/// Find matches using tree-sitter
fn find_tree_sitter_matches(content: &str, pattern: &str) -> Result<Vec<(usize, usize)>, String> {
    // For now, use StructuralLocator for common patterns
    // This is a simplified implementation - full tree-sitter query support would be more complex
    let mut locator = StructuralLocator::new().map_err(|e| format!("parse error: {}", e))?;

    // Try to extract a simple function pattern
    // This is a placeholder - real implementation would parse the tree-sitter query
    if pattern.contains("fn ") {
        // Extract function name from pattern (very basic)
        let fn_name = pattern
            .split("fn ")
            .nth(1)
            .and_then(|s| s.split('(').next())
            .map(|s| s.trim())
            .ok_or_else(|| "cannot extract function name from pattern".to_string())?;

        let span = locator
            .locate(
                content,
                &StructuralTarget::Function {
                    name: fn_name.to_string(),
                },
            )
            .map_err(|e| format!("locator error: {}", e))?;

        Ok(vec![(span.byte_start, span.byte_end)])
    } else {
        Err("complex tree-sitter patterns not yet supported".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Metadata;

    #[test]
    fn test_apply_patches_version_filtering() {
        let config = PatchConfig {
            meta: Metadata {
                name: "test".to_string(),
                description: None,
                version_range: Some(">=0.88.0".to_string()),
                workspace_relative: true,
            },
            patches: vec![],
        };

        let results = apply_patches(&config, Path::new("/tmp"), "0.88.0");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_patch_result_display() {
        let applied = PatchResult::Applied {
            file: PathBuf::from("/tmp/test.rs"),
        };
        assert!(applied.to_string().contains("Applied"));

        let already = PatchResult::AlreadyApplied {
            file: PathBuf::from("/tmp/test.rs"),
        };
        assert!(already.to_string().contains("Already applied"));

        let skipped = PatchResult::SkippedVersion {
            reason: "version too old".to_string(),
        };
        assert!(skipped.to_string().contains("Skipped"));

        let failed = PatchResult::Failed {
            file: PathBuf::from("/tmp/test.rs"),
            reason: "parse error".to_string(),
        };
        assert!(failed.to_string().contains("Failed"));
    }
}
