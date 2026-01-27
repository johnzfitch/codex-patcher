# Codex Patcher Justfile
# Quick recipes for common patch operations

# Default recipe - show available commands
default:
    @just --list

# Build the patcher in release mode
build:
    cargo build --release --quiet

# Run all tests
test:
    cargo test --quiet

# Apply patches to local codex workspace
patch workspace=`echo ~/dev/codex/codex-rs`:
    cargo run --quiet --release -- apply --workspace {{workspace}}

# Apply specific patch file
patch-file workspace=`echo ~/dev/codex/codex-rs` file="patches/privacy.toml":
    cargo run --quiet --release -- apply --workspace {{workspace}} --patches {{file}}

# Check patch status without applying
status workspace=`echo ~/dev/codex/codex-rs`:
    cargo run --quiet --release -- status --workspace {{workspace}}

# Verify patches are correctly applied
verify workspace=`echo ~/dev/codex/codex-rs`:
    cargo run --quiet --release -- verify --workspace {{workspace}}

# Apply patches with diff output
patch-diff workspace=`echo ~/dev/codex/codex-rs`:
    cargo run --quiet --release -- apply --workspace {{workspace}} --diff

# Test patches on clean checkout of specific version
test-patches version="rust-v0.88.0-alpha.4":
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Testing patches on clean checkout of {{version}}..."
    rm -rf /tmp/codex-patch-test
    git clone --quiet --depth=1 -b {{version}} https://github.com/openai/codex /tmp/codex-patch-test
    echo "Applying patches..."
    cargo run --quiet --release -- apply --workspace /tmp/codex-patch-test/codex-rs
    echo "Running cargo check..."
    cd /tmp/codex-patch-test/codex-rs && cargo check --quiet --workspace
    echo "✓ All patches applied successfully and workspace builds"

# Test patches with verification
test-patches-verify version="rust-v0.88.0-alpha.4":
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Testing patches on {{version}} with verification..."
    rm -rf /tmp/codex-patch-test
    git clone --quiet --depth=1 -b {{version}} https://github.com/openai/codex /tmp/codex-patch-test
    cargo run --quiet --release -- apply --workspace /tmp/codex-patch-test/codex-rs --diff
    cargo run --quiet --release -- verify --workspace /tmp/codex-patch-test/codex-rs
    cd /tmp/codex-patch-test/codex-rs && cargo check --quiet --workspace
    echo "✓ Patches verified and workspace builds"

# Build codex with zack profile (requires patches applied)
build-codex workspace=`echo ~/dev/codex/codex-rs`:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{workspace}}
    echo "Building with zack profile..."
    RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack --quiet
    echo "✓ Build complete"

# Verify no telemetry strings in binary
verify-no-telemetry workspace=`echo ~/dev/codex/codex-rs`:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{workspace}}
    if [ ! -f target/zack/codex ]; then
        echo "Error: Binary not found. Run 'just build-codex' first."
        exit 1
    fi
    echo "Checking for telemetry strings..."
    if strings target/zack/codex | grep -q "ab.chatgpt.com"; then
        echo "✗ Found telemetry strings!"
        strings target/zack/codex | grep "ab.chatgpt.com"
        exit 1
    else
        echo "✓ No telemetry strings found"
    fi

# Full workflow: patch, build, and verify
full-workflow workspace=`echo ~/dev/codex/codex-rs`:
    just patch {{workspace}}
    just verify {{workspace}}
    just build-codex {{workspace}}
    just verify-no-telemetry {{workspace}}

# Clean test artifacts
clean:
    rm -rf /tmp/codex-patch-test
    cargo clean

# Format code
fmt:
    cargo fmt

# Run clippy lints
lint:
    cargo clippy --quiet -- -D warnings

# Generate documentation
docs:
    cargo doc --no-deps --open

# Watch for changes and run tests
watch:
    cargo watch -x test

# Create a new patch file from template
new-patch name:
    #!/usr/bin/env bash
    cat > patches/{{name}}.toml << 'EOF'
    [meta]
    name = "{{name}}"
    description = "Description of patches"
    workspace_relative = true

    [[patches]]
    id = "example-patch"
    file = "path/to/file.rs"

    [patches.query]
    type = "ast-grep"
    pattern = "fn example() { $$$BODY }"

    [patches.operation]
    type = "replace"
    text = "fn example() { /* modified */ }"
    EOF
    echo "Created patches/{{name}}.toml"

# Find subagent limit constant location
find-subagent-limit workspace=`echo ~/dev/codex/codex-rs`:
    ./find-subagent-limit.sh {{workspace}}

# List all available patches
list-patches:
    #!/usr/bin/env bash
    echo "Available patches:"
    echo "=================="
    for patch in patches/*.toml; do
        if [ -f "$patch" ]; then
            name=$(basename "$patch" .toml)
            desc=$(grep "^description" "$patch" | head -1 | cut -d'"' -f2)
            echo "  $name: $desc"
        fi
    done
