# Quick Start: Patching Codex for Privacy & Performance

This guide shows how to apply privacy and performance patches to a clean Codex checkout.

## Prerequisites

```bash
# Build the patcher
cd ~/dev/codex-patcher
cargo build --release

# Alias for convenience
alias codex-patcher="~/dev/codex-patcher/target/release/codex-patcher"
```

## Step 1: Apply Privacy Patches

The privacy patches remove hardcoded Statsig telemetry that was enabled by default.

```bash
# Apply to your working Codex checkout
codex-patcher apply \
  --workspace ~/dev/codex/codex-rs \
  --patches ~/dev/codex-patcher/patches/privacy.toml

# Or test on a fresh clone
git clone --branch rust-v0.88.0-alpha.4 https://github.com/openai/codex /tmp/test-codex
codex-patcher apply \
  --workspace /tmp/test-codex/codex-rs \
  --patches ~/dev/codex-patcher/patches/privacy.toml
```

### What Gets Patched

**File: `otel/src/config.rs`**
- ✓ Disables `resolve_exporter` function (returns `OtelExporter::None` for Statsig)
- ✓ Removes `STATSIG_OTLP_HTTP_ENDPOINT` constant (was: `https://ab.chatgpt.com/otlp/v1/metrics`)
- ✓ Removes `STATSIG_API_KEY_HEADER` constant
- ✓ Removes `STATSIG_API_KEY` constant (was: `client-MkRuleRQBd6qakfnDYqJVR9JuXcY57Ljly3vi5JVUIO`)

**File: `core/src/config/types.rs`**
- ✓ Changes `metrics_exporter` default from `Statsig` to `None`

## Step 2: Verify Patches Applied

```bash
cd ~/dev/codex/codex-rs

# Check that Statsig constants are gone
! rg "STATSIG_OTLP_HTTP_ENDPOINT" otel/src/config.rs
! rg "STATSIG_API_KEY" otel/src/config.rs

# Check that resolver is disabled
rg "OtelExporter::None" otel/src/config.rs
# Should show: OtelExporter::Statsig => OtelExporter::None

# Check default metrics exporter
rg "metrics_exporter.*None" core/src/config/types.rs
```

## Step 3: Build & Verify Telemetry Removed

```bash
cd ~/dev/codex/codex-rs

# Compile to verify no breakage
cargo check --workspace

# Build optimized binary
cargo build --release

# Verify no telemetry strings in binary
strings target/release/codex | grep -i statsig        # Should be empty
strings target/release/codex | grep "ab.chatgpt.com"  # Should be empty
strings target/release/codex | grep "MkRuleRQBd"      # Should be empty (API key)

# If any of the above find matches, the patches didn't work correctly
```

## Step 4: Add Performance Profile (Manual)

TOML editing is not yet implemented, so add the profile manually:

```bash
cd ~/dev/codex/codex-rs

# Append the zack profile to Cargo.toml
tail -n +35 ~/dev/codex-patcher/patches/zack-profile.toml >> Cargo.toml

# Or use your editor to copy-paste from patches/zack-profile.toml
# Add it after the [profile.ci-test] section
```

## Step 5: Build with Performance Profile

```bash
cd ~/dev/codex/codex-rs

# Build with Zen 5 optimizations
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack

# The binary will be at: target/zack/codex

# For profiling, add frame pointers:
RUSTFLAGS="-C target-cpu=znver5 -C force-frame-pointers=yes" \
  cargo build --profile zack

# Run it
./target/zack/codex --version
```

## Step 6: Idempotency Test

Patches are idempotent - safe to run multiple times:

```bash
# Run again - should report "already applied"
codex-patcher apply \
  --workspace ~/dev/codex/codex-rs \
  --patches ~/dev/codex-patcher/patches/privacy.toml

# Expected output:
# ✓ disable-statsig-resolver: Applied (idempotent)
# ⊘ remove-statsig-endpoint: Already deleted
# ⊘ remove-statsig-header: Already deleted
# ⊘ remove-statsig-api-key: Already deleted
# ✓ default-metrics-none-types: Applied (idempotent)
```

## Workflow: Updating to New Codex Release

When OpenAI releases a new version (e.g., `rust-v0.89.0`):

```bash
cd ~/dev/codex

# 1. Fetch upstream
git fetch origin
git tag | grep rust-v0.89

# 2. Merge new release
git checkout zack
git merge rust-v0.89.0

# 3. Resolve conflicts (if any)
# Manually fix Cargo.toml conflicts, etc.
git add .
git commit -m "Merge rust-v0.89.0 into zack branch"

# 4. Re-apply patches
cd ~/dev/codex-patcher
cargo run --release -- apply \
  --workspace ~/dev/codex/codex-rs \
  --patches patches/privacy.toml

# 5. Handle conflicts
# If patches fail, check what changed upstream:
cd ~/dev/codex/codex-rs
rg "resolve_exporter" otel/src/config.rs

# Update patches if needed
cd ~/dev/codex-patcher
# Edit patches/privacy.toml to match new structure

# 6. Verify
cd ~/dev/codex/codex-rs
cargo check --workspace
cargo build --release
strings target/release/codex | grep -i statsig  # Should be empty

# 7. Re-add performance profile (manual)
tail -n +35 ~/dev/codex-patcher/patches/zack-profile.toml >> Cargo.toml

# 8. Build with performance profile
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
./target/zack/codex --version
```

## Troubleshooting

### "Query matched no locations"

The patch can't find the code it's looking for. This means:
- Upstream changed the code structure
- The code was already patched
- The code was moved to a different file

**Solution:** Check what changed:

```bash
cd ~/dev/codex/codex-rs
rg "resolve_exporter" otel/src/config.rs

# Compare with original:
git show rust-v0.88.0-alpha.4:codex-rs/otel/src/config.rs | rg "resolve_exporter"
```

Update the patch pattern in `patches/privacy.toml` to match the new structure.

### "Parse error introduced"

The patch created invalid Rust syntax.

**Solution:**
1. Check `cargo check` output for errors
2. Review the patch `text` field for syntax errors
3. Test the snippet compiles:
   ```bash
   echo "fn test() { YOUR_CODE_HERE }" | rustc -
   ```

### Telemetry strings still in binary

The patches didn't apply or were incomplete.

**Solution:**
```bash
# Check which patches applied
codex-patcher status --workspace ~/dev/codex/codex-rs

# Re-apply failed patches
codex-patcher apply --workspace ~/dev/codex/codex-rs --patches patches/privacy.toml

# Manually verify the code changes
cat ~/dev/codex/codex-rs/otel/src/config.rs
```

## Additional Privacy Measures

Even with patches applied, add runtime safeguards:

### 1. Environment Variables

```bash
# Disable OpenTelemetry SDK completely
export OTEL_SDK_DISABLED=true

# Or in your shell rc:
echo 'export OTEL_SDK_DISABLED=true' >> ~/.zshrc
```

### 2. Config File

Add to `~/.codex/config.toml`:

```toml
[analytics]
enabled = false

[otel]
exporter = "none"
trace_exporter = "none"
metrics_exporter = "none"
```

### 3. Network Monitoring

Verify no telemetry with `tcpdump`:

```bash
# In one terminal:
sudo tcpdump -i any -n host ab.chatgpt.com

# In another terminal:
./target/zack/codex exec "print('hello')"

# tcpdump should show no packets
```

## Next Steps

- Read [patches/README.md](README.md) for patch authoring guide
- See [PHASE7_COMPLETE.md](../PHASE7_COMPLETE.md) for implementation details
- Check [plan.md](../plan.md) for the full roadmap
