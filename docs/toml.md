# TOML Patching Guide

## TOML Query Syntax

Section paths use dotted notation. Each segment maps to a TOML table name.

Examples:
- `profile.zack` => `[profile.zack]`
- `target."x86_64-unknown-linux-gnu"` => `[target."x86_64-unknown-linux-gnu"]`
- `profile."zack.test"` => `[profile."zack.test"]`

Key paths are also dotted. A key query requires both `section` and `key`.

Examples:
- `section = "profile.release"`, `key = "opt-level"`
- `section = "target"`, `key = "x86_64-unknown-linux-gnu"`

## Operations

- `insert_section`: Insert a TOML section at a specific location.
- `append_section`: Add a new section at the end of the file.
- `replace_value`: Replace a key's value inside a section.
- `delete_section`: Remove a whole TOML section.
- `replace_key`: Replace a key name inside a section.

## Positioning

Use exactly one positioning directive:
- `after_section = "profile.ci-test"`
- `before_section = "profile.dev"`
- `at_end = true`
- `at_beginning = true`

If none is set, the default is `at_end`.

## Constraints

Use constraints to make operations idempotent and safe:
- `ensure_absent = true`: Only apply if the section/key does not exist.
- `ensure_present = true`: Only apply if the section/key exists.

## Examples

Insert a new Cargo profile after `profile.ci-test`:

```toml
[[patches]]
id = "add-zack-profile"
file = "codex-rs/Cargo.toml"

[patches.query]
type = "toml"
section = "profile.zack"
ensure_absent = true

[patches.operation]
type = "insert_section"
after_section = "profile.ci-test"
text = '''
[profile.zack]
opt-level = 3
lto = "fat"
'''
```

Append a new target section to `.cargo/config.toml`:

```toml
[[patches]]
id = "cargo-config-zen5"
file = "codex-rs/.cargo/config.toml"

[patches.query]
type = "toml"
section = "target.x86_64-unknown-linux-gnu"
ensure_absent = true

[patches.operation]
type = "append_section"
text = '''
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "target-cpu=znver5", "-C", "link-arg=-fuse-ld=mold"]
'''
```

Replace a key value inside a section:

```toml
[[patches]]
id = "release-opt"
file = "codex-rs/Cargo.toml"

[patches.query]
type = "toml"
section = "profile.release"
key = "opt-level"
ensure_present = true

[patches.operation]
type = "replace_value"
value = "3"
```

## Error Guide

- `InvalidTomlSyntax`: The input or result is not valid TOML. Check the section text or value snippet.
- `SectionNotFound`: The requested section path does not exist in the file.
- `KeyNotFound`: The requested key does not exist inside the section.
- `AmbiguousMatch`: Multiple sections/keys matched the query. Narrow the query.
- `InvalidPositioning`: Conflicting positioning directives were provided.
- `Unsupported`: The editor encountered TOML it cannot safely edit (for example, multiline values).
