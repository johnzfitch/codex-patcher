use crate::toml::errors::TomlError;
use toml_edit::DocumentMut;

pub fn validate_document(content: &str) -> Result<(), TomlError> {
    content
        .parse::<DocumentMut>()
        .map_err(|err| TomlError::InvalidTomlSyntax {
            message: err.to_string(),
        })?;
    Ok(())
}
