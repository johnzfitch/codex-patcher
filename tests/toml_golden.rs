use codex_patcher::toml::{Constraints, Positioning, SectionPath, TomlEditor, TomlPlan, TomlQuery};
use codex_patcher::toml::{KeyPath, TomlOperation};
use std::fs;
use std::io::Write;

fn load_fixture(name: &str) -> String {
    fs::read_to_string(format!("tests/fixtures/{name}"))
        .unwrap_or_else(|err| panic!("failed to load fixture {name}: {err}"))
}

fn write_temp(contents: &str) -> tempfile::NamedTempFile {
    let mut temp = tempfile::NamedTempFile::new().expect("tempfile");
    temp.write_all(contents.as_bytes()).expect("write temp");
    temp.flush().expect("flush temp");
    temp
}

#[test]
fn insert_section_after_fixture() {
    let input = load_fixture("Cargo.toml.input");
    let expected = load_fixture("Cargo.toml.expected");
    let temp = write_temp(&input);

    let editor = TomlEditor::from_path(temp.path(), &input).expect("editor");
    let query = TomlQuery::Section {
        path: SectionPath::parse("profile.zack").expect("section path"),
    };
    let operation = TomlOperation::InsertSection {
        text: "[profile.zack]\nopt-level = 3\nlto = \"fat\"\n".to_string(),
        positioning: Positioning::AfterSection(
            SectionPath::parse("profile.ci-test").expect("position section"),
        ),
    };
    let constraints = Constraints {
        ensure_absent: true,
        ensure_present: false,
    };

    let plan = editor.plan(&query, &operation, constraints).expect("plan");
    match plan {
        TomlPlan::Edit(edit) => {
            let _ = edit.apply().expect("apply edit");
        }
        TomlPlan::NoOp(reason) => panic!("unexpected no-op: {reason}"),
    }

    let output = fs::read_to_string(temp.path()).expect("read output");
    assert_eq!(output, expected);

    let editor = TomlEditor::from_path(temp.path(), &output).expect("editor");
    let plan = editor.plan(&query, &operation, constraints).expect("plan");
    match plan {
        TomlPlan::NoOp(_) => {}
        TomlPlan::Edit(_) => panic!("expected no-op on second application"),
    }
}

#[test]
fn append_section_at_end_fixture() {
    let input = load_fixture("config.toml.input");
    let expected = load_fixture("config.toml.expected");
    let temp = write_temp(&input);

    let editor = TomlEditor::from_path(temp.path(), &input).expect("editor");
    let query = TomlQuery::Section {
        path: SectionPath::parse("target.x86_64-unknown-linux-gnu").expect("section path"),
    };
    let operation = TomlOperation::AppendSection {
        text: "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\nrustflags = [\"-C\", \"target-cpu=znver5\", \"-C\", \"link-arg=-fuse-ld=mold\"]\n".to_string(),
    };

    let plan = editor
        .plan(&query, &operation, Constraints::none())
        .expect("plan");
    match plan {
        TomlPlan::Edit(edit) => {
            let _ = edit.apply().expect("apply edit");
        }
        TomlPlan::NoOp(reason) => panic!("unexpected no-op: {reason}"),
    }

    let output = fs::read_to_string(temp.path()).expect("read output");
    assert_eq!(output, expected);
}

#[test]
fn replace_value_in_section() {
    let input = "[profile.release]\nopt-level = 3\n";
    let temp = write_temp(input);

    let editor = TomlEditor::from_path(temp.path(), input).expect("editor");
    let query = TomlQuery::Key {
        section: SectionPath::parse("profile.release").expect("section path"),
        key: KeyPath::parse("opt-level").expect("key path"),
    };
    let operation = TomlOperation::ReplaceValue {
        value: "2".to_string(),
    };

    let plan = editor
        .plan(&query, &operation, Constraints::none())
        .expect("plan");
    match plan {
        TomlPlan::Edit(edit) => {
            let _ = edit.apply().expect("apply edit");
        }
        TomlPlan::NoOp(reason) => panic!("unexpected no-op: {reason}"),
    }

    let output = fs::read_to_string(temp.path()).expect("read output");
    assert_eq!(output, "[profile.release]\nopt-level = 2\n");
}
