use crate::toml::errors::TomlError;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SectionPath {
    parts: Vec<String>,
}

impl SectionPath {
    pub fn new(parts: Vec<String>) -> Result<Self, TomlError> {
        if parts.is_empty() {
            return Err(TomlError::InvalidSectionPath {
                input: "".to_string(),
                message: "empty section path".to_string(),
            });
        }
        Ok(Self { parts })
    }

    pub fn parse(input: &str) -> Result<Self, TomlError> {
        let parts = parse_dotted_path(input)?;
        if parts.is_empty() {
            return Err(TomlError::InvalidSectionPath {
                input: input.to_string(),
                message: "empty section path".to_string(),
            });
        }
        Ok(Self { parts })
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn as_string(&self) -> String {
        self.parts.join(".")
    }
}

impl fmt::Display for SectionPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPath {
    parts: Vec<String>,
}

impl KeyPath {
    pub fn parse(input: &str) -> Result<Self, TomlError> {
        let parts = parse_dotted_path(input)?;
        if parts.is_empty() {
            return Err(TomlError::InvalidSectionPath {
                input: input.to_string(),
                message: "empty key path".to_string(),
            });
        }
        Ok(Self { parts })
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn as_string(&self) -> String {
        self.parts.join(".")
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlQuery {
    Section { path: SectionPath },
    Key { section: SectionPath, key: KeyPath },
}

fn parse_dotted_path(input: &str) -> Result<Vec<String>, TomlError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
                continue;
            }

            if quote_char == '"' && ch == '\\' {
                if let Some(next) = chars.next() {
                    let escaped = match next {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    };
                    current.push(escaped);
                    continue;
                }
            }

            current.push(ch);
            continue;
        }

        match ch {
            '.' => {
                if current.is_empty() {
                    return Err(TomlError::InvalidSectionPath {
                        input: input.to_string(),
                        message: "empty path segment".to_string(),
                    });
                }
                parts.push(current.clone());
                current.clear();
            }
            '"' | '\'' => {
                if !current.is_empty() {
                    return Err(TomlError::InvalidSectionPath {
                        input: input.to_string(),
                        message: "unexpected quote inside key".to_string(),
                    });
                }
                in_quotes = true;
                quote_char = ch;
            }
            ch if ch.is_whitespace() => {
                return Err(TomlError::InvalidSectionPath {
                    input: input.to_string(),
                    message: "whitespace not allowed in key".to_string(),
                });
            }
            other => current.push(other),
        }
    }

    if in_quotes {
        return Err(TomlError::InvalidSectionPath {
            input: input.to_string(),
            message: "unterminated quoted key".to_string(),
        });
    }

    if !current.is_empty() {
        parts.push(current);
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_section_path_basic() {
        let path = SectionPath::parse("profile.zack").unwrap();
        assert_eq!(path.parts(), &["profile", "zack"]);
    }

    #[test]
    fn parse_section_path_quoted() {
        let path = SectionPath::parse("profile.\"zack.test\"").unwrap();
        assert_eq!(path.parts(), &["profile", "zack.test"]);
    }

    #[test]
    fn parse_key_path_dotted() {
        let key = KeyPath::parse("target.x86_64").unwrap();
        assert_eq!(key.parts(), &["target", "x86_64"]);
    }
}
