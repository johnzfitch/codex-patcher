# Codex Changelog Analysis: 0.98.0-alpha.2 through 0.99.0-alpha.3

Source: `openai/codex` GitHub repository
Tags analyzed: `rust-v0.98.0-alpha.2` -> `rust-v0.98.0` -> `rust-v0.99.0-alpha.1` -> `rust-v0.99.0-alpha.2` -> `rust-v0.99.0-alpha.3`

---

## Version Progression Summary

| From | To | Commits | Date |
|------|----|---------|------|
| rust-v0.98.0-alpha.2 | rust-v0.98.0 | 1 | 2026-02-05 |
| rust-v0.98.0 | rust-v0.99.0-alpha.1 | 20 | 2026-02-05 |
| rust-v0.99.0-alpha.1 | rust-v0.99.0-alpha.2 | 10 | 2026-02-05 |
| rust-v0.99.0-alpha.2 | rust-v0.99.0-alpha.3 | 4 | 2026-02-05 |

All 35 commits landed on the same day (2026-02-05), indicating a rapid release cadence.

---

## rust-v0.98.0-alpha.2 -> rust-v0.98.0

**1 commit** - Release bump only.

- `82464689` - Steer mode now stable and enabled by default (`Enter` sends immediately during active turns, `Tab` queues follow-up). Workspace version bumped from `0.0.0` to `0.98.0`.
- Bug fixes: resumeThread() SDK arg ordering, model-instruction handling on model changes, remote compaction mismatch, cloud requirements reload after login.

---

## rust-v0.98.0 -> rust-v0.99.0-alpha.1

**20 commits** - Major feature additions.

### TELEMETRY / PRIVACY

| Commit | Impact | Details |
|--------|--------|---------|
| `f2ffc4e5` **[VIOLATION]** | Telemetry expansion | Adds real OS type + OS version to metrics resource attributes via `os_info` crate. Collects `os_info::get().os_type()` and `os_info::get().version()`. Commit message mentions "calculated a hashed user ID from either auth user id or API key" though code for that isn't visible in diff. New `os_resource_attributes()` fn in `otel/src/metrics/client.rs`. |

### SECURITY / SANDBOX

| Commit | Impact | Details |
|--------|--------|---------|
| `c67120f4` | Test fix | Fixes flaky landlock tests. Adds Landlock capability probing (`linux_sandbox_test_env()`) to skip tests where enforcement unavailable. Test-only, adds `sandbox_mode = "danger-full-access"` to test configs. |
| `97582ac5` | Behavior change | User shell commands now run **alongside** active turns (async) rather than replacing/aborting the current turn. New `UserShellCommandMode::ActiveTurnAuxiliary`. |

### NEW SUBSYSTEMS

| Commit | Impact | Details |
|--------|--------|---------|
| `3b54fd73` | New system | **Hooks subsystem** replaces `UserNotifier`. Introduces `Hooks` service with `HookEvent::AfterAgent`, `HookPayload`, `HookOutcome`. Executes user-specified commands after lifecycle events. Currently only wires up legacy `notify` config. Hook result/stopping semantics not yet implemented. Files: `core/src/hooks/mod.rs`, `registry.rs`, `types.rs`, `user_notification.rs`. |
| `4033f905` | New system | **Resumable backfill** - SQLite rollout backfill is now resumable and repeatable (persisted backfill state table) instead of one-shot-on-db-create. |
| `9ee746af` | Enhancement | Conversation summaries and cwd info read from state DB instead of rollout files. |

### NEW TOOLS

| Commit | Impact | Details |
|--------|--------|---------|
| `41f3b1ba` | New tool | **`get_memory` tool** - Retrieves full thread memory by memory ID from SQLite. Feature-gated as `Feature::MemoryTool` (key: `memory_tool`), `Stage::UnderDevelopment`, default **disabled**. New handler at `core/src/tools/handlers/get_memory.rs`. |

### SUBAGENTS

| Commit | Impact | Details |
|--------|--------|---------|
| `040ecee7` | Model downgrade | Explorer role default model changed from `gpt-5.2-codex` to `gpt-5.1-codex-mini`. Affects sub-agent quality for exploration tasks. |

### TUI

| Commit | Impact | Details |
|--------|--------|---------|
| `b0e5a630` | New command | `/statusline` command for interactive status line configuration with `MultiSelect` widget. |
| `22545bf2` | Enhancement | Sortable resume picker with `Tab` to toggle between created/updated timestamp ordering. |
| `b2424cb6` | Enhancement | Fork info shown in UI: `/fork` command in previous session, session origin displayed as event. |
| `fe1cbd0f` | Bug fix | Correct shutdown handling in TUI. |
| `7b28b350` | Bug fix | Flush input buffer on init to prevent early exit on Windows. |

### OTHER

| Commit | Impact | Details |
|--------|--------|---------|
| `d337b517` | New flag | `--ephemeral` flag for `codex exec` - runs without persisting session rollout files. **Privacy-positive.** |
| `901215e3` | Enhancement | DB repair for missing lines. |
| `68e82e5d` | Enhancement | DB version discrepancy recording. |
| `3582b74d` | Refactor | Isolate `chatgptAuthTokens` concept to auth manager and app-server. |
| `aa46b5cf` | Nit | Backfill "stronger" (minor). |

