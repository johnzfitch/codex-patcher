use crate::edit::{Edit, EditVerification};
use crate::toml::errors::TomlError;
use crate::toml::operations::{Constraints, Positioning, TomlOperation};
use crate::toml::query::{KeyPath, SectionPath, TomlQuery};
use crate::toml::validator::validate_document;
use toml_edit::DocumentMut;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlPlan {
    Edit(Edit),
    NoOp(String),
}

#[derive(Debug, Clone)]
struct SectionInfo {
    path: SectionPath,
    header_start: usize,
    header_line_end: usize,
    body_start: usize,
    body_end: usize,
}

pub struct TomlEditor {
    file: std::path::PathBuf,
    content: String,
    _document: DocumentMut,
    sections: Vec<SectionInfo>,
}

impl TomlEditor {
    pub fn parse(content: &str) -> Result<Self, TomlError> {
        Self::from_path("<toml-buffer>", content)
    }

    pub fn from_path(
        path: impl Into<std::path::PathBuf>,
        content: &str,
    ) -> Result<Self, TomlError> {
        let document =
            content
                .parse::<DocumentMut>()
                .map_err(|err| TomlError::InvalidTomlSyntax {
                    message: err.to_string(),
                })?;
        let sections = scan_sections(content)?;
        Ok(Self {
            file: path.into(),
            content: content.to_string(),
            _document: document,
            sections,
        })
    }

    pub fn plan(
        &self,
        query: &TomlQuery,
        operation: &TomlOperation,
        constraints: Constraints,
    ) -> Result<TomlPlan, TomlError> {
        match operation {
            TomlOperation::InsertSection { text, positioning } => {
                self.plan_insert_section(query, text, positioning, constraints)
            }
            TomlOperation::AppendSection { text } => self.plan_append_section(query, text),
            TomlOperation::ReplaceValue { value } => {
                self.plan_replace_value(query, value, constraints)
            }
            TomlOperation::DeleteSection => self.plan_delete_section(query, constraints),
            TomlOperation::ReplaceKey { new_key } => {
                self.plan_replace_key(query, new_key, constraints)
            }
        }
    }

    fn plan_insert_section(
        &self,
        query: &TomlQuery,
        text: &str,
        positioning: &Positioning,
        constraints: Constraints,
    ) -> Result<TomlPlan, TomlError> {
        let section = match query {
            TomlQuery::Section { path } => path,
            TomlQuery::Key { section, .. } => section,
        };

        if constraints.ensure_absent && self.find_section(section).is_ok() {
            return Ok(TomlPlan::NoOp(format!(
                "section already present: {}",
                section
            )));
        }

        validate_section_snippet(text)?;

        let insert_point = self.resolve_insertion(positioning)?;
        let (byte_start, byte_end, expected_before) = insertion_anchor(&self.content, insert_point);
        let mut new_text = normalize_insertion(text, &self.content, byte_start, &expected_before);

        if !expected_before.is_empty() {
            new_text.push_str(&expected_before);
        }

        let edit = Edit::with_verification(
            self.file.clone(),
            byte_start,
            byte_end,
            new_text,
            EditVerification::from_text(&expected_before),
        );

        self.validate_edit(&edit)?;
        Ok(TomlPlan::Edit(edit))
    }

    fn plan_append_section(&self, query: &TomlQuery, text: &str) -> Result<TomlPlan, TomlError> {
        let section = match query {
            TomlQuery::Section { path } => path,
            TomlQuery::Key { section, .. } => section,
        };

        if self.find_section(section).is_ok() {
            return Ok(TomlPlan::NoOp(format!(
                "section already present: {}",
                section
            )));
        }

        validate_section_snippet(text)?;

        let insert_point = InsertionPoint {
            anchor_start: self.content.len(),
            anchor_end: self.content.len(),
            anchor_text: String::new(),
        };
        let (byte_start, byte_end, expected_before) = insertion_anchor(&self.content, insert_point);
        let mut new_text = normalize_insertion(text, &self.content, byte_start, &expected_before);

        if !expected_before.is_empty() {
            new_text.push_str(&expected_before);
        }

        let edit = Edit::with_verification(
            self.file.clone(),
            byte_start,
            byte_end,
            new_text,
            EditVerification::from_text(&expected_before),
        );

        self.validate_edit(&edit)?;
        Ok(TomlPlan::Edit(edit))
    }

