#!/usr/bin/env bash
# =============================================================================
# Codex Rollout Cleanup Script
# =============================================================================
# Removes old rollout files to reclaim disk space.
#
# Usage:
#   cleanup-rollouts.sh [--dry-run] [--days N] [--max-size MB]
#
# Options:
#   --dry-run     Show what would be deleted without actually deleting
#   --days N      Delete rollouts older than N days (default: 30)
#   --max-size MB Keep deleting oldest until under this size in MB (default: 500)
#
# Examples:
#   cleanup-rollouts.sh --dry-run           # Preview cleanup with defaults
#   cleanup-rollouts.sh --days 7            # Delete files older than 7 days
#   cleanup-rollouts.sh --max-size 100      # Keep only 100MB of rollouts
# =============================================================================

set -euo pipefail

# Defaults
DRY_RUN=false
MAX_AGE_DAYS=30
MAX_SIZE_MB=500
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
SESSIONS_DIR="$CODEX_HOME/sessions"
ARCHIVED_DIR="$CODEX_HOME/archived_sessions"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --days)
            MAX_AGE_DAYS="$2"
            shift 2
            ;;
        --max-size)
            MAX_SIZE_MB="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Check if sessions directory exists
if [[ ! -d "$SESSIONS_DIR" ]]; then
    echo "No sessions directory found at $SESSIONS_DIR"
    exit 0
fi

# Get current size
current_size_kb=$(du -sk "$SESSIONS_DIR" 2>/dev/null | cut -f1 || echo 0)
current_size_mb=$((current_size_kb / 1024))
echo "Current sessions size: ${current_size_mb}MB (limit: ${MAX_SIZE_MB}MB)"

# Count files
total_files=$(find "$SESSIONS_DIR" -name "*.jsonl" -type f 2>/dev/null | wc -l)
echo "Total rollout files: $total_files"

# Phase 1: Delete by age
echo ""
echo "Phase 1: Deleting files older than $MAX_AGE_DAYS days..."
old_files=$(find "$SESSIONS_DIR" -name "*.jsonl" -type f -mtime +$MAX_AGE_DAYS 2>/dev/null || true)
old_count=$(echo "$old_files" | grep -c . || echo 0)

if [[ $old_count -gt 0 ]]; then
    old_size_kb=$(echo "$old_files" | xargs du -sk 2>/dev/null | awk '{sum+=$1} END {print sum}' || echo 0)
    old_size_mb=$((old_size_kb / 1024))
    echo "  Found $old_count files (${old_size_mb}MB) older than $MAX_AGE_DAYS days"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY RUN] Would delete these files"
    else
        echo "$old_files" | xargs rm -f 2>/dev/null || true
        echo "  Deleted $old_count files"
    fi
else
    echo "  No files older than $MAX_AGE_DAYS days"
fi

# Phase 2: Delete by size (oldest first)
current_size_kb=$(du -sk "$SESSIONS_DIR" 2>/dev/null | cut -f1 || echo 0)
current_size_mb=$((current_size_kb / 1024))

if [[ $current_size_mb -gt $MAX_SIZE_MB ]]; then
    echo ""
    echo "Phase 2: Reducing size from ${current_size_mb}MB to ${MAX_SIZE_MB}MB..."

    # Get files sorted by modification time (oldest first)
    to_delete_mb=$((current_size_mb - MAX_SIZE_MB))
    deleted_mb=0
    deleted_count=0

    while IFS= read -r file; do
        if [[ $deleted_mb -ge $to_delete_mb ]]; then
            break
        fi

        file_size_kb=$(du -sk "$file" 2>/dev/null | cut -f1 || echo 0)
        file_size_mb=$((file_size_kb / 1024))

        if [[ "$DRY_RUN" == "true" ]]; then
            echo "  [DRY RUN] Would delete: $file (${file_size_kb}KB)"
        else
            rm -f "$file" 2>/dev/null || true
        fi

        deleted_mb=$((deleted_mb + file_size_mb))
        deleted_count=$((deleted_count + 1))
    done < <(find "$SESSIONS_DIR" -name "*.jsonl" -type f -printf '%T+ %p\n' 2>/dev/null | sort | cut -d' ' -f2-)

    echo "  Would delete $deleted_count additional files (~${deleted_mb}MB)"
fi

# Phase 3: Clean up empty directories
echo ""
echo "Phase 3: Cleaning empty directories..."
if [[ "$DRY_RUN" == "true" ]]; then
    empty_dirs=$(find "$SESSIONS_DIR" -type d -empty 2>/dev/null | wc -l)
    echo "  [DRY RUN] Would remove $empty_dirs empty directories"
else
    find "$SESSIONS_DIR" -type d -empty -delete 2>/dev/null || true
    echo "  Cleaned empty directories"
fi

# Final stats
echo ""
if [[ "$DRY_RUN" == "false" ]]; then
    final_size_kb=$(du -sk "$SESSIONS_DIR" 2>/dev/null | cut -f1 || echo 0)
    final_size_mb=$((final_size_kb / 1024))
    final_files=$(find "$SESSIONS_DIR" -name "*.jsonl" -type f 2>/dev/null | wc -l)
    echo "Final: ${final_size_mb}MB, $final_files files"
else
    echo "[DRY RUN] No changes made. Run without --dry-run to apply."
fi
