-- Token-efficient query pack for codex version patch DB.
-- DB expected at: /home/zack/dev/codex-patcher/db/codex-version-patches.sqlite3

-- 1) High-level range summary
SELECT * FROM v_range_summary;

-- 2) Patch-by-patch commit summary (ordered timeline)
SELECT
  range_seq, from_tag, to_tag, commit_ordinal, commit_sha, author_date,
  subject, files_changed, hunk_count, line_additions, line_deletions,
  block_additions, block_deletions
FROM v_commit_patch_summary;

-- 3) Every exact code block event (verbatim + / - code block text)
SELECT
  event_id, from_tag, to_tag, commit_ordinal, commit_sha, old_path, new_path,
  hunk_index, block_index, op, block_hash, block_diff
FROM v_block_timeline
ORDER BY range_seq, commit_ordinal, file_index, hunk_index, block_index;

-- 4) Track one block hash through time
-- Replace :hash with your hash.
SELECT
  event_id, from_tag, to_tag, commit_ordinal, commit_sha, author_date, subject,
  old_path, new_path, hunk_index, block_index, op, block_diff
FROM v_block_timeline
WHERE block_hash = :hash
ORDER BY event_id;

-- 5) Track changes in one file across all ranges
-- Replace :path with file path.
SELECT
  event_id, from_tag, to_tag, commit_ordinal, commit_sha, subject,
  hunk_index, block_index, op, block_diff
FROM v_block_timeline
WHERE old_path = :path OR new_path = :path
ORDER BY event_id;

-- 6) Most volatile files by block events
SELECT
  COALESCE(NULLIF(new_path, ''), old_path) AS file_path,
  COUNT(*) AS block_events,
  SUM(CASE WHEN op='add' THEN 1 ELSE 0 END) AS adds,
  SUM(CASE WHEN op='del' THEN 1 ELSE 0 END) AS dels
FROM v_block_timeline
GROUP BY file_path
ORDER BY block_events DESC
LIMIT 100;

-- 7) Block lifecycle summary
SELECT * FROM v_block_lifecycle
ORDER BY (add_events + del_events) DESC, block_hash
LIMIT 200;