    fn plan_replace_value(
        &self,
        query: &TomlQuery,
        value: &str,
        constraints: Constraints,
    ) -> Result<TomlPlan, TomlError> {
        let (section, key) = match query {
            TomlQuery::Key { section, key } => (section, key),
            TomlQuery::Section { .. } => {
                return Err(TomlError::InvalidPositioning {
                    message: "replace_value requires a key query".to_string(),
                });
            }
        };

        validate_value_snippet(value)?;

        let section_info = match self.find_section(section) {
            Ok(info) => info,
            Err(err) => {
                if constraints.ensure_present {
                    return Err(err);
                }
                return Ok(TomlPlan::NoOp(format!("section missing: {}", section)));
            }
        };

        let key_span = match find_key_span(&self.content, section_info, key) {
            Ok(span) => span,
            Err(err) => {
                if constraints.ensure_present {
                    return Err(err);
                }
                return Ok(TomlPlan::NoOp(format!("key missing: {}.{}", section, key)));
            }
        };

        let current = &self.content[key_span.start..key_span.end];
        if current.trim() == value.trim() {
            return Ok(TomlPlan::NoOp(format!(
                "value already matches: {}.{}",
                section, key
            )));
        }

        let edit = Edit::with_verification(
            self.file.clone(),
            key_span.start,
            key_span.end,
            value.to_string(),
            EditVerification::from_text(current),
        );

        self.validate_edit(&edit)?;
        Ok(TomlPlan::Edit(edit))
    }

    fn plan_replace_key(
        &self,
        query: &TomlQuery,
        new_key: &str,
        constraints: Constraints,
    ) -> Result<TomlPlan, TomlError> {
        let (section, key) = match query {
            TomlQuery::Key { section, key } => (section, key),
            TomlQuery::Section { .. } => {
                return Err(TomlError::InvalidPositioning {
                    message: "replace_key requires a key query".to_string(),
                });
            }
        };

        let _ = KeyPath::parse(new_key)?;

        let section_info = match self.find_section(section) {
            Ok(info) => info,
            Err(err) => {
                if constraints.ensure_present {
                    return Err(err);
                }
                return Ok(TomlPlan::NoOp(format!("section missing: {}", section)));
            }
        };

        let key_span = match find_key_span(&self.content, section_info, key) {
            Ok(span) => span,
            Err(err) => {
                if constraints.ensure_present {
                    return Err(err);
                }
                return Ok(TomlPlan::NoOp(format!("key missing: {}.{}", section, key)));
            }
        };

        let current = &self.content[key_span.key_start..key_span.key_end];
        if current.trim() == new_key.trim() {
            return Ok(TomlPlan::NoOp(format!(
                "key already matches: {}.{}",
                section, key
            )));
        }

        let edit = Edit::with_verification(
            self.file.clone(),
            key_span.key_start,
            key_span.key_end,
            new_key.to_string(),
            EditVerification::from_text(current),
        );

        self.validate_edit(&edit)?;
        Ok(TomlPlan::Edit(edit))
    }

    fn plan_delete_section(
        &self,
        query: &TomlQuery,
        constraints: Constraints,
    ) -> Result<TomlPlan, TomlError> {
        let section = match query {
            TomlQuery::Section { path } => path,
            TomlQuery::Key { section, .. } => section,
        };

        let section_info = match self.find_section(section) {
            Ok(info) => info,
            Err(err) => {
                if constraints.ensure_present {
                    return Err(err);
                }
                return Ok(TomlPlan::NoOp(format!("section missing: {}", section)));
            }
        };

        let current = &self.content[section_info.header_start..section_info.body_end];
        let edit = Edit::with_verification(
            self.file.clone(),
            section_info.header_start,
            section_info.body_end,
            String::new(),
            EditVerification::from_text(current),
        );

        self.validate_edit(&edit)?;
        Ok(TomlPlan::Edit(edit))
    }

