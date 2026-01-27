#!/usr/bin/env bash
# Find the MAX_CONCURRENT_SUBAGENTS constant location in Codex codebase

set -euo pipefail

CODEX_ROOT="${1:-$HOME/dev/codex/codex-rs}"

if [ ! -d "$CODEX_ROOT" ]; then
    echo "Error: Codex root not found: $CODEX_ROOT"
    echo "Usage: $0 [codex-rs-path]"
    exit 1
fi

echo "Searching for MAX_CONCURRENT_SUBAGENTS in $CODEX_ROOT..."
echo

# Search for the constant
RESULTS=$(grep -r "MAX_CONCURRENT_SUBAGENTS" "$CODEX_ROOT" --include="*.rs" 2>/dev/null || true)

if [ -z "$RESULTS" ]; then
    echo "❌ MAX_CONCURRENT_SUBAGENTS not found"
    echo
    echo "Alternative patterns to search:"
    echo "  - max_concurrent_subagents"
    echo "  - MAX_SUBAGENTS"
    echo "  - concurrent_subagents"
    echo "  - subagent_limit"
    exit 1
fi

echo "✅ Found MAX_CONCURRENT_SUBAGENTS:"
echo
echo "$RESULTS"
echo

# Extract file paths and show with context
echo "Detailed context:"
echo "================"
echo "$RESULTS" | cut -d: -f1 | sort -u | while read -r file; do
    echo
    echo "File: $file"
    echo "---"
    grep -B2 -A2 "MAX_CONCURRENT_SUBAGENTS" "$file" | sed 's/^/  /'
done

echo
echo "To update the patch file:"
echo "1. Edit patches/subagent-limit.toml"
echo "2. Change the 'file' field to the correct path (relative to workspace root)"
echo "3. Example: file = \"agent/src/config.rs\""
