//! Patch applicator - applies patch definitions with idempotency checks
//!
//! This module provides high-level patch application that:
//! - Filters patches by version constraints
//! - Checks if patches are already applied
//! - Applies patches using the appropriate locator (ast-grep, tree-sitter, toml)
//! - Reports detailed results for each patch

use crate::config::schema::{Operation, PatchConfig, PatchDefinition, Positioning, Query};
use crate::config::version::{matches_requirement, VersionError};
use crate::edit::{Edit, EditError, EditResult, EditVerification};
use crate::sg::PatternMatcher;
use crate::toml::{
    Constraints, KeyPath, SectionPath, TomlEditor, TomlOperation, TomlPlan, TomlQuery,
};
use crate::ts::StructuralTarget;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Check if a patch should be skipped based on its per-patch version constraint.
/// Returns `Some(reason)` if the patch should be skipped, `None` if it should be applied.
fn check_patch_version(
    patch: &PatchDefinition,
    workspace_version: &str,
) -> Result<Option<String>, ApplicationError> {
    let version_req = match patch.version.as_deref() {
        Some(r) => r,
        None => return Ok(None),
    };
    match matches_requirement(workspace_version, Some(version_req)) {
        Ok(true) => Ok(None),
        Ok(false) => Ok(Some(format!(
            "patch version {} not satisfied by workspace {}",
            version_req, workspace_version
        ))),
        Err(e) => Err(ApplicationError::Version(e)),
    }
}

/// Result of applying a single patch
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "PatchResult should be checked for success/failure"]
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
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
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

fn skip_all_patches(
    config: &PatchConfig,
    reason: String,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    config
        .patches
        .iter()
        .map(|patch| {
            (
                patch.id.clone(),
                Ok(PatchResult::SkippedVersion {
                    reason: reason.clone(),
                }),
            )
        })
        .collect()
}

fn error_all_patches(
    config: &PatchConfig,
    e: VersionError,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    config
        .patches
        .iter()
        .map(|patch| (patch.id.clone(), Err(ApplicationError::Version(e.clone()))))
        .collect()
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
    match matches_requirement(workspace_version, config.meta.version_range.as_deref()) {
        Ok(true) => apply_patches_batched(config, workspace_root, workspace_version),
        Ok(false) => {
            let req = config.meta.version_range.as_deref().unwrap_or("").trim();
            let reason = if req.is_empty() {
                format!("workspace version {workspace_version} does not satisfy patch version constraints")
            } else {
                format!(
                    "workspace version {workspace_version} does not satisfy version_range {req}"
                )
            };
            skip_all_patches(config, reason)
        }
        Err(e) => error_all_patches(config, e),
    }
}

/// Check patch status without mutating the workspace.
///
/// This mirrors `apply_patches` result semantics (`Applied` means "would apply"),
/// while running all edit operations against temporary files.
pub fn check_patches(
    config: &PatchConfig,
    workspace_root: &Path,
    workspace_version: &str,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    match matches_requirement(workspace_version, config.meta.version_range.as_deref()) {
        Ok(true) => check_patches_batched(config, workspace_root, workspace_version),
        Ok(false) => {
            let req = config.meta.version_range.as_deref().unwrap_or("").trim();
            let reason = if req.is_empty() {
                format!("workspace version {workspace_version} does not satisfy patch version constraints")
            } else {
                format!(
                    "workspace version {workspace_version} does not satisfy version_range {req}"
                )
            };
            skip_all_patches(config, reason)
        }
        Err(e) => error_all_patches(config, e),
    }
}

