use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct PatchConfig {
    #[serde(default)]
    pub meta: Metadata,
    #[serde(default)]
    pub patches: Vec<PatchDefinition>,
}

impl PatchConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Vec::new();

        if self.patches.is_empty() {
            issues.push(ValidationIssue::EmptyPatchList);
        }

        for patch in &self.patches {
            if patch.id.trim().is_empty() {
                issues.push(ValidationIssue::MissingField {
                    patch_id: None,
                    field: "id",
                });
            }
            if patch.file.trim().is_empty() {
                issues.push(ValidationIssue::MissingField {
                    patch_id: Some(patch.id.clone()),
                    field: "file",
                });
            }

            match &patch.query {
                Query::Toml {
                    section,
                    key,
                    ensure_absent,
                    ensure_present,
                } => {
                    if section.as_deref().unwrap_or("").is_empty() && key.is_none() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "query.section",
                        });
                    }
                    if section.is_none() && key.is_some() {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message: "toml query with key requires section".to_string(),
                        });
                    }
                    if *ensure_absent && *ensure_present {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message: "ensure_absent and ensure_present cannot both be true"
                                .to_string(),
                        });
                    }
                }
                Query::AstGrep { pattern } | Query::TreeSitter { pattern } => {
                    if pattern.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "query.pattern",
                        });
                    }
                }
                Query::Text { search } => {
                    if search.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "query.search",
                        });
                    }
                }
            }

            match &patch.operation {
                Operation::InsertSection { text, positioning } => {
                    if text.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.text",
                        });
                    }
                    if let Err(message) = positioning.validate() {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message,
                        });
                    }
                }
                Operation::AppendSection { text } => {
                    if text.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.text",
                        });
                    }
                }
                Operation::ReplaceValue { value } => {
                    if value.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.value",
                        });
                    }
                    if !patch.query.is_key_query() {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message: "replace_value requires toml key query".to_string(),
                        });
                    }
                }
                Operation::ReplaceKey { new_key } => {
                    if new_key.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.new_key",
                        });
                    }
                    if !patch.query.is_key_query() {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message: "replace_key requires toml key query".to_string(),
                        });
                    }
                }
                Operation::DeleteSection => {
                    if !patch.query.is_section_query() {
                        issues.push(ValidationIssue::InvalidCombo {
                            patch_id: Some(patch.id.clone()),
                            message: "delete_section requires toml section query".to_string(),
                        });
                    }
                }
                Operation::Replace { text } => {
                    if text.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.text",
                        });
                    }
                }
                Operation::ReplaceCapture { capture, text } => {
                    if capture.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.capture",
                        });
                    }
                    if text.trim().is_empty() {
                        issues.push(ValidationIssue::MissingField {
                            patch_id: Some(patch.id.clone()),
                            field: "operation.text",
                        });
                    }
                }
                Operation::Delete { insert_comment: _ } => {}
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { issues })
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Metadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version_range: Option<String>,
    #[serde(default)]
    pub workspace_relative: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PatchDefinition {
    pub id: String,
    pub file: String,
    pub query: Query,
    pub operation: Operation,
    #[serde(default)]
    pub verify: Option<Verify>,
    #[serde(default)]
    pub constraint: Option<Constraints>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Query {
    Toml {
        #[serde(default)]
        section: Option<String>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        ensure_absent: bool,
        #[serde(default)]
        ensure_present: bool,
    },
    AstGrep {
        pattern: String,
    },
    TreeSitter {
        pattern: String,
    },
    /// Simple text search - finds exact string match
    Text {
        /// The exact text to search for
        search: String,
    },
}

impl Query {
    pub fn is_key_query(&self) -> bool {
        matches!(self, Query::Toml { key: Some(_), .. })
    }

    pub fn is_section_query(&self) -> bool {
        matches!(
            self,
            Query::Toml {
                section: Some(_),
                ..
            }
        )
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Operation {
    InsertSection {
        text: String,
        #[serde(flatten)]
        positioning: Positioning,
    },
    AppendSection {
        text: String,
    },
    ReplaceValue {
        value: String,
    },
    DeleteSection,
    ReplaceKey {
        new_key: String,
    },
    Replace {
        text: String,
    },
    ReplaceCapture {
        capture: String,
        text: String,
    },
    Delete {
        #[serde(default)]
        insert_comment: Option<String>,
    },
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Positioning {
    #[serde(default)]
    pub after_section: Option<String>,
    #[serde(default)]
    pub before_section: Option<String>,
    #[serde(default)]
    pub at_end: bool,
    #[serde(default)]
    pub at_beginning: bool,
}

impl Positioning {
    pub fn validate(&self) -> Result<(), String> {
        let mut count = 0;
        if self.after_section.is_some() {
            count += 1;
        }
        if self.before_section.is_some() {
            count += 1;
        }
        if self.at_end {
            count += 1;
        }
        if self.at_beginning {
            count += 1;
        }
        if count > 1 {
            return Err("only one positioning directive is allowed".to_string());
        }
        Ok(())
    }

    pub fn relative_position(&self) -> RelativePosition {
        if let Some(path) = &self.after_section {
            return RelativePosition::After(path.clone());
        }
        if let Some(path) = &self.before_section {
            return RelativePosition::Before(path.clone());
        }
        if self.at_beginning {
            return RelativePosition::AtBeginning;
        }
        RelativePosition::AtEnd
    }
}

#[derive(Debug, Deserialize, Clone)]
pub enum RelativePosition {
    After(String),
    Before(String),
    AtEnd,
    AtBeginning,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Constraints {
    #[serde(default)]
    pub ensure_absent: bool,
    #[serde(default)]
    pub ensure_present: bool,
    #[serde(default)]
    pub function_context: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Verify {
    ExactMatch {
        expected_text: String,
    },
    Hash {
        algorithm: Option<HashAlgorithm>,
        expected: String,
    },
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HashAlgorithm {
    Xxh3,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, issue) in self.issues.iter().enumerate() {
            if idx > 0 {
                writeln!(f)?;
            }
            write!(f, "{issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone)]
pub enum ValidationIssue {
    EmptyPatchList,
    MissingField {
        patch_id: Option<String>,
        field: &'static str,
    },
    InvalidCombo {
        patch_id: Option<String>,
        message: String,
    },
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::EmptyPatchList => write!(f, "patch config contains no patches"),
            ValidationIssue::MissingField { patch_id, field } => match patch_id {
                Some(id) => write!(f, "patch '{id}' missing required field '{field}'"),
                None => write!(f, "patch missing required field '{field}'"),
            },
            ValidationIssue::InvalidCombo { patch_id, message } => match patch_id {
                Some(id) => write!(f, "patch '{id}' has invalid configuration: {message}"),
                None => write!(f, "invalid patch configuration: {message}"),
            },
        }
    }
}