    fn find_section(&self, path: &SectionPath) -> Result<&SectionInfo, TomlError> {
        let matches: Vec<&SectionInfo> = self
            .sections
            .iter()
            .filter(|section| section.path == *path)
            .collect();

        match matches.len() {
            0 => Err(TomlError::SectionNotFound {
                path: path.as_string(),
            }),
            1 => Ok(matches[0]),
            _ => Err(TomlError::AmbiguousMatch {
                kind: "section".to_string(),
                path: path.as_string(),
            }),
        }
    }

    fn resolve_insertion(&self, positioning: &Positioning) -> Result<InsertionPoint, TomlError> {
        match positioning {
            Positioning::AfterSection(path) => {
                let section = self.find_section(path)?;
                if let Some(next) = self.next_section(section) {
                    Ok(InsertionPoint {
                        anchor_start: next.header_start,
                        anchor_end: next.header_line_end,
                        anchor_text: self.content[next.header_start..next.header_line_end]
                            .to_string(),
                    })
                } else {
                    Ok(InsertionPoint {
                        anchor_start: self.content.len(),
                        anchor_end: self.content.len(),
                        anchor_text: String::new(),
                    })
                }
            }
            Positioning::BeforeSection(path) => {
                let section = self.find_section(path)?;
                Ok(InsertionPoint {
                    anchor_start: section.header_start,
                    anchor_end: section.header_line_end,
                    anchor_text: self.content[section.header_start..section.header_line_end]
                        .to_string(),
                })
            }
            Positioning::AtEnd => Ok(InsertionPoint {
                anchor_start: self.content.len(),
                anchor_end: self.content.len(),
                anchor_text: String::new(),
            }),
            Positioning::AtBeginning => {
                if let Some(first) = self.sections.first() {
                    Ok(InsertionPoint {
                        anchor_start: first.header_start,
                        anchor_end: first.header_line_end,
                        anchor_text: self.content[first.header_start..first.header_line_end]
                            .to_string(),
                    })
                } else {
                    Ok(InsertionPoint {
                        anchor_start: 0,
                        anchor_end: 0,
                        anchor_text: String::new(),
                    })
                }
            }
        }
    }

    fn next_section(&self, current: &SectionInfo) -> Option<&SectionInfo> {
        self.sections
            .iter()
            .filter(|section| section.header_start > current.header_start)
            .min_by_key(|section| section.header_start)
    }

    fn validate_edit(&self, edit: &Edit) -> Result<(), TomlError> {
        let mut updated = String::with_capacity(
            self.content.len() + edit.new_text.len() - (edit.byte_end - edit.byte_start),
        );
        updated.push_str(&self.content[..edit.byte_start]);
        updated.push_str(&edit.new_text);
        updated.push_str(&self.content[edit.byte_end..]);

        validate_document(&updated)?;

        Ok(())
    }

    /// Check if a section exists
    pub fn section_exists(&self, path: &str) -> bool {
        if let Ok(section_path) = SectionPath::parse(path) {
            self.find_section(&section_path).is_ok()
        } else {
            false
        }
    }

    /// Get the value of a key in a section
    pub fn get_value(&self, section: Option<&str>, key: &str) -> Option<String> {
        let section_path = if let Some(s) = section {
            SectionPath::parse(s).ok()?
        } else {
            return None;
        };

        let key_path = KeyPath::parse(key).ok()?;
        let section_info = self.find_section(&section_path).ok()?;
        let key_span = find_key_span(&self.content, section_info, &key_path).ok()?;
        Some(self.content[key_span.start..key_span.end].to_string())
    }

    /// Create a new TomlEditor without a file path (for testing/memory operations)
    pub fn new(content: &str) -> Result<Self, TomlError> {
        Self::parse(content)
    }
}

#[derive(Debug, Clone)]
struct InsertionPoint {
    anchor_start: usize,
    anchor_end: usize,
    anchor_text: String,
}