---

## rust-v0.99.0-alpha.1 -> rust-v0.99.0-alpha.2

**10 commits** - Telemetry expansion, announcements, experimental features API.

### TELEMETRY / PRIVACY

| Commit | Impact | Details |
|--------|--------|---------|
| `901d5b8f` **[VIOLATION]** | Telemetry expansion | Adds `sandbox` and `sandbox_policy` tags to `codex.tool.call` metrics. Reveals sandbox implementation (none/seatbelt/seccomp/windows_sandbox) and policy (read-only/danger-full-access/etc.). Renames `tool_result` -> `tool_result_with_tags` to accept extra tag pairs. |
| `529b5395` **[VIOLATION]** | Telemetry expansion | Adds analytics counters: `codex.thread.fork` (with source: cli_subcommand/slash_command) and `codex.thread.rename`. Tracks user behavioral patterns. |
| `5fdf6f5e` **[POSITIVE]** | Privacy improvement | **Removes** `x-oai-web-search-eligible` header. Web search enablement is now client-side only, no eligibility info sent to backend. Removes `web_search_eligible` parameter from `stream()` method. |

### DEVELOPER FLAGS / HIDDEN FEATURES

| Commit | Impact | Details |
|--------|--------|---------|
| `7e81f636` | New API | **`experimentalFeature/list`** endpoint added to app-server protocol. Returns all experimental features with metadata: `flagName`, `displayName`, `description`, `announcement`, `enabled`, `defaultEnabled`. Paginated with cursor. Useful for discovering hidden features. |

### ANNOUNCEMENTS

| Commit | Impact | Details |
|--------|--------|---------|
| `ddfb8bfd` | Announcement | gpt-5.3-codex announcement (initial). |
| `4df9f202` | Announcement | gpt-5.3-codex announcement (update). |
| `ddd09a93` | Announcement | Priority fix for announcement display. |
| `5602edc1` | Guard | Limits 0.98.0 NUX update announcement to versions < 0.98.0 only. |

### OTHER

| Commit | Impact | Details |
|--------|--------|---------|
| `428a9f60` | Enhancement | Wait for backfill readiness before proceeding. |

---

## rust-v0.99.0-alpha.2 -> rust-v0.99.0-alpha.3

**4 commits** - WebSocket transport, cloud requirements sync.

### NEW SUBSYSTEMS

| Commit | Impact | Details |
|--------|--------|---------|
| `8473096e` | New transport | **WebSocket transport** for app-server. New `--listen <URL>` flag supports `stdio://` (default) and `ws://IP:PORT`. Per-connection session state tracking, connection-aware message routing. **Experimental/unsupported.** New file: `app-server/src/transport.rs` (424 lines). Adds `clap`, `futures`, `tokio-tungstenite` dependencies. |
| `43a7290f` | Enhancement | Sync app-server `configRequirements/read` with refreshed cloud requirements after login. |

---

## Violations Requiring Patches

### 1. OS Fingerprinting in Metrics (f2ffc4e5)
**Severity: HIGH** - Collects real OS type and version, attached to all metrics as resource attributes.
- `os_info::get().os_type()` -> e.g., "Arch Linux", "Ubuntu", "macOS"
- `os_info::get().version()` -> e.g., "6.18.3-arch1-1"
- Combined with existing auth_mode tag, enables device fingerprinting
- **Patch: `os-info-metrics.toml`**

### 2. Sandbox Implementation Leaked in Metrics (901d5b8f)
**Severity: MEDIUM** - Every tool call now reports which sandbox and policy is active.
- `sandbox` tag: none/seatbelt/seccomp/windows_sandbox
- `sandbox_policy` tag: the active sandbox policy string
- Reveals security posture to telemetry endpoint
- **Patch: `sandbox-metrics.toml`**

### 3. User Behavior Tracking via Analytics Counters (529b5395)
**Severity: MEDIUM** - Tracks /rename and /fork command usage with source attribution.
- `codex.thread.fork` counter with source tag
- `codex.thread.rename` counter
- Builds behavioral profile of user session management habits
- **Patch: `analytics-counters.toml`**

---

## Notable Non-Violations

| Area | Details |
|------|---------|
| **Privacy positive** | Web search eligibility header removed (5fdf6f5e). Ephemeral exec flag added (d337b517). |
| **Hooks system** | New, replaces old notifier. Neutral - user-controlled hooks only (3b54fd73). |
| **Memory tool** | Feature-gated, default disabled. No immediate concern (41f3b1ba). |
| **Explorer model downgrade** | `gpt-5.2-codex` -> `gpt-5.1-codex-mini`. Quality concern, not privacy (040ecee7). |
| **WebSocket transport** | Experimental, unsupported. New attack surface if `--listen ws://` is used on a network interface. Worth monitoring (8473096e). |
| **Experimental features API** | Exposes feature flag metadata. Actually useful for our patcher to discover features (7e81f636). |
