use crate::toml::errors::TomlError;
use crate::toml::query::SectionPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constraints {
    pub ensure_absent: bool,
    pub ensure_present: bool,
}

impl Constraints {
    pub fn none() -> Self {
        Self {
            ensure_absent: false,
            ensure_present: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Positioning {
    AfterSection(SectionPath),
    BeforeSection(SectionPath),
    AtEnd,
    AtBeginning,
}

impl Positioning {
    pub fn resolve(
        after_section: Option<SectionPath>,
        before_section: Option<SectionPath>,
        at_end: bool,
        at_beginning: bool,
    ) -> Result<Self, TomlError> {
        let mut count = 0;
        if after_section.is_some() {
            count += 1;
        }
        if before_section.is_some() {
            count += 1;
        }
        if at_end {
            count += 1;
        }
        if at_beginning {
            count += 1;
        }
        if count > 1 {
            return Err(TomlError::InvalidPositioning {
                message: "only one positioning directive is allowed".to_string(),
            });
        }
        if let Some(path) = after_section {
            return Ok(Positioning::AfterSection(path));
        }
        if let Some(path) = before_section {
            return Ok(Positioning::BeforeSection(path));
        }
        if at_beginning {
            return Ok(Positioning::AtBeginning);
        }
        Ok(Positioning::AtEnd)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlOperation {
    InsertSection {
        text: String,
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
}
