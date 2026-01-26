use crate::config::schema::{PatchConfig, ValidationError};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Toml {
        path: Option<PathBuf>,
        source: toml_edit::de::Error,
    },
    Validation {
        path: Option<PathBuf>,
        source: ValidationError,
    },
}

impl ConfigError {
    fn with_path(self, path: &Path) -> Self {
        let path = path.to_path_buf();
        match self {
            ConfigError::Io { .. } => self,
            ConfigError::Toml { path: None, source } => ConfigError::Toml {
                path: Some(path),
                source,
            },
            ConfigError::Validation { path: None, source } => ConfigError::Validation {
                path: Some(path),
                source,
            },
            other => other,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(
                    f,
                    "failed to read patch config from {}: {}",
                    path.display(),
                    source
                )
            }
            ConfigError::Toml { path, source } => match path {
                Some(path) => write!(
                    f,
                    "failed to parse patch config TOML ({}): {}",
                    path.display(),
                    source
                ),
                None => write!(f, "failed to parse patch config TOML: {}", source),
            },
            ConfigError::Validation { path, source } => match path {
                Some(path) => write!(f, "invalid patch config ({}): {}", path.display(), source),
                None => write!(f, "invalid patch config: {}", source),
            },
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Toml { source, .. } => Some(source),
            ConfigError::Validation { source, .. } => Some(source),
        }
    }
}

pub fn load_from_str(input: &str) -> Result<PatchConfig, ConfigError> {
    let config: PatchConfig = toml_edit::de::from_str(input)
        .map_err(|source| ConfigError::Toml { path: None, source })?;
    config
        .validate()
        .map_err(|source| ConfigError::Validation { path: None, source })?;
    Ok(config)
}

pub fn load_from_path(path: impl AsRef<Path>) -> Result<PatchConfig, ConfigError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    load_from_str(&contents).map_err(|error| error.with_path(path))
}
