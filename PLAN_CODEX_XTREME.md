# codex-xtreme: Interactive TUI for Building Patched Codex

## Overview

A separate binary `codex-xtreme` that provides an interactive ratatui-based TUI for:
1. Downloading/updating Codex source
2. Cherry-picking commits beyond the latest release
3. Selecting patches to apply
4. Building with dynamic CPU optimizations
5. Running verification tests
6. Setting up the `codex` alias

## Architecture

### Binary Structure
```
codex-patcher/
├── src/
│   ├── lib.rs           # Existing library (patching logic)
│   ├── main.rs          # Existing CLI
│   └── bin/
│       └── codex_xtreme/
│           ├── main.rs      # Entry point
│           ├── app.rs       # App state machine
│           ├── ui.rs        # Ratatui rendering
│           ├── git.rs       # Git operations
│           ├── build.rs     # Cargo build wrapper
│           ├── detect.rs    # CPU/system detection
│           └── screens/
│               ├── welcome.rs
│               ├── repo_select.rs
│               ├── commit_picker.rs
│               ├── patch_select.rs
│               ├── build_progress.rs
│               ├── test_runner.rs
│               └── finish.rs
```

### Workflow Screens

```
┌─────────────────────────────────────────────────────────────────┐
│  CODEX XTREME - Build Your Perfect Codex                        │
│─────────────────────────────────────────────────────────────────│
│                                                                  │
│  [1] Welcome / System Check                                      │
│      - Detect CPU (znver5, zen4, native, etc.)                  │
│      - Check for mold linker                                     │
│      - Check Rust toolchain                                      │
│                                                                  │
│  [2] Repository Selection                                        │
│      ○ Use existing: ~/dev/codex (last updated 2h ago)          │
│      ○ Use existing: ~/dev/codex-latest (last updated 15m ago)  │
│      ○ Clone fresh to ~/dev/codex-xtreme-build                  │
│                                                                  │
│  [3] Version / Commit Selection                                  │
│      Base: rust-v0.2.0-alpha.2 (latest release)                 │
│      ┌─────────────────────────────────────────────────────────┐│
│      │ [x] b77bf4d - Aligned feature stage names (#9929)       ││
│      │ [x] 62266b1 - Add thread/unarchive (#9843)              ││
│      │ [ ] 0925138 - chore: update interrupt message           ││
│      │ [x] e471ebc - prompt (#9928)                            ││
│      └─────────────────────────────────────────────────────────┘│
│                                                                  │
│  [4] Patch Selection                                             │
│      ┌─────────────────────────────────────────────────────────┐│
│      │ [x] privacy-patches         Remove Statsig telemetry    ││
│      │ [x] subagent-limit          Increase to 8 threads       ││
│      │ [x] approvals-ui            Simplified 4-preset system  ││
│      │ [ ] cargo-config            Linux x86_64 optimizations  ││
│      └─────────────────────────────────────────────────────────┘│
│                                                                  │
│  [5] Build Configuration                                         │
│      Profile: xtreme (LTO=fat, codegen-units=1)                 │
│      Target CPU: znver5 (AMD Zen 5 - auto-detected)             │
│      Linker: mold (detected)                                     │
│                                                                  │
│  [6] Build Progress                                              │
│      ████████████████████░░░░░░░░░░  65%  Building codex-core   │
│                                                                  │
│  [7] Test Verification                                           │
│      ○ cargo check --all           ✓ Passed                     │
│      ○ cargo test -p codex-common  ✓ Passed                     │
│      ○ cargo test -p codex-core    ◌ Running...                 │
│                                                                  │
│  [8] Finish                                                      │
│      Build complete! Binary at: ~/dev/codex/target/xtreme/codex │
│      Set alias? [Y/n]                                            │
│      alias codex="~/dev/codex/target/xtreme/codex"              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Key Components

### 1. System Detection (`detect.rs`)
```rust
pub struct SystemInfo {
    cpu_vendor: CpuVendor,      // AMD, Intel, Other
    cpu_model: String,          // "AMD Ryzen 9 9950X"
    cpu_target: String,         // "znver5", "alderlake", "native"
    has_mold: bool,
    rust_version: String,
    cargo_version: String,
}

fn detect_cpu_target() -> String {
    // Parse /proc/cpuinfo or use CPUID
    // Map to Rust target-cpu values
}
```

### 2. Git Operations (`git.rs`)
```rust
pub struct CodexRepo {
    path: PathBuf,
    remote_url: String,
    last_fetch: DateTime<Utc>,
}

impl CodexRepo {
    fn find_existing() -> Vec<CodexRepo>;
    fn clone_fresh(dest: &Path) -> Result<CodexRepo>;
    fn fetch_updates(&self) -> Result<()>;
    fn get_latest_release_tag(&self) -> Result<String>;
    fn get_commits_since(&self, tag: &str) -> Result<Vec<Commit>>;
    fn cherry_pick(&self, commits: &[&str]) -> Result<()>;
}
```

### 3. Build Configuration (`build.rs`)
```rust
pub struct BuildConfig {
    profile: BuildProfile,
    target_cpu: String,
    use_mold: bool,
    lto: LtoMode,
    codegen_units: u8,
}