/// Read-only status evaluation that groups patches by file.
fn check_patches_batched(
    config: &PatchConfig,
    workspace_root: &Path,
    workspace_version: &str,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    use std::collections::HashMap;

    let mut patches_by_file: HashMap<PathBuf, Vec<&PatchDefinition>> = HashMap::new();

    for patch in &config.patches {
        let file_path = if config.meta.workspace_relative {
            workspace_root.join(&patch.file)
        } else {
            PathBuf::from(&patch.file)
        };
        patches_by_file.entry(file_path).or_default().push(patch);
    }

    let mut all_results = Vec::new();

    for (file_path, patches) in patches_by_file {
        if !file_path.exists() {
            for patch in patches {
                all_results.push((
                    patch.id.clone(),
                    Err(ApplicationError::NoMatch {
                        file: file_path.clone(),
                    }),
                ));
            }
            continue;
        }

        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(source) => {
                let kind = source.kind();
                let msg = source.to_string();
                for patch in patches {
                    all_results.push((
                        patch.id.clone(),
                        Err(ApplicationError::Io {
                            path: file_path.clone(),
                            source: std::io::Error::new(kind, msg.clone()),
                        }),
                    ));
                }
                continue;
            }
        };

        let mut edits_with_ids = Vec::new();
        let mut immediate_results = Vec::new();

        for patch in patches {
            match check_patch_version(patch, workspace_version) {
                Err(e) => {
                    immediate_results.push((patch.id.clone(), Err(e)));
                    continue;
                }
                Ok(Some(reason)) => {
                    immediate_results
                        .push((patch.id.clone(), Ok(PatchResult::SkippedVersion { reason })));
                    continue;
                }
                Ok(None) => {}
            }

            match compute_edit_for_patch(patch, &file_path, &content) {
                Ok(edit) => edits_with_ids.push((patch.id.clone(), edit)),
                Err(e) => immediate_results.push((patch.id.clone(), Err(e))),
            }
        }

        if !edits_with_ids.is_empty() {
            // Sort to match apply_batch's internal descending byte_start order so
            // the zip in simulate_batch_edits correctly pairs IDs with results.
            edits_with_ids.sort_by(|(_, a), (_, b)| b.byte_start.cmp(&a.byte_start));

            match simulate_batch_edits(&file_path, &content, &edits_with_ids) {
                Ok(results) => all_results.extend(results),
                Err(err) => {
                    let err_clone = err.clone();
                    for (patch_id, _) in &edits_with_ids {
                        all_results.push((
                            patch_id.clone(),
                            Err(ApplicationError::Edit(err_clone.clone())),
                        ));
                    }
                }
            }
        }

        all_results.extend(immediate_results);
    }

    // Restore config.patches order — HashMap iteration is unordered.
    let patch_order: std::collections::HashMap<&str, usize> = config
        .patches
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    all_results.sort_by_key(|(id, _)| patch_order.get(id.as_str()).copied().unwrap_or(usize::MAX));

    all_results
}

/// Simulate a batch of edits against a temporary file, preserving result semantics.
#[allow(clippy::type_complexity)]
fn simulate_batch_edits(
    file_path: &Path,
    content: &str,
    edits_with_ids: &[(String, Edit)],
) -> Result<Vec<(String, Result<PatchResult, ApplicationError>)>, EditError> {
    let temp_dir = tempfile::tempdir().map_err(EditError::Io)?;
    let temp_file = temp_dir.path().join("patch-check.tmp");
    fs::write(&temp_file, content).map_err(EditError::Io)?;

    let simulated_edits: Vec<Edit> = edits_with_ids
        .iter()
        .map(|(_, edit)| {
            let mut simulated = edit.clone();
            simulated.file = temp_file.clone();
            simulated
        })
        .collect();

    let results = Edit::apply_batch(simulated_edits)?;

    Ok(edits_with_ids
        .iter()
        .zip(results.iter())
        .map(|((patch_id, _), result)| {
            let patch_result = match result {
                EditResult::Applied { .. } => Ok(PatchResult::Applied {
                    file: file_path.to_path_buf(),
                }),
                EditResult::AlreadyApplied { .. } => Ok(PatchResult::AlreadyApplied {
                    file: file_path.to_path_buf(),
                }),
            };
            (patch_id.clone(), patch_result)
        })
        .collect())
}

