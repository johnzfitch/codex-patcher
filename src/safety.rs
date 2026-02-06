use std::path::{Path, PathBuf};
use thiserror::Error;

/// Workspace safety checks to prevent editing files outside the target workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceGuard {
    /// Absolute path to workspace root
    workspace_root: PathBuf,
    /// Canonical paths to forbidden directories
    forbidden_paths: Vec<PathBuf>,
}

#[derive(Error, Debug)]
pub enum SafetyError {
    #[error("Path is outside workspace: {path} (workspace: {workspace})")]
    OutsideWorkspace { path: PathBuf, workspace: PathBuf },

    #[error("Path is in forbidden directory: {path} (forbidden: {forbidden})")]
    ForbiddenPath { path: PathBuf, forbidden: PathBuf },

    #[error("Failed to canonicalize path: {0}")]
    Canonicalize(#[from] std::io::Error),
}

impl WorkspaceGuard {
    /// Create a new workspace guard with the given root.
    ///
    /// The workspace root will be canonicalized to handle symlinks correctly.
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, SafetyError> {
        let workspace_root = workspace_root.as_ref().canonicalize()?;

        // Build list of forbidden directories
        let mut forbidden_paths = Vec::new();

        // ~/.cargo/registry - dependency source code
        if let Some(home) = home::home_dir() {
            if let Ok(cargo_registry) = home.join(".cargo/registry").canonicalize() {
                forbidden_paths.push(cargo_registry);
            }
            if let Ok(cargo_git) = home.join(".cargo/git").canonicalize() {
                forbidden_paths.push(cargo_git);
            }
        }

        // ~/.rustup - toolchain installations
        if let Some(home) = home::home_dir() {
            if let Ok(rustup_home) = home.join(".rustup").canonicalize() {
                forbidden_paths.push(rustup_home);
            }
        }

        // target/ directory within workspace
        if let Ok(target_dir) = workspace_root.join("target").canonicalize() {
            forbidden_paths.push(target_dir);
        }

        Ok(Self {
            workspace_root,
            forbidden_paths,
        })
    }

    /// Check if a path is safe to edit.
    ///
    /// Returns the canonicalized absolute path if safe.
    ///
    /// Note: This performs canonicalization at validation time. For maximum
    /// TOCTOU safety, callers should hold an open fd or re-validate immediately
    /// before write operations in adversarial environments.
    pub fn validate_path(&self, path: impl AsRef<Path>) -> Result<PathBuf, SafetyError> {
        let path = path.as_ref();

        // Resolve relative paths against workspace root
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };

        // Canonicalize to resolve symlinks and .. components
        let canonical = absolute.canonicalize()?;

        self.check_canonical(&canonical)?;

        Ok(canonical)
    }

    /// Re-validate a previously-validated canonical path.
    ///
    /// Call this immediately before write to close the TOCTOU window:
    /// the path is re-canonicalized and re-checked against workspace
    /// and forbidden boundaries.
    pub fn revalidate(&self, path: &Path) -> Result<PathBuf, SafetyError> {
        let canonical = path.canonicalize()?;
        self.check_canonical(&canonical)?;
        Ok(canonical)
    }

    fn check_canonical(&self, canonical: &Path) -> Result<(), SafetyError> {
        // Check if inside workspace
        if !canonical.starts_with(&self.workspace_root) {
            return Err(SafetyError::OutsideWorkspace {
                path: canonical.to_path_buf(),
                workspace: self.workspace_root.clone(),
            });
        }

        // Check against forbidden paths
        for forbidden in &self.forbidden_paths {
            if canonical.starts_with(forbidden) {
                return Err(SafetyError::ForbiddenPath {
                    path: canonical.to_path_buf(),
                    forbidden: forbidden.clone(),
                });
            }
        }

        Ok(())
    }

    /// Get the workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Create a guard with custom forbidden paths (for testing).
    #[cfg(test)]
    pub fn with_forbidden(
        workspace_root: impl AsRef<Path>,
        forbidden: Vec<PathBuf>,
    ) -> Result<Self, SafetyError> {
        let workspace_root = workspace_root.as_ref().canonicalize()?;
        Ok(Self {
            workspace_root,
            forbidden_paths: forbidden,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_validate_path_inside_workspace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path();
        let guard = WorkspaceGuard::new(workspace).unwrap();

        let file = workspace.join("src/main.rs");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"").unwrap();

        let result = guard.validate_path(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_outside_workspace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let guard = WorkspaceGuard::new(&workspace).unwrap();

        let outside = temp_dir.path().join("outside.rs");
        fs::write(&outside, b"").unwrap();

        let result = guard.validate_path(&outside);
        assert!(matches!(result, Err(SafetyError::OutsideWorkspace { .. })));
    }

    #[test]
    fn test_validate_path_forbidden() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path();
        let forbidden = workspace.join("target");
        fs::create_dir_all(&forbidden).unwrap();

        let guard = WorkspaceGuard::with_forbidden(workspace, vec![forbidden.clone()]).unwrap();

        let file = forbidden.join("debug/binary");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"").unwrap();

        let result = guard.validate_path(&file);
        assert!(matches!(result, Err(SafetyError::ForbiddenPath { .. })));
    }

    #[test]
    fn test_validate_relative_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path();
        let guard = WorkspaceGuard::new(workspace).unwrap();

        let file = workspace.join("test.rs");
        fs::write(&file, b"").unwrap();

        // Validate relative path
        let result = guard.validate_path("test.rs");
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let outside = temp_dir.path().join("outside.rs");
        fs::write(&outside, b"").unwrap();

        let link = workspace.join("escape.rs");
        symlink(&outside, &link).unwrap();

        let guard = WorkspaceGuard::new(&workspace).unwrap();
        let result = guard.validate_path(&link);

        // Should reject because canonical path is outside workspace
        assert!(matches!(result, Err(SafetyError::OutsideWorkspace { .. })));
    }
}