pub enum BuildProfile {
    Release,
    Xtreme,  // Our custom high-perf profile
}

impl BuildConfig {
    fn to_rustflags(&self) -> String;
    fn to_cargo_args(&self) -> Vec<String>;
    fn run_build(&self, workspace: &Path, progress: impl Fn(BuildProgress));
}
```

### 4. App State Machine (`app.rs`)
```rust
pub enum Screen {
    Welcome,
    RepoSelect,
    CommitPicker,
    PatchSelect,
    BuildConfig,
    Building,
    Testing,
    Finish,
}

pub struct App {
    screen: Screen,
    system_info: SystemInfo,
    selected_repo: Option<CodexRepo>,
    selected_commits: Vec<String>,
    selected_patches: Vec<String>,
    build_config: BuildConfig,
    build_progress: Option<BuildProgress>,
    test_results: Vec<TestResult>,
}
```

### 5. UI Rendering (`ui.rs`)
```rust
fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Welcome => render_welcome(frame, app),
        Screen::RepoSelect => render_repo_select(frame, app),
        // ...
    }
}
```

## Dependencies to Add

```toml
[dependencies]
# TUI
ratatui = "0.29"
crossterm = "0.28"

# Async for background tasks
tokio = { version = "1", features = ["full"] }

# System detection
sysinfo = "0.32"
raw-cpuid = "11"

# Git operations
gix = "0.68"  # Pure Rust git, or shell out to git CLI

# Progress parsing
indicatif = "0.17"  # For parsing cargo output

# Time
chrono = "0.4"
```

## Build Profile (injected into target Cargo.toml)

```toml
[profile.xtreme]
inherits = "release"
lto = "fat"
codegen-units = 1
opt-level = 3
strip = false
debug = 1  # Keep some debug info for profiling
panic = "abort"
overflow-checks = false

[profile.xtreme.build-override]
opt-level = 3

[profile.xtreme.package."*"]
opt-level = 3
```

## Test Verification Strategy

1. `cargo check --all` - Quick syntax/type check
2. `cargo test -p codex-common` - Fast unit tests for patched crate
3. `cargo test -p codex-otel` - Verify privacy patches
4. `cargo test -p codex-core --lib` - Core library tests (skip integration)
5. Optional: Full test suite if user wants

## Alias Setup

```bash
# Detect shell
if [ -n "$ZSH_VERSION" ]; then
    RC_FILE="$HOME/.zshrc"
elif [ -n "$BASH_VERSION" ]; then
    RC_FILE="$HOME/.bashrc"
fi

# Add or update alias
grep -q "alias codex=" "$RC_FILE" && \
    sed -i 's|alias codex=.*|alias codex="'"$BINARY_PATH"'"|' "$RC_FILE" || \
    echo 'alias codex="'"$BINARY_PATH"'"' >> "$RC_FILE"
```

## Implementation Order

1. **Phase 1: Core Infrastructure**
   - [ ] Set up binary structure in `src/bin/codex_xtreme/`
   - [ ] Basic app state machine
   - [ ] Terminal setup/teardown
   - [ ] Navigation (Tab/Shift+Tab, Enter, Esc)

2. **Phase 2: System Detection**
   - [ ] CPU detection and target-cpu mapping
   - [ ] Mold linker detection
   - [ ] Rust toolchain check
   - [ ] Welcome screen rendering

3. **Phase 3: Repository Management**
   - [ ] Find existing Codex repos
   - [ ] Display repo list with metadata
   - [ ] Clone fresh option
   - [ ] Fetch/update existing repos

4. **Phase 4: Commit Selection**
   - [ ] Get latest release tag
   - [ ] List commits since release
   - [ ] Multi-select with Space
   - [ ] Cherry-pick selected commits

5. **Phase 5: Patch Selection**
   - [ ] Load available patches from codex-patcher
   - [ ] Display descriptions
   - [ ] Multi-select patches
   - [ ] Show compatibility status

6. **Phase 6: Build System**
   - [ ] Inject xtreme profile
   - [ ] Apply patches via codex-patcher lib
   - [ ] Run cargo build with progress
   - [ ] Parse cargo JSON output for progress bar

7. **Phase 7: Test & Finish**
   - [ ] Run verification tests
   - [ ] Display results
   - [ ] Alias setup prompt
   - [ ] Success screen with binary path

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` / `↓` | Next item/screen |
| `Shift+Tab` / `↑` | Previous item/screen |
| `Space` | Toggle selection |
| `Enter` | Confirm / Proceed |
| `Esc` | Back / Cancel |
| `q` | Quit |
| `?` | Help |
| `a` | Select all (in lists) |
| `n` | Select none (in lists) |

## Error Handling

- Network errors during clone → Retry prompt
- Build failures → Show error, offer to retry or skip tests
- Test failures → Show which tests failed, continue anyway?
- Git conflicts during cherry-pick → Skip commit or abort

## Future Enhancements

- Save/load build profiles
- Watch for new releases
- Automatic updates
- Build cache management
- Cross-compilation support
