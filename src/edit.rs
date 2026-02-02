use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// The fundamental edit primitive: byte-span replacement with verification.
///
/// All high-level operations (AST transforms, structural edits, diffs) compile down
/// to this single primitive. Intelligence lives in span acquisition, not application.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "Edit does nothing until apply() is called"]
pub struct Edit {
    /// Path to the file to edit (absolute, workspace-relative resolved)
    pub file: PathBuf,
    /// Starting byte offset (inclusive)
    pub byte_start: usize,
    /// Ending byte offset (exclusive)
    pub byte_end: usize,
    /// New text to insert at [byte_start, byte_end)
    pub new_text: String,
    /// Verification of what we expect to find before applying
    pub expected_before: EditVerification,
}

/// Verification strategy for edit safety.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditVerification {
    /// Exact text match required
    ExactMatch(String),
    /// xxh3 hash of expected text (faster for large spans)
    Hash(u64),
}

impl EditVerification {
    /// Check if the provided text matches the verification criteria.
    pub fn matches(&self, text: &str) -> bool {
        match self {
            EditVerification::ExactMatch(expected) => text == expected,
            EditVerification::Hash(expected_hash) => {
                let actual_hash = xxh3_64(text.as_bytes());
                actual_hash == *expected_hash
            }
        }
    }

    /// Create verification from text, using hash for text over 1KB.
    pub fn from_text(text: &str) -> Self {
        if text.len() > 1024 {
            EditVerification::Hash(xxh3_64(text.as_bytes()))
        } else {
            EditVerification::ExactMatch(text.to_string())
        }
    }

    /// Get hash value regardless of variant.
    pub fn hash(&self) -> u64 {
        match self {
            EditVerification::Hash(h) => *h,
            EditVerification::ExactMatch(text) => xxh3_64(text.as_bytes()),
        }
    }
}

#[derive(Error, Debug)]
pub enum EditError {
    #[error("Before-text verification failed at {file}:{byte_start}")]
    BeforeTextMismatch {
        file: PathBuf,
        byte_start: usize,
        byte_end: usize,
        expected: String,
        found: String,
    },

    #[error("Invalid byte range: [{byte_start}, {byte_end}) in file of length {file_len}")]
    InvalidByteRange {
        byte_start: usize,
        byte_end: usize,
        file_len: usize,
    },

    #[error("Cannot edit file outside workspace: {0}")]
    OutsideWorkspace(PathBuf),

    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 validation error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Invalid edit would create malformed UTF-8")]
    InvalidUtf8Edit,
}

/// Result of applying an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "EditResult should be checked for success/already-applied"]
pub enum EditResult {
    /// Edit was successfully applied
    Applied { file: PathBuf, bytes_changed: usize },
    /// Edit was already applied (current text matches new_text)
    AlreadyApplied { file: PathBuf },
}

impl Edit {
    /// Create a new edit with automatic verification generation.
    pub fn new(
        file: impl Into<PathBuf>,
        byte_start: usize,
        byte_end: usize,
        new_text: impl Into<String>,
        expected_before: impl Into<String>,
    ) -> Self {
        let expected = expected_before.into();
        Self {
            file: file.into(),
            byte_start,
            byte_end,
            new_text: new_text.into(),
            expected_before: EditVerification::from_text(&expected),
        }
    }

    /// Create an edit with explicit verification strategy.
    pub fn with_verification(
        file: impl Into<PathBuf>,
        byte_start: usize,
        byte_end: usize,
        new_text: impl Into<String>,
        verification: EditVerification,
    ) -> Self {
        Self {
            file: file.into(),
            byte_start,
            byte_end,
            new_text: new_text.into(),
            expected_before: verification,
        }
    }