/// Optimized batch application that groups patches by file.
///
/// All 4 query types (Text, AstGrep, TreeSitter, Toml) flow through
/// `compute_edit_for_patch` → `Edit::apply_batch`. Each file is read once,
/// all edits are computed, then applied atomically in a single write.
fn apply_patches_batched(
    config: &PatchConfig,
    workspace_root: &Path,
    workspace_version: &str,
) -> Vec<(String, Result<PatchResult, ApplicationError>)> {
    use std::collections::HashMap;

    let mut patches_by_file: HashMap<PathBuf, Vec<&PatchDefinition>> = HashMap::new();

    for patch in &config.patches {
        let file_path = if config.meta.workspace_relative {
            workspace_root.join(&patch.file)
        } else {
            PathBuf::from(&patch.file)
        };
        patches_by_file.entry(file_path).or_default().push(patch);
    }

    let mut all_results = Vec::new();

    for (file_path, patches) in patches_by_file {
        // Drain version-skipped patches before the file-existence check so a
        // patch targeting a file removed in a newer version returns
        // SkippedVersion instead of NoMatch.
        let patches: Vec<_> = patches
            .into_iter()
            .filter(|patch| {
                match check_patch_version(patch, workspace_version) {
                    Err(e) => {
                        all_results.push((patch.id.clone(), Err(e)));
                        false
                    }
                    Ok(Some(reason)) => {
                        all_results.push((
                            patch.id.clone(),
                            Ok(PatchResult::SkippedVersion { reason }),
                        ));
                        false
                    }
                    Ok(None) => true,
                }
            })
            .collect();

        if patches.is_empty() {
            continue;
        }

        if !file_path.exists() {
            for patch in patches {
                all_results.push((
                    patch.id.clone(),
                    Err(ApplicationError::NoMatch {
                        file: file_path.clone(),
                    }),
                ));
            }
            continue;
        }

        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(source) => {
                // Preserve kind + message; std::io::Error is not Clone so we
                // reconstruct one per patch from the original error's text.
                let kind = source.kind();
                let msg = source.to_string();
                for patch in patches {
                    all_results.push((
                        patch.id.clone(),
                        Err(ApplicationError::Io {
                            path: file_path.clone(),
                            source: std::io::Error::new(kind, msg.clone()),
                        }),
                    ));
                }
                continue;
            }
        };

        let mut edits_with_ids = Vec::new();
        let mut patch_errors = Vec::new();

        for patch in patches {
            match compute_edit_for_patch(patch, &file_path, &content) {
                Ok(edit) => edits_with_ids.push((patch.id.clone(), edit)),
                Err(e) => patch_errors.push((patch.id.clone(), Err(e))),
            }
        }

        if !edits_with_ids.is_empty() {
            // apply_batch sorts by byte_start descending internally.
            // Sort edits_with_ids the same way so zip() aligns correctly.
            edits_with_ids.sort_by(|(_, a), (_, b)| b.byte_start.cmp(&a.byte_start));

            let edits: Vec<Edit> = edits_with_ids.iter().map(|(_, e)| e.clone()).collect();

            match Edit::apply_batch(edits) {
                Ok(results) => {
                    for ((patch_id, _), result) in edits_with_ids.iter().zip(results.iter()) {
                        let patch_result = match result {
                            EditResult::Applied { .. } => Ok(PatchResult::Applied {
                                file: file_path.clone(),
                            }),
                            EditResult::AlreadyApplied { .. } => Ok(PatchResult::AlreadyApplied {
                                file: file_path.clone(),
                            }),
                        };
                        all_results.push((patch_id.clone(), patch_result));
                    }
                }
                Err(e) => {
                    // Reconstruct per-patch errors using Clone (kind+message preserved).
                    let e_clone = e.clone();
                    for (patch_id, _) in &edits_with_ids {
                        all_results.push((
                            patch_id.clone(),
                            Err(ApplicationError::Edit(e_clone.clone())),
                        ));
                    }
                }
            }
        }

        all_results.extend(patch_errors);
    }

    // Restore config.patches order — HashMap iteration is unordered.
    let patch_order: std::collections::HashMap<&str, usize> = config
        .patches
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    all_results.sort_by_key(|(id, _)| patch_order.get(id.as_str()).copied().unwrap_or(usize::MAX));

    all_results
}

