# Complete Patch Set for Codex Custom Build

This document describes all patches in the codex-patcher system for maintaining your custom Codex build.

## Patch Files Overview

| File | Purpose | Status | Files Modified |
|------|---------|--------|----------------|
| `privacy.toml` | Disable Statsig telemetry | ✅ Tested on 0.88.0 | otel/src/config.rs, core/src/config/types.rs |
| `performance.toml` | Zen 5 optimized profile (manual) | ⚠️ Manual install | Cargo.toml |
| `zack-profile.toml` | Profile definition reference | 📄 Reference only | N/A |
| `approvals-ui.toml` | Simplified 4-preset approval UI | 🔄 For 0.91.0+ | common/src/approval_presets.rs, tui/src/bottom_pane/footer.rs |
| `subagent-limit.toml` | Increase subagents 6→8 | 🔄 For 0.91.0+ | TBD (needs grep) |
| `cargo-config.toml` | Linux optimization defaults | 🔄 Optional | .cargo/config.toml |

## Patch Application Order

For a clean 0.91.0+ build from upstream:

```bash
# 1. Privacy patches (always apply first)
just patch-file ~/dev/codex/codex-rs patches/privacy.toml

# 2. Subagent limit (if you want 8 instead of 6)
just patch-file ~/dev/codex/codex-rs patches/subagent-limit.toml

# 3. Approvals UI (simplified presets)
just patch-file ~/dev/codex/codex-rs patches/approvals-ui.toml

# 4. Cargo config optimizations (optional)
just patch-file ~/dev/codex/codex-rs patches/cargo-config.toml

# 5. Zack profile (manual - TOML insertion not yet automated)
tail -n +33 patches/zack-profile.toml >> ~/dev/codex/codex-rs/Cargo.toml
```

Or apply all at once:

```bash
just patch ~/dev/codex/codex-rs
```

## Detailed Patch Descriptions

### 1. Privacy Patches (`privacy.toml`)

**Purpose:** Remove hardcoded telemetry to ab.chatgpt.com

**What it does:**
- Disables Statsig telemetry resolver (always returns `OtelExporter::None`)
- Removes hardcoded endpoint: `https://ab.chatgpt.com/otlp/v1/metrics`
- Removes hardcoded API key: `client-MkRuleRQBd6qakfnDYqJVR9JuXcY57Ljly3vi5JVUIO`
- Changes default metrics_exporter from `Statsig` to `None`

**Files modified:**
1. `codex-rs/otel/src/config.rs`
   - Patch `resolve_exporter()` function
   - Delete 3 Statsig constants
2. `codex-rs/core/src/config/types.rs`
   - Change `OtelConfig::default()` metrics_exporter to `None`

**Verification:**
```bash
strings target/zack/codex | grep -i statsig          # Should be empty
strings target/zack/codex | grep "ab.chatgpt.com"    # Should be empty
strings target/zack/codex | grep "MkRuleRQBd6"       # Should be empty
```

**Status:** ✅ Tested and working on rust-v0.88.0-alpha.17

---

### 2. Performance Profile (`performance.toml`, `zack-profile.toml`)

**Purpose:** Maximum performance build with Zen 5 optimizations

**What it does:**
- Adds `[profile.zack]` section to Cargo.toml
- Enables fat LTO, single codegen unit, opt-level 3
- Keeps debug symbols for profiling (no runtime cost)
- Optimizes all dependencies aggressively

**Manual installation required:**
```bash
# Option 1: Append from reference file
tail -n +33 patches/zack-profile.toml >> ~/dev/codex/codex-rs/Cargo.toml

# Option 2: Copy-paste the [profile.zack] section manually
```

**Build command:**
```bash
RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack -p codex-cli
```

**Profile settings:**
```toml
[profile.zack]
inherits = "release"
lto = "fat"                    # Maximum link-time optimization
codegen-units = 1              # Best optimization (single codegen unit)
opt-level = 3                  # Maximum optimization level
strip = false                  # Keep symbols for profiling/debugging
debug = 2                      # Full debug info (flamegraphs, perf, gdb)
overflow-checks = false        # Disable overflow checks for max speed

[profile.zack.build-override]
opt-level = 3

[profile.zack.package."*"]
opt-level = 3
```

**Status:** ⚠️ Manual installation (TOML section insertion not yet automated)

---

### 3. Approvals UI (`approvals-ui.toml`)

**Purpose:** Simplified 4-preset approval system (Claude Code-style)

**What it does:**
- Replaces complex approval presets with 4 clear modes
- Adds approval/sandbox policy badge to TUI footer
- Makes permission model more intuitive

**Four presets:**

| Preset | Description | Approval Policy | Sandbox Policy |
|--------|-------------|----------------|----------------|
| **Read Only** | Can read files/code. No edits, no commands. | OnRequest | ReadOnly |
| **Edits Only** | Can read and edit files. No command execution. | OnRequest | WorkspaceWrite |
| **Sandboxed Agent** | Full agent mode, sandboxed to workspace. | OnRequest | WorkspaceWrite |
| **Full Access** | No restrictions. Full file and network access. | Never | DangerFullAccess |

