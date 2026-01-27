# <img src="../.github/assets/icons/script.png" width="20" height="20" alt=""/> API Reference

Complete reference for using Codex Patcher as a library.

## <img src="../.github/assets/icons/layers.png" width="16" height="16" alt=""/> Core Types

### Edit

The fundamental edit primitive. All operations compile down to this.

```rust
pub struct Edit {
    pub file: PathBuf,
    pub byte_start: usize,
    pub byte_end: usize,
    pub new_text: String,
    pub expected_before: EditVerification,
}
```

#### Creating Edits

```rust
use codex_patcher::Edit;
use std::path::PathBuf;

// Simple creation with automatic verification
let edit = Edit::new(
    PathBuf::from("src/main.rs"),
    0,              // byte_start
    5,              // byte_end
    "HELLO",        // new_text
    "hello",        // expected_before
);

// With explicit verification strategy
use codex_patcher::EditVerification;

let edit = Edit::with_verification(
    PathBuf::from("src/main.rs"),
    0,
    5,
    "HELLO",
    EditVerification::Hash(0x1234567890abcdef),
);
```

#### Applying Edits

```rust
use codex_patcher::{Edit, EditResult};

let edit = Edit::new("src/main.rs", 0, 5, "HELLO", "hello");

match edit.apply() {
    Ok(EditResult::Applied { file, bytes_changed }) => {
        println!("Applied {} bytes to {}", bytes_changed, file.display());
    }
    Ok(EditResult::AlreadyApplied { file }) => {
        println!("Already patched: {}", file.display());
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

#### Batch Edits

```rust
use codex_patcher::Edit;

let edits = vec![
    Edit::new("src/main.rs", 0, 5, "HELLO", "hello"),
    Edit::new("src/main.rs", 10, 15, "WORLD", "world"),
    Edit::new("src/lib.rs", 0, 3, "FOO", "foo"),
];

// Edits are sorted and applied atomically per file
let results = Edit::apply_batch(edits)?;
```

---

### EditVerification

Verification strategy for edit safety.

```rust
pub enum EditVerification {
    /// Exact text match (for spans < 1KB)
    ExactMatch(String),
    /// xxh3 hash (for larger spans)
    Hash(u64),
}
```

#### Methods

```rust
use codex_patcher::EditVerification;

// Create from text (auto-selects strategy based on size)
let verify = EditVerification::from_text("hello world");

// Check if text matches
assert!(verify.matches("hello world"));

// Get hash value (works for both variants)
let hash = verify.hash();
```

---

### WorkspaceGuard

Enforces workspace boundaries for file operations.

```rust
use codex_patcher::WorkspaceGuard;

// Create guard for workspace
let guard = WorkspaceGuard::new("/path/to/workspace")?;

// Validate paths before editing
let file = guard.validate_path("src/main.rs")?;  // Ok
let file = guard.validate_path("../outside.rs"); // Error: outside workspace
let file = guard.validate_path("target/foo");    // Error: forbidden path

// Get workspace root
let root = guard.workspace_root();
```

#### Forbidden Paths

The guard automatically blocks:
- `~/.cargo/registry` - Dependency sources
- `~/.cargo/git` - Git dependencies
- `~/.rustup` - Rust toolchains
- `{workspace}/target` - Build artifacts

---

## <img src="../.github/assets/icons/tree.png" width="16" height="16" alt=""/> Tree-sitter Integration

### RustParser

Parse Rust source with tree-sitter.

```rust
use codex_patcher::ts::RustParser;

let mut parser = RustParser::new()?;

// Parse source
let tree = parser.parse("fn main() {}")?;

// Parse with source reference
let parsed = parser.parse_with_source("fn main() {}")?;
println!("Has errors: {}", parsed.has_errors());
```

### StructuralLocator

Find code structures by name/pattern.

```rust
use codex_patcher::ts::{StructuralLocator, StructuralTarget};

let mut locator = StructuralLocator::new()?;

let source = r#"
    fn main() {
        println!("hello");
    }

    struct Config {
        value: i32,
    }
"#;

// Find function by name
let span = locator.locate(source, &StructuralTarget::Function {
    name: "main".to_string(),
})?;

// Find struct by name
let span = locator.locate(source, &StructuralTarget::Struct {
    name: "Config".to_string(),
})?;

// Use the span
println!("Found at bytes {}..{}", span.byte_start, span.byte_end);
```

---

## <img src="../.github/assets/icons/search.png" width="16" height="16" alt=""/> ast-grep Integration

### PatternMatcher

Pattern-based code matching.

```rust
use codex_patcher::sg::PatternMatcher;

let source = r#"
    fn main() {
        let x = foo.clone();
        let y = bar.clone();
    }
"#;

let matcher = PatternMatcher::new(source);

// Find all matches
let matches = matcher.find_all("$EXPR.clone()")?;
for m in &matches {
    println!("Found: {} at {}..{}", m.text, m.byte_start, m.byte_end);
}

// Find unique match (errors if 0 or >1 matches)
let m = matcher.find_unique("fn main() { $$$BODY }")?;

// Find within a specific function
let matches = matcher.find_in_function("$EXPR.clone()", "main")?;
```

### Pattern Syntax

| Pattern | Matches |
|---------|---------|
| `$NAME` | Single node, captures as NAME |
| `$$$NAME` | Zero or more nodes (variadic) |
| `$_` | Single node, no capture |
| Literal | Exact match |

#### Examples

```rust
// Match function definitions
"fn $NAME($$$PARAMS) { $$$BODY }"

// Match method calls
"$EXPR.$METHOD($$$ARGS)"

// Match struct fields
"struct $NAME { $$$FIELDS }"

