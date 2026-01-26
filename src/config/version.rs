//! Version filtering for patches using semver constraints
//!
//! Allows patches to specify version ranges like ">=0.88.0, <0.90.0"
//! and filters them based on the workspace version.

use semver::{Version, VersionReq};
use std::fmt;

/// Errors during version filtering
#[derive(Debug, Clone)]
pub enum VersionError {
    /// Invalid version string (e.g., "not-a-version")
    InvalidVersion { value: String, source: String },
    /// Invalid version requirement (e.g., ">=bad")
    InvalidRequirement { value: String, source: String },
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::InvalidVersion { value, source } => {
                write!(f, "invalid version '{}': {}", value, source)
            }
            VersionError::InvalidRequirement { value, source } => {
                write!(f, "invalid version requirement '{}': {}", value, source)
            }
        }
    }
}

impl std::error::Error for VersionError {}

/// Check if a version matches a requirement string
///
/// # Examples
///
/// ```
/// use codex_patcher::config::version::matches_requirement;
///
/// assert!(matches_requirement("0.88.0", Some(">=0.88.0")).unwrap());
/// assert!(matches_requirement("0.89.0", Some(">=0.88.0, <0.90.0")).unwrap());
/// assert!(!matches_requirement("0.87.0", Some(">=0.88.0")).unwrap());
///
/// // None requirement means "apply to all versions"
/// assert!(matches_requirement("1.0.0", None).unwrap());
/// ```
pub fn matches_requirement(
    version: &str,
    requirement: Option<&str>,
) -> Result<bool, VersionError> {
    // No requirement means "apply to all versions"
    let Some(req_str) = requirement else {
        return Ok(true);
    };

    // Empty requirement string means "apply to all versions"
    let req_str = req_str.trim();
    if req_str.is_empty() {
        return Ok(true);
    }

    // Parse version
    let version = Version::parse(version).map_err(|e| VersionError::InvalidVersion {
        value: version.to_string(),
        source: e.to_string(),
    })?;

    // Parse requirement
    let req = VersionReq::parse(req_str).map_err(|e| VersionError::InvalidRequirement {
        value: req_str.to_string(),
        source: e.to_string(),
    })?;

    Ok(req.matches(&version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_requirement() {
        assert!(matches_requirement("0.88.0", None).unwrap());
        assert!(matches_requirement("1.0.0", None).unwrap());
        assert!(matches_requirement("0.1.0", None).unwrap());
    }

    #[test]
    fn test_empty_requirement() {
        assert!(matches_requirement("0.88.0", Some("")).unwrap());
        assert!(matches_requirement("1.0.0", Some("   ")).unwrap());
    }

    #[test]
    fn test_simple_requirement() {
        // Exact version
        assert!(matches_requirement("0.88.0", Some("=0.88.0")).unwrap());
        assert!(!matches_requirement("0.88.1", Some("=0.88.0")).unwrap());

        // Greater than or equal
        assert!(matches_requirement("0.88.0", Some(">=0.88.0")).unwrap());
        assert!(matches_requirement("0.89.0", Some(">=0.88.0")).unwrap());
        assert!(!matches_requirement("0.87.0", Some(">=0.88.0")).unwrap());

        // Less than
        assert!(matches_requirement("0.87.0", Some("<0.88.0")).unwrap());
        assert!(!matches_requirement("0.88.0", Some("<0.88.0")).unwrap());
    }

    #[test]
    fn test_compound_requirement() {
        let req = ">=0.88.0, <0.90.0";

        assert!(matches_requirement("0.88.0", Some(req)).unwrap());
        assert!(matches_requirement("0.89.0", Some(req)).unwrap());
        assert!(matches_requirement("0.89.5", Some(req)).unwrap());
        assert!(!matches_requirement("0.87.0", Some(req)).unwrap());
        assert!(!matches_requirement("0.90.0", Some(req)).unwrap());
        assert!(!matches_requirement("1.0.0", Some(req)).unwrap());
    }

    #[test]
    fn test_caret_requirement() {
        // ^0.88 means >=0.88.0, <0.89.0
        let req = "^0.88";
        assert!(matches_requirement("0.88.0", Some(req)).unwrap());
        assert!(matches_requirement("0.88.5", Some(req)).unwrap());
        assert!(!matches_requirement("0.89.0", Some(req)).unwrap());
        assert!(!matches_requirement("0.87.0", Some(req)).unwrap());
    }

    #[test]
    fn test_tilde_requirement() {
        // ~0.88.0 means >=0.88.0, <0.89.0
        let req = "~0.88.0";
        assert!(matches_requirement("0.88.0", Some(req)).unwrap());
        assert!(matches_requirement("0.88.9", Some(req)).unwrap());
        assert!(!matches_requirement("0.89.0", Some(req)).unwrap());
    }

    #[test]
    fn test_invalid_version() {
        let result = matches_requirement("not-a-version", Some(">=0.88.0"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), VersionError::InvalidVersion { .. }));
    }

    #[test]
    fn test_invalid_requirement() {
        let result = matches_requirement("0.88.0", Some(">=bad-version"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), VersionError::InvalidRequirement { .. }));
    }

    #[test]
    fn test_prerelease_versions() {
        let req = ">=0.88.0-alpha.4";
        assert!(matches_requirement("0.88.0-alpha.4", Some(req)).unwrap());
        assert!(matches_requirement("0.88.0-alpha.5", Some(req)).unwrap());
        assert!(matches_requirement("0.88.0", Some(req)).unwrap());
        assert!(!matches_requirement("0.88.0-alpha.3", Some(req)).unwrap());
    }
}