/// Convert a TOML patch into an `Edit` (or a sentinel no-op `Edit` when the
/// operation is already satisfied).
///
/// Passes `patch.constraint` through to `TomlEditor::plan` so that
/// `ensure_absent` / `ensure_present` constraints are enforced at runtime.
fn compute_toml_edit(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
) -> Result<Edit, ApplicationError> {
    let editor =
        TomlEditor::from_path(file_path, content).map_err(|e| ApplicationError::TomlOperation {
            file: file_path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let toml_query = match &patch.query {
        Query::Toml { section, key, .. } => {
            if let Some(key_val) = key {
                let section_path = if let Some(sec) = section {
                    SectionPath::parse(sec).map_err(|e| ApplicationError::TomlOperation {
                        file: file_path.to_path_buf(),
                        reason: format!("Invalid section path: {}", e),
                    })?
                } else {
                    SectionPath::parse("").map_err(|e| ApplicationError::TomlOperation {
                        file: file_path.to_path_buf(),
                        reason: format!("Invalid section path: {}", e),
                    })?
                };
                let key_path =
                    KeyPath::parse(key_val).map_err(|e| ApplicationError::TomlOperation {
                        file: file_path.to_path_buf(),
                        reason: format!("Invalid key path: {}", e),
                    })?;
                TomlQuery::Key {
                    section: section_path,
                    key: key_path,
                }
            } else if let Some(section_val) = section {
                let section_path = SectionPath::parse(section_val).map_err(|e| {
                    ApplicationError::TomlOperation {
                        file: file_path.to_path_buf(),
                        reason: format!("Invalid section path: {}", e),
                    }
                })?;
                TomlQuery::Section { path: section_path }
            } else {
                return Err(ApplicationError::TomlOperation {
                    file: file_path.to_path_buf(),
                    reason: "TOML query must specify section or key".to_string(),
                });
            }
        }
        _ => unreachable!("compute_toml_edit called with non-TOML query"),
    };

    let toml_operation = match &patch.operation {
        Operation::InsertSection { text, positioning } => TomlOperation::InsertSection {
            text: text.clone(),
            positioning: convert_positioning(positioning).map_err(|e| {
                ApplicationError::TomlOperation {
                    file: file_path.to_path_buf(),
                    reason: format!("Invalid positioning: {}", e),
                }
            })?,
        },
        Operation::AppendSection { text } => TomlOperation::AppendSection { text: text.clone() },
        Operation::ReplaceValue { value } => TomlOperation::ReplaceValue {
            value: value.clone(),
        },
        Operation::DeleteSection => TomlOperation::DeleteSection,
        Operation::ReplaceKey { new_key } => TomlOperation::ReplaceKey {
            new_key: new_key.clone(),
        },
        _ => {
            return Err(ApplicationError::TomlOperation {
                file: file_path.to_path_buf(),
                reason: format!("Unsupported operation for TOML: {:?}", patch.operation),
            });
        }
    };

    let constraints = patch
        .constraint
        .as_ref()
        .map(|c| Constraints {
            ensure_absent: c.ensure_absent,
            ensure_present: c.ensure_present,
        })
        .unwrap_or_else(Constraints::none);

    let plan = editor
        .plan(&toml_query, &toml_operation, constraints)
        .map_err(|e| ApplicationError::TomlOperation {
            file: file_path.to_path_buf(),
            reason: e.to_string(),
        })?;

    match plan {
        TomlPlan::Edit(edit) => Ok(edit),
        TomlPlan::NoOp(_) => {
            // Anchor the sentinel at EOF to avoid colliding with real edits at byte 0.
            let end = content.len();
            Ok(Edit::new(file_path, end, end, "", ""))
        }
    }
}

/// Compute an Edit for a patch without applying it.
fn compute_edit_for_patch(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
) -> Result<Edit, ApplicationError> {
    match &patch.query {
        Query::Text {
            search,
            fuzzy_threshold,
            fuzzy_expansion,
        } => compute_text_edit(
            patch,
            file_path,
            content,
            search,
            *fuzzy_threshold,
            *fuzzy_expansion,
        ),
        Query::AstGrep { pattern } => {
            compute_structural_edit(patch, file_path, content, pattern, true)
        }
        Query::TreeSitter { pattern } => {
            compute_structural_edit(patch, file_path, content, pattern, false)
        }
        Query::Toml { .. } => compute_toml_edit(patch, file_path, content),
    }
}

/// Compute a text edit without applying it (for batching).
fn compute_text_edit(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
    search: &str,
    fuzzy_threshold: Option<f64>,
    fuzzy_expansion: Option<usize>,
) -> Result<Edit, ApplicationError> {
    // Check if the search text exists in the file
    if !content.contains(search) {
        // Check if the replacement text already exists (idempotency)
        if let Operation::Replace { text } = &patch.operation {
            if content.contains(text.as_str()) {
                // Return a no-op edit for idempotency
                return Ok(Edit::new(file_path, 0, 0, String::new(), ""));
            }
        }

        // Fuzzy fallback: only when the user has explicitly opted in via threshold or expansion.
        if fuzzy_threshold.is_none() && fuzzy_expansion.is_none() {
            return Err(ApplicationError::NoMatch {
                file: file_path.to_path_buf(),
            });
        }
        let threshold = fuzzy_threshold.unwrap_or(0.85);
        let fuzzy_result = match fuzzy_expansion {
            Some(expansion) => {
                crate::fuzzy::find_best_match_elastic(search, content, threshold, expansion)
            }
            None => crate::fuzzy::find_best_match(search, content, threshold),
        };
        if let Some(fuzzy) = fuzzy_result {
            eprintln!(
                "  [fuzzy] patch '{}': exact match failed, using fuzzy match (score: {:.2})",
                patch.id, fuzzy.score
            );

            return match &patch.operation {
                Operation::Replace { text } => Ok(Edit::new(
                    file_path,
                    fuzzy.start,
                    fuzzy.end,
                    text.clone(),
                    fuzzy.matched_text,
                )),
                _ => Err(ApplicationError::TomlOperation {
                    file: file_path.to_path_buf(),
                    reason: "Text queries only support 'replace' operation".to_string(),
                }),
            };
        }

        return Err(ApplicationError::NoMatch {
            file: file_path.to_path_buf(),
        });
    }

    // O(1) ambiguity check: bail if more than one match exists
    let mut occurrences = content.match_indices(search);
    let first = occurrences.next();
    if first.is_some() && occurrences.next().is_some() {
        return Err(ApplicationError::AmbiguousMatch {
            file: file_path.to_path_buf(),
            count: content.matches(search).count(), // full count only for error message
        });
    }

    // Create edit
    match &patch.operation {
        Operation::Replace { text } => {
            let byte_start = first.expect("existence checked above").0;
            let byte_end = byte_start + search.len();
            let verification = if let Some(verify) = &patch.verify {
                match verify {
                    crate::config::schema::Verify::ExactMatch { expected_text } => {
                        EditVerification::ExactMatch(expected_text.clone())
                    }
                    crate::config::schema::Verify::Hash { expected, .. } => {
                        let hash = u64::from_str_radix(expected.trim_start_matches("0x"), 16)
                            .map_err(|_| ApplicationError::TomlOperation {
                                file: file_path.to_path_buf(),
                                reason: format!("invalid hash value: {}", expected),
                            })?;
                        EditVerification::Hash(hash)
                    }
                }
            } else {
                EditVerification::from_text(search)
            };
            Ok(Edit::with_verification(
                file_path,
                byte_start,
                byte_end,
                text.clone(),
                verification,
            ))
        }
        _ => Err(ApplicationError::TomlOperation {
            file: file_path.to_path_buf(),
            reason: "Text queries only support 'replace' operation".to_string(),
        }),
    }
}

/// Compute a structural edit without applying it (for batching).
fn compute_structural_edit(
    patch: &PatchDefinition,
    file_path: &Path,
    content: &str,
    pattern: &str,
    use_ast_grep: bool,
) -> Result<Edit, ApplicationError> {
    fn align_trailing_newline(current_text: &str, replacement: &str) -> String {
        // ast-grep spans typically exclude the following newline. Many patch definitions
        // use triple-quoted strings that include a trailing '\n'. Align to the matched
        // span so replace patches are idempotent.
        match (current_text.ends_with('\n'), replacement.ends_with('\n')) {
            (true, false) => {
                let mut s = replacement.to_string();
                s.push('\n');
                s
            }
            (false, true) => replacement
                .strip_suffix('\n')
                .unwrap_or(replacement)
                .to_string(),
            _ => replacement.to_string(),
        }
    }

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

    // Special handling for Delete operations
    if matches.is_empty() {
        // Structural replace patches can still be already applied if the target
        // shape changed but the replacement text is present in the file.
        if let Operation::Replace { text } = &patch.operation {
            let replacement = text.as_str();
            let replacement_without_trailing_newline = replacement.trim_end_matches('\n');
            if content.contains(replacement)
                || content.contains(replacement_without_trailing_newline)
            {
                return Ok(Edit::new(file_path, 0, 0, String::new(), ""));
            }
        }

        // For Delete operations, check if the deletion was already applied
        if let Operation::Delete { insert_comment } = &patch.operation {
            if let Some(comment) = insert_comment {
                // Check if the comment exists in the file
                if content.contains(comment) {
                    // Return a no-op edit for idempotency
                    return Ok(Edit::new(file_path, 0, 0, String::new(), ""));
                }
            }
            // If no comment or comment not found, return no-op edit
            return Ok(Edit::new(file_path, 0, 0, String::new(), ""));
        }

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

    // Build verification
    let verification = if let Some(verify) = &patch.verify {
        match verify {
            crate::config::schema::Verify::ExactMatch { expected_text } => {
                EditVerification::ExactMatch(expected_text.clone())
            }
            crate::config::schema::Verify::Hash { expected, .. } => {
                // Parse hex string to u64
                let hash =
                    u64::from_str_radix(expected.trim_start_matches("0x"), 16).map_err(|_| {
                        ApplicationError::TomlOperation {
                            file: file_path.to_path_buf(),
                            reason: format!("invalid hash value: {}", expected),
                        }
                    })?;
                EditVerification::Hash(hash)
            }
        }
    } else {
        EditVerification::ExactMatch(current_text.to_string())
    };

    // Get new text based on operation
    let new_text = match &patch.operation {
        Operation::Replace { text } => align_trailing_newline(current_text, text.as_str()),
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

    // Check idempotency for Replace operation (after normalizing trailing newline).
    if matches!(patch.operation, Operation::Replace { .. }) && current_text == new_text {
        return Ok(Edit::new(file_path, 0, 0, String::new(), ""));
    }

    // Create edit without applying
    Ok(Edit {
        file: file_path.to_path_buf(),
        byte_start,
        byte_end,
        new_text,
        expected_before: verification,
    })
}

/// Convert config::Positioning to toml::Positioning.
///
/// Positioning validation (at-most-one directive) is enforced at load time via
/// `Positioning::validate()` in schema.rs, so no re-validation is needed here.
fn convert_positioning(pos: &Positioning) -> Result<crate::toml::Positioning, String> {
    use crate::toml::Positioning as TP;

    if let Some(after) = &pos.after_section {
        let path =
            SectionPath::parse(after).map_err(|e| format!("Invalid after_section: {}", e))?;
        Ok(TP::AfterSection(path))
    } else if let Some(before) = &pos.before_section {
        let path =
            SectionPath::parse(before).map_err(|e| format!("Invalid before_section: {}", e))?;
        Ok(TP::BeforeSection(path))
    } else if pos.at_end {
        Ok(TP::AtEnd)
    } else if pos.at_beginning {
        Ok(TP::AtBeginning)
    } else {
        // Default to AtEnd if nothing specified
        Ok(TP::AtEnd)
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

/// Parse a tree-sitter pattern string into a `StructuralTarget`.
///
/// Accepts two forms:
///
/// **S-expression** (starts with `(`): passed directly to the tree-sitter query
/// engine as a `Custom` target. The query must include at least one capture
/// that spans the desired replacement range.
///
/// **DSL shorthand**: a human-readable prefix syntax that maps to well-known
/// `StructuralTarget` variants:
///
/// | Pattern | Target |
/// |---|---|
/// | `fn name` | `Function { name }` |
/// | `fn Type::method` | `Method { type_name, method_name }` |
/// | `struct Name` | `Struct { name }` |
/// | `enum Name` | `Enum { name }` |
/// | `const NAME` | `Const { name }` |
/// | `const /regex/` | `ConstMatching { pattern }` |
/// | `static NAME` | `Static { name }` |
/// | `impl Type` | `Impl { type_name }` |
/// | `impl Trait for Type` | `ImplTrait { trait_name, type_name }` |
/// | `use path_pattern` | `Use { path_pattern }` |
fn parse_tree_sitter_pattern(pattern: &str) -> Result<StructuralTarget, String> {
    let pattern = pattern.trim();

    // Raw S-expression tree-sitter query
    if pattern.starts_with('(') {
        return Ok(StructuralTarget::Custom {
            query: pattern.to_string(),
        });
    }

    // DSL: `fn Type::method` or `fn name`
    if let Some(rest) = pattern.strip_prefix("fn ") {
        let rest = rest.trim();
        if let Some((type_name, method_name)) = rest.split_once("::") {
            return Ok(StructuralTarget::Method {
                type_name: type_name.trim().to_string(),
                method_name: method_name.trim().to_string(),
            });
        }
        return Ok(StructuralTarget::Function {
            name: rest.to_string(),
        });
    }

    // DSL: `struct Name`
    if let Some(name) = pattern.strip_prefix("struct ") {
        return Ok(StructuralTarget::Struct {
            name: name.trim().to_string(),
        });
    }

    // DSL: `enum Name`
    if let Some(name) = pattern.strip_prefix("enum ") {
        return Ok(StructuralTarget::Enum {
            name: name.trim().to_string(),
        });
    }

    // DSL: `const /regex/` or `const NAME`
    if let Some(rest) = pattern.strip_prefix("const ") {
        let rest = rest.trim();
        if rest.starts_with('/') && rest.ends_with('/') && rest.len() > 1 {
            let regex_pattern = &rest[1..rest.len() - 1];
            return Ok(StructuralTarget::ConstMatching {
                pattern: regex_pattern.to_string(),
            });
        }
        return Ok(StructuralTarget::Const {
            name: rest.to_string(),
        });
    }

    // DSL: `static NAME`
    if let Some(name) = pattern.strip_prefix("static ") {
        return Ok(StructuralTarget::Static {
            name: name.trim().to_string(),
        });
    }

    // DSL: `impl Trait for Type` or `impl Type`
    if let Some(rest) = pattern.strip_prefix("impl ") {
        let rest = rest.trim();
        if let Some(for_pos) = rest.find(" for ") {
            let trait_name = rest[..for_pos].trim();
            let type_name = rest[for_pos + 5..].trim();
            return Ok(StructuralTarget::ImplTrait {
                trait_name: trait_name.to_string(),
                type_name: type_name.to_string(),
            });
        }
        return Ok(StructuralTarget::Impl {
            type_name: rest.to_string(),
        });
    }

    // DSL: `use path_pattern`
    if let Some(path_pattern) = pattern.strip_prefix("use ") {
        return Ok(StructuralTarget::Use {
            path_pattern: path_pattern.trim().to_string(),
        });
    }

    Err(format!(
        "unrecognized tree-sitter pattern: {:?}. \
        Use S-expression syntax (starting with '(') or a DSL shorthand: \
        fn name, fn Type::method, struct Name, enum Name, const NAME, \
        const /regex/, static NAME, impl Type, impl Trait for Type, use path_pattern",
        pattern
    ))
}

/// Find matches using tree-sitter (pooled parser for performance).
///
/// Accepts the DSL shorthand or raw S-expression syntax described in
/// [`parse_tree_sitter_pattern`].
fn find_tree_sitter_matches(content: &str, pattern: &str) -> Result<Vec<(usize, usize)>, String> {
    use crate::ts::locator::pooled;

    let target = parse_tree_sitter_pattern(pattern)?;

    // For Method targets the query engine's union span runs from the impl's type
    // identifier all the way to the method body — wider than the method alone.
    // Extract the dedicated `@method` capture to get the correct replacement span.
    let is_method = matches!(target, StructuralTarget::Method { .. });

    let results =
        pooled::locate_all(content, &target).map_err(|e| format!("tree-sitter error: {}", e))?;

    Ok(results
        .into_iter()
        .map(|r| {
            if is_method {
                r.captures
                    .get("method")
                    .map(|c| (c.byte_start, c.byte_end))
                    .unwrap_or((r.byte_start, r.byte_end))
            } else {
                (r.byte_start, r.byte_end)
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Metadata;

    // -------------------------------------------------------------------------
    // parse_tree_sitter_pattern unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn ts_parse_fn_name() {
        assert!(matches!(
            parse_tree_sitter_pattern("fn hello"),
            Ok(StructuralTarget::Function { name }) if name == "hello"
        ));
    }

    #[test]
    fn ts_parse_method() {
        assert!(matches!(
            parse_tree_sitter_pattern("fn Foo::bar"),
            Ok(StructuralTarget::Method { type_name, method_name })
                if type_name == "Foo" && method_name == "bar"
        ));
    }

    #[test]
    fn ts_parse_struct() {
        assert!(matches!(
            parse_tree_sitter_pattern("struct Config"),
            Ok(StructuralTarget::Struct { name }) if name == "Config"
        ));
    }

    #[test]
    fn ts_parse_enum() {
        assert!(matches!(
            parse_tree_sitter_pattern("enum Status"),
            Ok(StructuralTarget::Enum { name }) if name == "Status"
        ));
    }

    #[test]
    fn ts_parse_const_by_name() {
        assert!(matches!(
            parse_tree_sitter_pattern("const MAX_SIZE"),
            Ok(StructuralTarget::Const { name }) if name == "MAX_SIZE"
        ));
    }

    #[test]
    fn ts_parse_const_regex() {
        assert!(matches!(
            parse_tree_sitter_pattern("const /^STATSIG_/"),
            Ok(StructuralTarget::ConstMatching { pattern }) if pattern == "^STATSIG_"
        ));
    }

    #[test]
    fn ts_parse_static() {
        assert!(matches!(
            parse_tree_sitter_pattern("static COUNTER"),
            Ok(StructuralTarget::Static { name }) if name == "COUNTER"
        ));
    }

    #[test]
    fn ts_parse_impl() {
        assert!(matches!(
            parse_tree_sitter_pattern("impl Foo"),
            Ok(StructuralTarget::Impl { type_name }) if type_name == "Foo"
        ));
    }

    #[test]
    fn ts_parse_impl_trait() {
        assert!(matches!(
            parse_tree_sitter_pattern("impl Display for Foo"),
            Ok(StructuralTarget::ImplTrait { trait_name, type_name })
                if trait_name == "Display" && type_name == "Foo"
        ));
    }

    #[test]
    fn ts_parse_use() {
        assert!(matches!(
            parse_tree_sitter_pattern("use std::collections"),
            Ok(StructuralTarget::Use { path_pattern }) if path_pattern == "std::collections"
        ));
    }

    #[test]
    fn ts_parse_sexpr() {
        let q = "(function_item) @func";
        assert!(matches!(
            parse_tree_sitter_pattern(q),
            Ok(StructuralTarget::Custom { query }) if query == q
        ));
    }

    #[test]
    fn ts_parse_unknown_errors() {
        assert!(parse_tree_sitter_pattern("xyz unknown").is_err());
        let err = parse_tree_sitter_pattern("xyz unknown").unwrap_err();
        assert!(
            err.contains("unrecognized"),
            "error message should be descriptive: {err}"
        );
    }

    // -------------------------------------------------------------------------
    // Applicator integration tests
    // -------------------------------------------------------------------------

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