// Match specific patterns
"Option::Some($VALUE)"
"println!($$$ARGS)"
```

### CaptureReplacer

Replace matched code using captures.

```rust
use codex_patcher::sg::{PatternMatcher, CaptureReplacer};

let source = "let x = foo.clone();";
let matcher = PatternMatcher::new(source);
let m = matcher.find_unique("$EXPR.clone()")?;

let replacer = CaptureReplacer::new(&matcher, m);

// Replace entire match
let replacement = replacer.replace_match("$EXPR.to_owned()");

// Replace using template with captures
let replacement = replacer.replace_with_template("$EXPR.to_owned()");

// Convert to Edit
let edit = replacement.to_edit("src/main.rs");
edit.apply()?;
```

---

## <img src="../.github/assets/icons/folder.png" width="16" height="16" alt=""/> TOML Editing

### TomlEditor

Format-preserving TOML editing.

```rust
use codex_patcher::toml::{TomlEditor, TomlQuery, TomlOperation, SectionPath, KeyPath};

let content = r#"
[package]
name = "my-crate"
version = "0.1.0"
"#;

let editor = TomlEditor::parse(content)?;

// Query for a key
let query = TomlQuery::Key {
    section: SectionPath::parse("package")?,
    key: KeyPath::parse("version")?,
};

// Replace value
let operation = TomlOperation::ReplaceValue {
    value: "\"0.2.0\"".to_string(),
};

// Plan the edit
let plan = editor.plan(&query, &operation, Constraints::none())?;

// Apply
match plan {
    TomlPlan::Edit(edit) => edit.apply()?,
    TomlPlan::NoOp(reason) => println!("Nothing to do: {}", reason),
}
```

See [TOML Patching](toml.md) for complete TOML reference.

---

## <img src="../.github/assets/icons/shield-security-protection-16x16.png" width="16" height="16" alt=""/> Validation

### ParseValidator

Validate edits don't introduce parse errors.

```rust
use codex_patcher::validate::ParseValidator;

let mut validator = ParseValidator::new()?;

// Validate source has no errors
validator.validate("fn main() {}")?;

// Validate edit doesn't introduce errors
let original = "fn main() { let x = 1; }";
let edited = "fn main() { let y = 2; }";
validator.validate_edit(original, edited)?;
```

### ValidatedEdit

Wrapper that validates before applying.

```rust
use codex_patcher::validate::ValidatedEdit;
use codex_patcher::Edit;

let edit = Edit::new("src/main.rs", 0, 5, "HELLO", "hello");

// Wrap for validation
let validated = ValidatedEdit::new(edit);

// Apply with validation (fails if parse errors introduced)
validated.apply()?;

// Skip validation if needed
let validated = ValidatedEdit::new(edit).skip_parse_validation();
```

### syn Validation

Validate code snippets with syn.

```rust
use codex_patcher::validate::syn_validate;

// Validate as item (fn, struct, etc.)
syn_validate::validate_item("fn foo() {}")?;

// Validate as expression
syn_validate::validate_expr("1 + 2")?;

// Validate as type
syn_validate::validate_type("Option<String>")?;

// Validate as complete file
syn_validate::validate_file("fn main() {}")?;
```

---

## <img src="../.github/assets/icons/gear-24x24.png" width="16" height="16" alt=""/> Configuration

### Loading Patch Configs

```rust
use codex_patcher::config::{load_from_path, load_from_str};

// Load from file
let config = load_from_path("patches/privacy.toml")?;

// Load from string
let toml = r#"
[meta]
name = "example"
workspace_relative = true

[[patches]]
id = "test"
file = "src/main.rs"
"#;
let config = load_from_str(toml)?;
```

### Applying Patches

```rust
use codex_patcher::config::{apply_patches, PatchResult};
use std::path::Path;

let config = load_from_path("patches/privacy.toml")?;
let workspace = Path::new("/path/to/workspace");
let version = "0.88.0";

let results = apply_patches(&config, workspace, version);

for (patch_id, result) in results {
    match result {
        Ok(PatchResult::Applied { file }) => {
            println!("{}: Applied to {}", patch_id, file.display());
        }
        Ok(PatchResult::AlreadyApplied { file }) => {
            println!("{}: Already applied to {}", patch_id, file.display());
        }
        Ok(PatchResult::SkippedVersion { reason }) => {
            println!("{}: Skipped ({})", patch_id, reason);
        }
        Ok(PatchResult::Failed { file, reason }) => {
            println!("{}: Failed on {} - {}", patch_id, file.display(), reason);
        }
        Err(e) => {
            println!("{}: Error - {}", patch_id, e);
        }
    }
}
```

---

## <img src="../.github/assets/icons/error.png" width="16" height="16" alt=""/> Error Types

### EditError

```rust
pub enum EditError {
    BeforeTextMismatch { file, byte_start, byte_end, expected, found },
    InvalidByteRange { byte_start, byte_end, file_len },
    OutsideWorkspace(PathBuf),
    Io(std::io::Error),
    Utf8(std::str::Utf8Error),
    InvalidUtf8Edit,
}
```

### SafetyError

```rust
pub enum SafetyError {
    OutsideWorkspace { path, workspace },
    ForbiddenPath { path, forbidden },
    Canonicalize(std::io::Error),
}
```

### ApplicationError

```rust
pub enum ApplicationError {
    Version(VersionError),
    Io { path, source },
    Edit(EditError),
    AmbiguousMatch { file, count },
    NoMatch { file },
    TomlOperation { file, reason },
}
```

---

## <img src="../.github/assets/icons/book.png" width="16" height="16" alt=""/> See Also

- [Getting Started](getting-started.md)
- [Patch Authoring](patches.md)
- [Architecture](architecture.md)
- [TOML Patching](toml.md)