    /// Validate the edit against the current file contents.
    ///
    /// Returns the current text at [byte_start, byte_end) if validation succeeds.
    fn validate<'a>(&self, content: &'a [u8]) -> Result<&'a [u8], EditError> {
        // Validate byte range
        if self.byte_start > self.byte_end {
            return Err(EditError::InvalidByteRange {
                byte_start: self.byte_start,
                byte_end: self.byte_end,
                file_len: content.len(),
            });
        }

        if self.byte_end > content.len() {
            return Err(EditError::InvalidByteRange {
                byte_start: self.byte_start,
                byte_end: self.byte_end,
                file_len: content.len(),
            });
        }

        // Extract current text at span
        let current_bytes = &content[self.byte_start..self.byte_end];
        let current_text = std::str::from_utf8(current_bytes)?;

        // Check if already applied (idempotency)
        if current_text == self.new_text {
            return Ok(current_bytes);
        }

        // Verify expected before-text
        if !self.expected_before.matches(current_text) {
            return Err(EditError::BeforeTextMismatch {
                file: self.file.clone(),
                byte_start: self.byte_start,
                byte_end: self.byte_end,
                expected: format!("{:?}", self.expected_before),
                found: current_text.to_string(),
            });
        }

        Ok(current_bytes)
    }

    /// Apply this edit to the file system atomically.
    ///
    /// Uses tempfile + fsync + rename for crash safety.
    pub fn apply(&self) -> Result<EditResult, EditError> {
        // Read current file
        let original_content = fs::read(&self.file)?;

        // Validate edit
        let current_bytes = self.validate(&original_content)?;

        // Check idempotency
        if std::str::from_utf8(current_bytes)? == self.new_text {
            return Ok(EditResult::AlreadyApplied {
                file: self.file.clone(),
            });
        }

        // Build new content
        let mut new_content = Vec::with_capacity(
            original_content.len() + self.new_text.len() - (self.byte_end - self.byte_start),
        );
        new_content.extend_from_slice(&original_content[..self.byte_start]);
        new_content.extend_from_slice(self.new_text.as_bytes());
        new_content.extend_from_slice(&original_content[self.byte_end..]);

        // Validate resulting content is valid UTF-8
        std::str::from_utf8(&new_content).map_err(|_| EditError::InvalidUtf8Edit)?;

        // Atomic write: tempfile in same directory, fsync, rename
        atomic_write(&self.file, &new_content)?;

        // Update mtime to invalidate incremental compilation
        let now = filetime::FileTime::now();
        filetime::set_file_mtime(&self.file, now)?;

        Ok(EditResult::Applied {
            file: self.file.clone(),
            bytes_changed: self.new_text.len(),
        })
    }

    /// Apply multiple edits to the same file in a single atomic operation.
    ///
    /// Edits are sorted by byte_start descending and applied bottom-to-top
    /// to avoid offset invalidation.
    pub fn apply_batch(mut edits: Vec<Edit>) -> Result<Vec<EditResult>, EditError> {
        if edits.is_empty() {
            return Ok(Vec::new());
        }

        // Group by file
        edits.sort_by(|a, b| {
            a.file.cmp(&b.file).then(b.byte_start.cmp(&a.byte_start)) // Descending by byte_start
        });

        let mut results = Vec::with_capacity(edits.len());
        let mut current_file = None;
        let mut file_edits = Vec::new();

        for edit in edits {
            match &current_file {
                None => {
                    current_file = Some(edit.file.clone());
                    file_edits.push(edit);
                }
                Some(path) if path == &edit.file => {
                    file_edits.push(edit);
                }
                Some(_) => {
                    // File changed, apply accumulated edits
                    results.extend(apply_file_edits(&file_edits)?);
                    file_edits.clear();
                    current_file = Some(edit.file.clone());
                    file_edits.push(edit);
                }
            }
        }

        // Apply remaining edits
        if !file_edits.is_empty() {
            results.extend(apply_file_edits(&file_edits)?);
        }

        Ok(results)
    }
}

