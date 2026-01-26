# Practical Patch Examples

Real-world examples you can copy and adapt for your needs.

## Table of Contents

1. [Privacy & Security](#privacy--security)
2. [Performance Optimization](#performance-optimization)
3. [UI Customization](#ui-customization)
4. [Debugging & Development](#debugging--development)
5. [Configuration Changes](#configuration-changes)
6. [Build System](#build-system)

---

## Privacy & Security

### Example 1: Remove Analytics Tracking

**Problem:** Application sends analytics to external service

**Solution:**

```toml
[meta]
name = "remove-analytics"
workspace_relative = true

[[patches]]
id = "disable-analytics-call"
file = "src/analytics.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn track_event(event: &str) {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn track_event(event: &str) {
    // PRIVACY: Analytics disabled
    log::debug!("Analytics call suppressed: {}", event);
}
'''
```

### Example 2: Disable Telemetry Completely

**Problem:** Telemetry enabled by default

```toml
[[patches]]
id = "disable-telemetry-default"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
impl Default for TelemetryConfig {
    fn default() -> Self {
        $$$BODY
    }
}
'''

[patches.operation]
type = "replace"
text = '''
impl Default for TelemetryConfig {
    fn default() -> Self {
        TelemetryConfig {
            enabled: false,        // Changed from true
            endpoint: None,         // Changed from Some(...)
            sample_rate: 0.0,      // Changed from 0.1
        }
    }
}
'''
```

### Example 3: Remove Hardcoded API Endpoint

**Problem:** Hardcoded cloud endpoint

```toml
[[patches]]
id = "remove-cloud-endpoint"
file = "src/api.rs"

[patches.query]
type = "ast-grep"
pattern = '''
const CLOUD_ENDPOINT: &str = $URL;
'''

[patches.operation]
type = "delete"
insert_comment = "// PRIVACY: Cloud endpoint removed"
```

---

## Performance Optimization

### Example 4: Increase Thread Pool Size

**Problem:** Default thread pool too small for your hardware

```toml
[[patches]]
id = "increase-thread-pool"
file = "src/executor.rs"

[patches.query]
type = "ast-grep"
pattern = "const DEFAULT_THREADS: usize = $VALUE;"

[patches.operation]
type = "replace"
text = "const DEFAULT_THREADS: usize = 32;"  # Match your CPU cores
```

### Example 5: Adjust Cache Size

**Problem:** Cache too small for your workload

```toml
[[patches]]
id = "increase-cache-size"
file = "src/cache.rs"

[patches.query]
type = "ast-grep"
pattern = "const CACHE_SIZE: usize = $VALUE;"

[patches.operation]
type = "replace"
text = "const CACHE_SIZE: usize = 10_000;"  # 10x increase
```

### Example 6: Change Timeout Values

**Problem:** Timeouts too aggressive

```toml
[[patches]]
id = "increase-timeouts"
file = "src/http.rs"

[patches.query]
type = "ast-grep"
pattern = "const REQUEST_TIMEOUT: Duration = Duration::from_secs($SECS);"

[patches.operation]
type = "replace"
text = "const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);"  # 5 minutes
```

---

## UI Customization

### Example 7: Change Default Theme

**Problem:** Dark theme by default, you want light

```toml
[[patches]]
id = "light-theme-default"
file = "src/ui/theme.rs"

[patches.query]
type = "ast-grep"
pattern = '''
impl Default for Theme {
    fn default() -> Self {
        $$$BODY
    }
}
'''

[patches.operation]
type = "replace"
text = '''
impl Default for Theme {
    fn default() -> Self {
        Theme::Light  // Changed from Theme::Dark
    }
}
'''
```

### Example 8: Customize Error Messages

**Problem:** Error messages too technical

```toml
[[patches]]
id = "friendly-errors"
file = "src/errors.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn format_error(err: &Error) -> String {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn format_error(err: &Error) -> String {
    // Friendly error messages
    match err {
        Error::NotFound => "Item not found. Please check the ID and try again.".to_string(),
        Error::Timeout => "Request took too long. Please try again.".to_string(),
        _ => format!("An error occurred: {}", err),
    }
}
'''
```

---

## Debugging & Development

### Example 9: Enable Debug Logging

**Problem:** Need more verbose logs

```toml
[[patches]]
id = "enable-debug-logging"
file = "src/logging.rs"

[patches.query]
type = "ast-grep"
pattern = "const DEFAULT_LOG_LEVEL: &str = $LEVEL;"

[patches.operation]
type = "replace"
text = "const DEFAULT_LOG_LEVEL: &str = \"debug\";"  # Changed from "info"
```

### Example 10: Add Profiling Points

**Problem:** Need performance metrics

```toml
[[patches]]
id = "add-profiling"
file = "src/processor.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn process_batch(items: Vec<Item>) -> Result<()> {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn process_batch(items: Vec<Item>) -> Result<()> {
    let _timer = crate::metrics::Timer::new("process_batch");

    // Original implementation
    for item in items {
        process_item(item)?;
    }

    Ok(())
}
'''
```

### Example 11: Disable Rate Limiting (Dev Mode)

**Problem:** Rate limits annoying during development

```toml
[[patches]]
id = "disable-rate-limits-dev"
file = "src/rate_limit.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn check_rate_limit(&self) -> Result<()> {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn check_rate_limit(&self) -> Result<()> {
    // DEV: Rate limiting disabled
    Ok(())
}
'''
```

---

## Configuration Changes

### Example 12: Change Default Port

**Problem:** Port conflicts with another service

```toml
[[patches]]
id = "change-default-port"
file = "src/server.rs"

[patches.query]
type = "ast-grep"
pattern = "const DEFAULT_PORT: u16 = $PORT;"

[patches.operation]
type = "replace"
text = "const DEFAULT_PORT: u16 = 8080;"  # Changed from 3000
```

### Example 13: Modify Default Paths

**Problem:** Data directory should be elsewhere

```toml
[[patches]]
id = "custom-data-dir"
file = "src/paths.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn default_data_dir() -> PathBuf {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn default_data_dir() -> PathBuf {
    PathBuf::from("/opt/myapp/data")  // Custom location
}
'''
```

### Example 14: Enable Feature Flag

**Problem:** Feature disabled by default, you want it enabled

```toml
[[patches]]
id = "enable-feature-x"
file = "src/features.rs"

[patches.query]
type = "ast-grep"
pattern = "const FEATURE_X_ENABLED: bool = $VALUE;"

[patches.operation]
type = "replace"
text = "const FEATURE_X_ENABLED: bool = true;"  # Changed from false
```

---

## Build System

### Example 15: Add Custom Cargo Profile

**Problem:** Need optimized profile for your CPU

```toml
[meta]
name = "build-optimization"

[[patches]]
id = "custom-profile"
file = "Cargo.toml"

# Note: TOML operations currently require manual application
# tail -n +10 patches/build-optimization.toml >> Cargo.toml

[patches.operation]
type = "insert-section"
text = '''
[profile.optimized]
inherits = "release"
lto = "fat"
codegen-units = 1
opt-level = 3
strip = false
debug = 1

[profile.optimized.build-override]
opt-level = 3

[profile.optimized.package."*"]
opt-level = 3
'''
after_section = "profile.release"
```

### Example 16: Change Optimization Level

**Problem:** Debug builds too slow

```toml
[[patches]]
id = "optimize-debug"
file = "Cargo.toml"

# Note: Modify existing profile
[patches.operation]
type = "replace-value"
value = "1"  # opt-level = 1 for debug

# Or manually:
# [profile.dev]
# opt-level = 1  # Changed from 0
```

---

## Combined Example: Privacy Suite

A complete patch file with multiple related patches:

```toml
[meta]
name = "privacy-suite"
description = "Remove all tracking and telemetry"
version_range = ">=0.88.0"
workspace_relative = true

# Disable analytics
[[patches]]
id = "disable-analytics"
file = "src/analytics.rs"

[patches.query]
type = "ast-grep"
pattern = '''
pub fn init_analytics() -> Result<()> {
    $$$BODY
}
'''

[patches.operation]
type = "replace"
text = '''
pub fn init_analytics() -> Result<()> {
    // PRIVACY: Analytics disabled
    Ok(())
}
'''

# Remove tracking endpoint
[[patches]]
id = "remove-endpoint"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = "const TRACKING_ENDPOINT: &str = $URL;"

[patches.operation]
type = "delete"
insert_comment = "// PRIVACY: Tracking endpoint removed"

# Disable telemetry by default
[[patches]]
id = "disable-telemetry"
file = "src/config.rs"

[patches.query]
type = "ast-grep"
pattern = '''
impl Default for Config {
    fn default() -> Self {
        $$$BODY
    }
}
'''

[patches.operation]
type = "replace"
text = '''
impl Default for Config {
    fn default() -> Self {
        Config {
            telemetry_enabled: false,  // Changed
            analytics_enabled: false,   // Changed
            error_reporting: false,     // Changed
            // ... other fields unchanged
        }
    }
}
'''
```

## Usage

Apply a single example:

```bash
# Copy example to a file
cat > patches/my-patch.toml << 'EOF'
[paste example here]
EOF

# Test it
codex-patcher apply --patches patches/my-patch.toml --dry-run --diff

# Apply it
codex-patcher apply --patches patches/my-patch.toml
```

## Adapting Examples

To adapt these examples for your code:

1. **Find your target code:**
   ```bash
   rg "function_name" /your/project
   ```

2. **Update the patch:**
   - Change `file` to your file path
   - Update `pattern` to match your code structure
   - Modify `text` with your replacement

3. **Test before applying:**
   ```bash
   codex-patcher apply --dry-run --diff
   ```

## More Examples

See the included patch files:
- `patches/privacy.toml` - Real production privacy patches
- `patches/zack-profile.toml` - Real build optimization
- `patches/README.md` - Complete syntax reference

## Getting Ideas

Search your project for common patterns:

```bash
# Find all constants
rg "^const " --type rust

# Find all Defaults
rg "impl Default for" --type rust

# Find all public functions
rg "pub fn " --type rust

# Find feature flags
rg "feature.*=.*true|false" --type rust
```

Then create patches for the ones you want to change!