**Files modified:**
1. `codex-rs/common/src/approval_presets.rs`
   - Replace `builtin_approval_presets()` with 4 simple presets
2. `codex-rs/tui/src/bottom_pane/footer.rs`
   - Add approval badge display `[READ ONLY]`, `[SANDBOXED]`, `[FULL ACCESS]`, etc.

**Status:** 🔄 For rust-v0.91.0+ (based on cherry-pick from zack-personal branch)

---

### 4. Subagent Limit (`subagent-limit.toml`)

**Purpose:** Increase max concurrent subagents from 6 to 8

**What it does:**
- Finds `MAX_CONCURRENT_SUBAGENTS` constant
- Changes value from 6 to 8
- Provides better parallelism for complex multi-file tasks

**Rationale:**
- Upstream decreased from 12 to 6 (too conservative)
- 8 is a reasonable middle ground
- Balances parallelism with resource usage

**Files modified:**
- Location TBD - need to `grep -r "MAX_CONCURRENT_SUBAGENTS" codex-rs/`
- Likely in: `codex-rs/agent/src/config.rs` or `codex-rs/executor/src/limits.rs`

**Status:** 🔄 For rust-v0.91.0+ (needs file location verification)

**To find location:**
```bash
cd ~/dev/codex/codex-rs
grep -r "MAX_CONCURRENT_SUBAGENTS" .
```

---

### 5. Cargo Config Optimizations (`cargo-config.toml`)

**Purpose:** Add Linux x86_64 optimization defaults

**What it does:**
- Adds `[target.x86_64-unknown-linux-gnu]` section to `.cargo/config.toml`
- Sets `target-cpu=native` for all CPU features
- Sets `opt-level=3` for maximum optimization
- Uses mold linker if available (falls back gracefully)

**Configuration added:**
```toml
[target.x86_64-unknown-linux-gnu]
rustflags = [
    "-C", "target-cpu=native",
    "-C", "opt-level=3",
    "-C", "link-arg=-fuse-ld=mold",
]
```

**Note:** The zack profile build overrides `target-cpu` to `znver5` specifically via RUSTFLAGS.

**Status:** 🔄 Optional enhancement

---

## Version Compatibility

| Codex Version | Privacy | Performance | Approvals UI | Subagent Limit | Cargo Config |
|---------------|---------|-------------|--------------|----------------|--------------|
| 0.88.0-alpha.17 | ✅ | ✅ Manual | ⚠️ N/A | ⚠️ N/A | ✅ |
| 0.91.0+ | ✅ | ✅ Manual | ✅ | ✅ | ✅ |

## Testing Patches

### Test on clean checkout
```bash
just test-patches rust-v0.91.0
```

### Test full workflow
```bash
just full-workflow ~/dev/codex/codex-rs
```

### Verify privacy patches
```bash
just verify-no-telemetry ~/dev/codex/codex-rs
```

## Patch Maintenance

When upstream releases a new version:

1. **Backup current state:**
   ```bash
   git branch zack-backup-pre-0.XX
   git tag zack-v0.XX-patched
   ```

2. **Merge upstream:**
   ```bash
   git fetch origin
   git merge origin/main
   ```

3. **Reapply patches:**
   ```bash
   cd ~/dev/codex-patcher
   just patch ~/dev/codex/codex-rs
   ```

4. **Manually add zack profile:**
   ```bash
   tail -n +33 patches/zack-profile.toml >> ~/dev/codex/codex-rs/Cargo.toml
   ```

5. **Build and verify:**
   ```bash
   cd ~/dev/codex/codex-rs
   RUSTFLAGS="-C target-cpu=znver5" cargo build --profile zack
   just verify-no-telemetry
   ```

## Creating New Patches

```bash
# Create from template
just new-patch my-feature

# Edit the file
nvim patches/my-feature.toml

# Test it
just patch-file ~/dev/codex/codex-rs patches/my-feature.toml
```

See `patches/README.md` for detailed patch authoring guide.

## Troubleshooting

### Patch fails with "query matched no locations"
- The upstream code changed
- Update the query pattern in the patch file
- Use `grep` to find the new location/structure

### Patch fails with "query matched multiple locations"
- Make the pattern more specific
- Add more surrounding context to the query

### TOML patches don't apply
- TOML section insertion is implemented as of Phase 9
- Try applying manually if issues occur

### Subagent limit patch doesn't apply
- Find the actual location: `grep -r "MAX_CONCURRENT_SUBAGENTS" codex-rs/`
- Update the `file` field in `subagent-limit.toml`

## Reference Documentation

- `README.md` - General patch authoring guide
- `QUICKSTART.md` - Quick start for new users
- `ADDING_PATCHES.md` - Step-by-step patch creation
- `EXAMPLES.md` - Example patches and patterns
- `PATCH_WORKFLOW.md` - Workflow and best practices
- `PHASE9_COMPLETE.md` - Phase 9 implementation details

## Support

For issues or questions:
- Check existing patch files for examples
- Review `just --list` for available commands
- Test patches on clean checkout with `just test-patches`
- Verify with `just verify`