fn insertion_anchor(content: &str, point: InsertionPoint) -> (usize, usize, String) {
    if point.anchor_text.is_empty() {
        return (point.anchor_start, point.anchor_end, String::new());
    }

    if content.get(point.anchor_start..point.anchor_end) == Some(point.anchor_text.as_str()) {
        (point.anchor_start, point.anchor_end, point.anchor_text)
    } else {
        (point.anchor_start, point.anchor_start, String::new())
    }
}

fn normalize_insertion(
    text: &str,
    content: &str,
    byte_start: usize,
    expected_before: &str,
) -> String {
    let mut result = text.to_string();
    if !result.ends_with('\n') {
        result.push('\n');
    }

    if byte_start > 0 {
        let before = &content[..byte_start];
        let insert_is_section = result.trim_start().starts_with('[');
        let mut needed = if insert_is_section {
            if before.ends_with("\n\n") {
                0
            } else if before.ends_with('\n') {
                1
            } else {
                2
            }
        } else if before.ends_with('\n') {
            0
        } else {
            1
        };

        while needed > 0 {
            result.insert(0, '\n');
            needed -= 1;
        }
    }

    if expected_before.trim_start().starts_with('[') && !result.ends_with("\n\n") {
        result.push('\n');
    }

    result
}

fn validate_section_snippet(text: &str) -> Result<(), TomlError> {
    validate_document(text)
}

fn validate_value_snippet(value: &str) -> Result<(), TomlError> {
    let snippet = format!("key = {value}");
    validate_document(&snippet)
}

#[derive(Debug, Clone)]
struct KeySpan {
    key_start: usize,
    key_end: usize,
    start: usize,
    end: usize,
}

fn find_key_span(
    content: &str,
    section: &SectionInfo,
    key: &KeyPath,
) -> Result<KeySpan, TomlError> {
    let region = &content[section.body_start..section.body_end];
    let mut offset = section.body_start;
    let mut matches = Vec::new();

    for line in region.split_inclusive('\n') {
        let line_range_start = offset;
        let line_range_end = offset + line.len();
        offset = line_range_end;

        let line_trimmed = line.trim_start();
        if line_trimmed.is_empty() || line_trimmed.starts_with('#') {
            continue;
        }
        if line_trimmed.starts_with('[') {
            continue;
        }

        if let Some(span) = parse_key_line(line, line_range_start)? {
            if span.key_path == *key {
                matches.push(span);
            }
        }
    }

    match matches.len() {
        0 => Err(TomlError::KeyNotFound {
            section: section.path.as_string(),
            key: key.as_string(),
        }),
        1 => Ok(KeySpan {
            key_start: matches[0].key_start,
            key_end: matches[0].key_end,
            start: matches[0].value_start,
            end: matches[0].value_end,
        }),
        _ => Err(TomlError::AmbiguousMatch {
            kind: "key".to_string(),
            path: format!("{}.{}", section.path, key),
        }),
    }
}

#[derive(Debug, Clone)]
struct ParsedKeyLine {
    key_path: KeyPath,
    key_start: usize,
    key_end: usize,
    value_start: usize,
    value_end: usize,
}