/// Apply multiple edits to a single file atomically.
///
/// Assumes edits are sorted by byte_start descending.
fn apply_file_edits(edits: &[Edit]) -> Result<Vec<EditResult>, EditError> {
    if edits.is_empty() {
        return Ok(Vec::new());
    }

    let file = &edits[0].file;
    let original_content = fs::read(file)?;

    // Validate all edits first
    for edit in edits {
        edit.validate(&original_content)?;
    }

    // Check for overlapping spans (edits are sorted descending by byte_start)
    // For non-overlapping regions: earlier edit's end <= later edit's start
    for window in edits.windows(2) {
        let (later, earlier) = (&window[0], &window[1]);
        if earlier.byte_end > later.byte_start {
            return Err(EditError::InvalidByteRange {
                byte_start: later.byte_start,
                byte_end: earlier.byte_end,
                file_len: original_content.len(),
            });
        }
    }

    // Apply edits bottom-to-top (already sorted descending)
    let mut new_content = original_content.clone();
    let mut results = Vec::with_capacity(edits.len());

    for edit in edits {
        let current_bytes = &new_content[edit.byte_start..edit.byte_end];
        let current_text = std::str::from_utf8(current_bytes)?;

        // Check idempotency
        if current_text == edit.new_text {
            results.push(EditResult::AlreadyApplied {
                file: edit.file.clone(),
            });
            continue;
        }

        // Splice in new text
        new_content.splice(
            edit.byte_start..edit.byte_end,
            edit.new_text.as_bytes().iter().copied(),
        );

        results.push(EditResult::Applied {
            file: edit.file.clone(),
            bytes_changed: edit.new_text.len(),
        });
    }

    // Validate resulting content is valid UTF-8
    std::str::from_utf8(&new_content).map_err(|_| EditError::InvalidUtf8Edit)?;

    // Atomic write
    atomic_write(file, &new_content)?;

    // Update mtime
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(file, now)?;

    Ok(results)
}

/// Atomic file write: tempfile + fsync + rename.
///
/// This ensures crash safety - either the full write succeeds or nothing changes.
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), EditError> {
    // Create tempfile in same directory to ensure same filesystem
    let parent = path.parent().ok_or_else(|| {
        EditError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Path has no parent directory",
        ))
    })?;

    let mut temp = tempfile::NamedTempFile::new_in(parent)?;

    // Write content
    temp.write_all(content)?;

    // Flush to disk (fsync)
    temp.as_file().sync_all()?;

    // Atomic rename
    temp.persist(path).map_err(|e| e.error)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_verification_exact_match() {
        let text = "hello world";
        let verify = EditVerification::ExactMatch(text.to_string());
        assert!(verify.matches(text));
        assert!(!verify.matches("hello"));
    }

    #[test]
    fn test_edit_verification_hash() {
        let text = "hello world";
        let hash = xxh3_64(text.as_bytes());
        let verify = EditVerification::Hash(hash);
        assert!(verify.matches(text));
        assert!(!verify.matches("goodbye world"));
    }

    #[test]
    fn test_edit_verification_from_text_small() {
        let text = "small";
        let verify = EditVerification::from_text(text);
        assert!(matches!(verify, EditVerification::ExactMatch(_)));
    }

    #[test]
    fn test_edit_verification_from_text_large() {
        let text = "x".repeat(2000);
        let verify = EditVerification::from_text(&text);
        assert!(matches!(verify, EditVerification::Hash(_)));
    }

    #[test]
    fn test_edit_validation_invalid_range() {
        let content = b"hello world";
        let edit = Edit::new("test.txt", 5, 20, "replacement", "");
        let result = edit.validate(content);
        assert!(matches!(result, Err(EditError::InvalidByteRange { .. })));
    }

    #[test]
    fn test_edit_validation_inverted_range() {
        let content = b"hello world";
        let edit = Edit::new("test.txt", 10, 5, "replacement", "");
        let result = edit.validate(content);
        assert!(matches!(result, Err(EditError::InvalidByteRange { .. })));
    }

    #[test]
    fn test_edit_idempotency_check() {
        let content = b"hello world";
        let edit = Edit::new("test.txt", 0, 5, "hello", "hello");
        let result = edit.validate(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_atomic_write_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"original content").unwrap();

        let edit = Edit::new(&file_path, 0, 8, "modified", "original");
        let result = edit.apply().unwrap();

        assert!(matches!(result, EditResult::Applied { .. }));
        let new_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_content, "modified content");
    }

    #[test]
    fn test_edit_idempotency_application() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"hello world").unwrap();

        let edit = Edit::new(&file_path, 0, 5, "hello", "hello");
        let result = edit.apply().unwrap();

        assert!(matches!(result, EditResult::AlreadyApplied { .. }));
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_batch_edits_same_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"line1\nline2\nline3\n").unwrap();

        let edits = vec![
            Edit::new(&file_path, 0, 5, "LINE1", "line1"),
            Edit::new(&file_path, 6, 11, "LINE2", "line2"),
            Edit::new(&file_path, 12, 17, "LINE3", "line3"),
        ];

        let results = Edit::apply_batch(edits).unwrap();
        assert_eq!(results.len(), 3);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "LINE1\nLINE2\nLINE3\n");
    }
}
