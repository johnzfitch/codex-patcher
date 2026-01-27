# <img src="assets/icons/globe.png" width="20" height="20" alt=""/> Contributing to Codex Patcher

Thank you for your interest in contributing to Codex Patcher! This document provides guidelines and information for contributors.

## <img src="assets/icons/book.png" width="16" height="16" alt=""/> Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing](#testing)

---

## <img src="assets/icons/shield-security-protection-16x16.png" width="16" height="16" alt=""/> Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

---

## <img src="assets/icons/lightning.png" width="16" height="16" alt=""/> Getting Started

### Prerequisites

- Rust 1.70.0 or later (check with `rustc --version`)
- Git
- A text editor or IDE with Rust support

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR-USERNAME/codex-patcher
   cd codex-patcher
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/ORIGINAL-ORG/codex-patcher
   ```

---

## <img src="assets/icons/toolbox.png" width="16" height="16" alt=""/> Development Setup

### Build the project

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

### Run tests

```bash
# All tests
cargo test

# Specific test
cargo test test_name

# With output
cargo test -- --nocapture
```

### Code quality checks

```bash
# Clippy (linting)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Format code
cargo fmt

# Generate documentation
cargo doc --open
```

---

## <img src="assets/icons/script.png" width="16" height="16" alt=""/> Making Changes

### Branch Naming

Use descriptive branch names with prefixes:

| Prefix | Use Case |
|--------|----------|
| `feature/` | New features |
| `fix/` | Bug fixes |
| `refactor/` | Code refactoring |
| `docs/` | Documentation changes |
| `test/` | Test additions/changes |

Example: `feature/add-json-support` or `fix/workspace-symlink-escape`

### Commit Messages

Follow conventional commits format:

```
type: short description (50 chars max)

Longer description explaining the "why" not the "what".
The code shows what changed; the commit message explains why.

Wrap at 72 characters.
```

**Types:**
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation
- `refactor:` - Code refactoring
- `test:` - Test changes
- `chore:` - Build/tooling changes

**Examples:**
```
feat: add TOML section deletion operation

fix: prevent symlink escape in workspace validation

docs: add patch authoring guide
```

---

## <img src="assets/icons/checkbox.png" width="16" height="16" alt=""/> Pull Request Process

### Before Submitting

1. **Sync with upstream:**
   ```bash
   git fetch upstream
   git rebase upstream/master
   ```

2. **Run all checks:**
   ```bash
   cargo test
   cargo clippy --all-targets -- -D warnings
   cargo fmt --check
   ```

3. **Update documentation** if needed

### PR Requirements

- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Code is formatted with `cargo fmt`
- [ ] New code has tests
- [ ] Documentation is updated
- [ ] Commit messages follow conventions

### Review Process

1. Open a PR against `master`
2. Fill out the PR template
3. Wait for CI to pass
4. Address reviewer feedback
5. Squash commits if requested
6. Maintainer merges when approved

---

## <img src="assets/icons/star.png" width="16" height="16" alt=""/> Coding Standards

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `snake_case` for functions/variables
- Use `PascalCase` for types/traits
- Use `SCREAMING_SNAKE_CASE` for constants

### Error Handling

```rust
// Library code: use thiserror
#[derive(Error, Debug)]
pub enum MyError {
    #[error("something went wrong: {0}")]
    SomethingWrong(String),
}

// Application code: use anyhow
fn main() -> anyhow::Result<()> {
    do_thing().context("failed to do thing")?;
    Ok(())
}
```

### Documentation

- Document all public items with `///`
- Include examples where helpful
- Use `#[must_use]` for important return values

```rust
/// Validates a path is within the workspace.
///
/// # Errors
///
/// Returns an error if the path is outside the workspace
/// or resolves to a forbidden directory.
///
/// # Example
///
/// ```
/// let guard = WorkspaceGuard::new("/path/to/workspace")?;
/// let file = guard.validate_path("src/main.rs")?;
/// ```
pub fn validate_path(&self, path: impl AsRef<Path>) -> Result<PathBuf, SafetyError>
```

### No Unwrap in Production

```rust
// Bad
let value = option.unwrap();

// Good
let value = option.ok_or_else(|| MyError::MissingValue)?;

// Also acceptable (with context)
let value = option.expect("value should always exist after validation");
```

---

## <img src="assets/icons/search.png" width="16" height="16" alt=""/> Testing

### Test Organization

```
tests/
├── integration/       # Integration tests
│   └── mod.rs
├── cli_integration.rs # CLI tests
└── toml_golden.rs     # Golden file tests

src/
└── */
    └── mod.rs         # Unit tests in #[cfg(test)] modules
```

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptive_name() {
        // Arrange
        let input = create_test_input();

        // Act
        let result = function_under_test(input);

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_value);
    }

    #[test]
    fn test_error_case() {
        let result = function_that_should_fail(bad_input);
        assert!(matches!(result, Err(MyError::SpecificVariant { .. })));
    }
}
```

### Property-Based Testing

We use `proptest` for property-based tests:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_edit_verification_roundtrip(text in ".*") {
        let verification = EditVerification::from_text(&text);
        prop_assert!(verification.matches(&text));
    }
}
```

---

## <img src="assets/icons/magic-wand.png" width="16" height="16" alt=""/> Need Help?

- Open an issue for bugs or feature requests
- Start a discussion for questions
- Tag maintainers for urgent issues

Thank you for contributing!