fn parse_key_line(line: &str, line_offset: usize) -> Result<Option<ParsedKeyLine>, TomlError> {
    let line_no_nl = line.strip_suffix('\n').unwrap_or(line);
    let mut in_double = false;
    let mut in_single = false;
    let mut escape = false;
    let mut eq_pos = None;

    for (idx, ch) in line_no_nl.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        match ch {
            '"' => in_double = true,
            '\'' => in_single = true,
            '=' => {
                eq_pos = Some(idx);
                break;
            }
            '#' => return Ok(None),
            _ => {}
        }
    }

    let eq_idx = match eq_pos {
        Some(idx) => idx,
        None => return Ok(None),
    };

    let key_raw = &line_no_nl[..eq_idx];
    let key_trimmed = key_raw.trim();
    if key_trimmed.is_empty() {
        return Ok(None);
    }

    let key_path = KeyPath::parse(key_trimmed)?;

    let key_start_rel = key_raw.find(key_trimmed).unwrap_or(key_raw.len());
    let key_start = line_offset + key_start_rel;
    let key_end = key_start + key_trimmed.len();

    let mut value_start = eq_idx + 1;
    while let Some(ch) = line_no_nl.as_bytes().get(value_start) {
        if *ch == b' ' || *ch == b'\t' {
            value_start += 1;
        } else {
            break;
        }
    }

    let mut in_double = false;
    let mut in_single = false;
    let mut escape = false;
    let mut comment_pos = None;
    for (idx, ch) in line_no_nl[value_start..].char_indices() {
        let absolute_idx = value_start + idx;
        if escape {
            escape = false;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        match ch {
            '"' => in_double = true,
            '\'' => in_single = true,
            '#' => {
                comment_pos = Some(absolute_idx);
                break;
            }
            _ => {}
        }
    }

    let mut value_end = comment_pos.unwrap_or(line_no_nl.len());
    while value_end > value_start {
        let ch = line_no_nl.as_bytes()[value_end - 1];
        if ch == b' ' || ch == b'\t' {
            value_end -= 1;
        } else {
            break;
        }
    }

    Ok(Some(ParsedKeyLine {
        key_path,
        key_start,
        key_end,
        value_start: line_offset + value_start,
        value_end: line_offset + value_end,
    }))
}

fn scan_sections(content: &str) -> Result<Vec<SectionInfo>, TomlError> {
    let mut sections: Vec<SectionInfo> = Vec::new();
    let mut offset = 0usize;
    let mut last_section_index: Option<usize> = None;

    for line in content.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        offset = line_end;

        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if !trimmed.starts_with('[') {
            continue;
        }

        let header_start = line_start + (line.len() - trimmed.len());
        let header_info = parse_header(trimmed)?;
        if let Some(header) = header_info {
            let header_line_end = line_end;
            let body_start = line_end;
            let body_end_placeholder = line_end;

            if let Some(index) = last_section_index {
                sections[index].body_end = header_start;
            }

            sections.push(SectionInfo {
                path: header.path,
                header_start,
                header_line_end,
                body_start,
                body_end: body_end_placeholder,
            });

            last_section_index = Some(sections.len() - 1);
        }
    }

    if let Some(index) = last_section_index {
        sections[index].body_end = content.len();
    }

    Ok(sections)
}

#[derive(Debug)]
struct ParsedHeader {
    path: SectionPath,
}

fn parse_header(line: &str) -> Result<Option<ParsedHeader>, TomlError> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return Ok(None);
    }

    let (open_len, close_seq) = if trimmed.starts_with("[[") {
        (2, "]]")
    } else {
        (1, "]")
    };

    let close_pos = trimmed
        .find(close_seq)
        .ok_or_else(|| TomlError::InvalidTomlSyntax {
            message: format!("unterminated section header: {trimmed}"),
        })?;

    if close_pos < open_len {
        return Err(TomlError::InvalidTomlSyntax {
            message: format!("invalid section header: {trimmed}"),
        });
    }

    let inner = &trimmed[open_len..close_pos];
    let path = SectionPath::parse(inner)?;
    Ok(Some(ParsedHeader { path }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml::operations::Positioning;

    #[test]
    fn parse_header_simple() {
        let parsed = parse_header("[profile.zack]\n").unwrap().unwrap();
        assert_eq!(parsed.path.as_string(), "profile.zack");
    }

    #[test]
    fn insert_section_after() {
        let content = "[profile.release]\nopt-level = 3\n";
        let editor = TomlEditor::parse(content).unwrap();
        let query = TomlQuery::Section {
            path: SectionPath::parse("profile.zack").unwrap(),
        };
        let op = TomlOperation::InsertSection {
            text: "[profile.zack]\nopt-level = 3\n".to_string(),
            positioning: Positioning::AfterSection(SectionPath::parse("profile.release").unwrap()),
        };
        let plan = editor.plan(&query, &op, Constraints::none()).unwrap();
        match plan {
            TomlPlan::Edit(edit) => {
                assert!(edit.new_text.contains("[profile.zack]"));
            }
            _ => panic!("expected edit"),
        }
    }
}
