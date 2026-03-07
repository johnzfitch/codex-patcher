#!/usr/bin/env python3
"""Build an SQLite timeline of exact patch blocks across Codex tag ranges.

This script extracts every commit patch between:
  - rust-v0.105.0 -> rust-v0.106.0
  - rust-v0.106.0 -> rust-v0.107.0
  - rust-v0.107.0 -> rust-v0.108.0-alpha.2

For each commit it stores:
  - file-level patches
  - hunk-level unified diff text
  - contiguous added/deleted code blocks (verbatim)
  - deduplicated block content hash for timeline tracking
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable
import re


DEFAULT_TAGS = [
    "rust-v0.105.0",
    "rust-v0.106.0",
    "rust-v0.107.0",
    "rust-v0.108.0-alpha.2",
]


DIFF_RE = re.compile(r"^diff --git a/(.*) b/(.*)$")
HUNK_RE = re.compile(
    r"^@@ -(?P<old_start>\d+)(?:,(?P<old_lines>\d+))? "
    r"\+(?P<new_start>\d+)(?:,(?P<new_lines>\d+))? @@(?: ?(?P<context>.*))?$"
)


@dataclass
class Block:
    op: str  # 'add' | 'del'
    start_old: int
    start_new: int
    lines: list[str] = field(default_factory=list)

    @property
    def line_count(self) -> int:
        return len(self.lines)

    @property
    def content(self) -> str:
        return "\n".join(self.lines)

    @property
    def diff_text(self) -> str:
        prefix = "+" if self.op == "add" else "-"
        return "\n".join(f"{prefix}{line}" for line in self.lines)

    @property
    def block_hash(self) -> str:
        return hashlib.sha256(self.content.encode("utf-8")).hexdigest()


@dataclass
class Hunk:
    hunk_index: int
    old_start: int
    old_lines: int
    new_start: int
    new_lines: int
    header_text: str
    body_lines: list[str]
    blocks: list[Block] = field(default_factory=list)

    @property
    def hunk_text(self) -> str:
        return "\n".join([self.header_text, *self.body_lines])


@dataclass
class FilePatch:
    file_index: int
    old_path: str
    new_path: str
    change_type: str = "modify"
    is_binary: bool = False
    similarity: int | None = None
    hunks: list[Hunk] = field(default_factory=list)

    @property
    def additions(self) -> int:
        return sum(
            block.line_count
            for hunk in self.hunks
            for block in hunk.blocks
            if block.op == "add"
        )

    @property
    def deletions(self) -> int:
        return sum(
            block.line_count
            for hunk in self.hunks
            for block in hunk.blocks
            if block.op == "del"
        )


def run_git(repo: Path, args: list[str]) -> str:
    cmd = ["git", "-C", str(repo), *args]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"git command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}"
        )
    return proc.stdout


def get_tag_commit(repo: Path, tag: str) -> str:
    return run_git(repo, ["rev-list", "-n", "1", tag]).strip()


def get_commit_date(repo: Path, commit_sha: str) -> str:
    return run_git(repo, ["show", "-s", "--format=%cI", commit_sha]).strip()


def list_commits_in_range(repo: Path, range_expr: str) -> list[str]:
    out = run_git(repo, ["rev-list", "--reverse", range_expr])
    return [line.strip() for line in out.splitlines() if line.strip()]


def commit_metadata(repo: Path, commit_sha: str) -> dict[str, str]:
    fmt = "%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%b"
    out = run_git(repo, ["show", "-s", f"--format={fmt}", commit_sha])
    parts = out.split("\x1f", 6)
    if len(parts) != 7:
        raise RuntimeError(f"unexpected commit metadata format for {commit_sha}")
    return {
        "commit_sha": parts[0].strip(),
        "parent_shas": parts[1].strip(),
        "author_name": parts[2].strip(),
        "author_email": parts[3].strip(),
        "author_date": parts[4].strip(),
        "subject": parts[5].strip(),
        "body": parts[6].strip(),
    }


def commit_patch_text(repo: Path, commit_sha: str) -> str:
    return run_git(
        repo,
        [
            "show",
            "--format=",
            "--patch",
            "--find-renames",
            "--find-copies",
            "--full-index",
            "--unified=3",
            "--no-color",
            commit_sha,
        ],
    )


def parse_hunk_blocks(old_start: int, new_start: int, body_lines: list[str]) -> list[Block]:
    blocks: list[Block] = []
    old_ln = old_start
    new_ln = new_start
    current: Block | None = None

    def flush() -> None:
        nonlocal current
        if current and current.lines:
            blocks.append(current)
        current = None

    for raw in body_lines:
        if not raw:
            flush()
            continue

        prefix = raw[0]
        if prefix == " ":
            flush()
            old_ln += 1
            new_ln += 1
            continue

        if prefix == "-":
            if current is None or current.op != "del":
                flush()
                current = Block(op="del", start_old=old_ln, start_new=new_ln)
            current.lines.append(raw[1:])
            old_ln += 1
            continue

        if prefix == "+":
            if current is None or current.op != "add":
                flush()
                current = Block(op="add", start_old=old_ln, start_new=new_ln)
            current.lines.append(raw[1:])
            new_ln += 1
            continue

        if raw.startswith("\\ No newline at end of file"):
            flush()
            continue

        flush()

    flush()
    return blocks


def parse_patch(text: str) -> list[FilePatch]:
    lines = text.splitlines()
    files: list[FilePatch] = []
    current: FilePatch | None = None
    i = 0
    file_index = 0

    while i < len(lines):
        line = lines[i]

        if line.startswith("diff --git "):
            if current is not None:
                files.append(current)
            file_index += 1
            match = DIFF_RE.match(line)
            if match:
                old_path, new_path = match.group(1), match.group(2)
            else:
                # Defensive fallback if path parsing fails.
                old_path, new_path = "", ""
            current = FilePatch(
                file_index=file_index,
                old_path=old_path,
                new_path=new_path,
            )
            i += 1
            continue

        if current is None:
            i += 1
            continue

        if line.startswith("new file mode "):
            current.change_type = "add"
            i += 1
            continue
        if line.startswith("deleted file mode "):
            current.change_type = "delete"
            i += 1
            continue
        if line.startswith("rename from "):
            current.change_type = "rename"
            current.old_path = line[len("rename from ") :]
            i += 1
            continue
        if line.startswith("rename to "):
            current.new_path = line[len("rename to ") :]
            i += 1
            continue
        if line.startswith("copy from "):
            current.change_type = "copy"
            current.old_path = line[len("copy from ") :]
            i += 1
            continue
        if line.startswith("copy to "):
            current.new_path = line[len("copy to ") :]
            i += 1
            continue
        if line.startswith("similarity index "):
            try:
                current.similarity = int(line[len("similarity index ") :].rstrip("%"))
            except ValueError:
                current.similarity = None
            i += 1
            continue
        if line.startswith("Binary files ") or line == "GIT binary patch":
            current.is_binary = True
            i += 1
            continue
        if line.startswith("--- "):
            old_name = line[4:]
            if old_name.startswith("a/"):
                current.old_path = old_name[2:]
            elif old_name != "/dev/null":
                current.old_path = old_name
            i += 1
            continue
        if line.startswith("+++ "):
            new_name = line[4:]
            if new_name.startswith("b/"):
                current.new_path = new_name[2:]
            elif new_name != "/dev/null":
                current.new_path = new_name
            i += 1
            continue

        if line.startswith("@@ "):
            match = HUNK_RE.match(line)
            if not match:
                i += 1
                continue

            old_start = int(match.group("old_start"))
            old_lines = int(match.group("old_lines") or "1")
            new_start = int(match.group("new_start"))
            new_lines = int(match.group("new_lines") or "1")
            hunk_body: list[str] = []
            i += 1
            while i < len(lines):
                candidate = lines[i]
                if candidate.startswith("diff --git ") or candidate.startswith("@@ "):
                    break
                hunk_body.append(candidate)
                i += 1

            blocks = parse_hunk_blocks(old_start, new_start, hunk_body)
            hunk = Hunk(
                hunk_index=len(current.hunks) + 1,
                old_start=old_start,
                old_lines=old_lines,
                new_start=new_start,
                new_lines=new_lines,
                header_text=line,
                body_lines=hunk_body,
                blocks=blocks,
            )
            current.hunks.append(hunk)
            continue

        i += 1

    if current is not None:
        files.append(current)

    return files


def create_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tags (
            tag_name TEXT PRIMARY KEY,
            commit_sha TEXT NOT NULL,
            commit_date TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ranges (
            range_id INTEGER PRIMARY KEY AUTOINCREMENT,
            seq INTEGER NOT NULL,
            from_tag TEXT NOT NULL REFERENCES tags(tag_name),
            to_tag TEXT NOT NULL REFERENCES tags(tag_name),
            range_expr TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS commits (
            commit_sha TEXT PRIMARY KEY,
            parent_shas TEXT NOT NULL,
            author_name TEXT NOT NULL,
            author_email TEXT NOT NULL,
            author_date TEXT NOT NULL,
            subject TEXT NOT NULL,
            body TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS range_commits (
            range_id INTEGER NOT NULL REFERENCES ranges(range_id),
            commit_sha TEXT NOT NULL REFERENCES commits(commit_sha),
            ordinal INTEGER NOT NULL,
            PRIMARY KEY (range_id, ordinal),
            UNIQUE (range_id, commit_sha)
        );

        CREATE TABLE IF NOT EXISTS patch_files (
            file_patch_id INTEGER PRIMARY KEY AUTOINCREMENT,
            commit_sha TEXT NOT NULL REFERENCES commits(commit_sha),
            file_index INTEGER NOT NULL,
            old_path TEXT NOT NULL,
            new_path TEXT NOT NULL,
            change_type TEXT NOT NULL,
            is_binary INTEGER NOT NULL DEFAULT 0,
            similarity INTEGER,
            additions INTEGER NOT NULL DEFAULT 0,
            deletions INTEGER NOT NULL DEFAULT 0,
            UNIQUE (commit_sha, file_index)
        );

        CREATE TABLE IF NOT EXISTS hunks (
            hunk_id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_patch_id INTEGER NOT NULL REFERENCES patch_files(file_patch_id),
            hunk_index INTEGER NOT NULL,
            old_start INTEGER NOT NULL,
            old_lines INTEGER NOT NULL,
            new_start INTEGER NOT NULL,
            new_lines INTEGER NOT NULL,
            header_text TEXT NOT NULL,
            hunk_text TEXT NOT NULL,
            UNIQUE (file_patch_id, hunk_index)
        );

        CREATE TABLE IF NOT EXISTS code_blocks (
            block_hash TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            line_count INTEGER NOT NULL,
            byte_count INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS block_events (
            event_id INTEGER PRIMARY KEY AUTOINCREMENT,
            range_id INTEGER NOT NULL REFERENCES ranges(range_id),
            commit_sha TEXT NOT NULL REFERENCES commits(commit_sha),
            file_patch_id INTEGER NOT NULL REFERENCES patch_files(file_patch_id),
            hunk_id INTEGER NOT NULL REFERENCES hunks(hunk_id),
            block_index INTEGER NOT NULL,
            op TEXT NOT NULL CHECK (op IN ('add', 'del')),
            start_old INTEGER NOT NULL,
            start_new INTEGER NOT NULL,
            line_count INTEGER NOT NULL,
            block_hash TEXT NOT NULL REFERENCES code_blocks(block_hash),
            block_diff TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS commit_stats (
            commit_sha TEXT PRIMARY KEY REFERENCES commits(commit_sha),
            files_changed INTEGER NOT NULL DEFAULT 0,
            hunk_count INTEGER NOT NULL DEFAULT 0,
            line_additions INTEGER NOT NULL DEFAULT 0,
            line_deletions INTEGER NOT NULL DEFAULT 0,
            block_additions INTEGER NOT NULL DEFAULT 0,
            block_deletions INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_range_commits_commit ON range_commits(commit_sha);
        CREATE INDEX IF NOT EXISTS idx_patch_files_commit ON patch_files(commit_sha);
        CREATE INDEX IF NOT EXISTS idx_patch_files_path_new ON patch_files(new_path);
        CREATE INDEX IF NOT EXISTS idx_patch_files_path_old ON patch_files(old_path);
        CREATE INDEX IF NOT EXISTS idx_hunks_file_patch ON hunks(file_patch_id);
        CREATE INDEX IF NOT EXISTS idx_block_events_commit ON block_events(commit_sha);
        CREATE INDEX IF NOT EXISTS idx_block_events_hash ON block_events(block_hash);
        CREATE INDEX IF NOT EXISTS idx_block_events_hunk ON block_events(hunk_id);
        CREATE INDEX IF NOT EXISTS idx_block_events_range_commit ON block_events(range_id, commit_sha);
        """
    )


def reset_data(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        DELETE FROM block_events;
        DELETE FROM commit_stats;
        DELETE FROM code_blocks;
        DELETE FROM hunks;
        DELETE FROM patch_files;
        DELETE FROM range_commits;
        DELETE FROM commits;
        DELETE FROM ranges;
        DELETE FROM tags;
        DELETE FROM metadata;
        """
    )


def create_views(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        DROP VIEW IF EXISTS v_commit_patch_summary;
        CREATE VIEW v_commit_patch_summary AS
        SELECT
            r.seq AS range_seq,
            r.from_tag,
            r.to_tag,
            rc.ordinal AS commit_ordinal,
            c.commit_sha,
            c.author_date,
            c.author_name,
            c.subject,
            cs.files_changed,
            cs.hunk_count,
            cs.line_additions,
            cs.line_deletions,
            cs.block_additions,
            cs.block_deletions
        FROM range_commits rc
        JOIN ranges r ON r.range_id = rc.range_id
        JOIN commits c ON c.commit_sha = rc.commit_sha
        JOIN commit_stats cs ON cs.commit_sha = c.commit_sha
        ORDER BY r.seq, rc.ordinal;

        DROP VIEW IF EXISTS v_block_timeline;
        CREATE VIEW v_block_timeline AS
        SELECT
            be.event_id,
            r.seq AS range_seq,
            r.from_tag,
            r.to_tag,
            rc.ordinal AS commit_ordinal,
            be.commit_sha,
            c.author_date,
            c.subject,
            pf.file_index,
            pf.old_path,
            pf.new_path,
            pf.change_type,
            h.hunk_index,
            be.block_index,
            be.op,
            be.start_old,
            be.start_new,
            be.line_count,
            be.block_hash,
            cb.content AS block_content,
            be.block_diff
        FROM block_events be
        JOIN ranges r ON r.range_id = be.range_id
        JOIN range_commits rc
          ON rc.range_id = be.range_id
         AND rc.commit_sha = be.commit_sha
        JOIN commits c ON c.commit_sha = be.commit_sha
        JOIN patch_files pf ON pf.file_patch_id = be.file_patch_id
        JOIN hunks h ON h.hunk_id = be.hunk_id
        JOIN code_blocks cb ON cb.block_hash = be.block_hash
        ORDER BY r.seq, rc.ordinal, pf.file_index, h.hunk_index, be.block_index;

        DROP VIEW IF EXISTS v_range_summary;
        CREATE VIEW v_range_summary AS
        SELECT
            r.range_id,
            r.seq,
            r.from_tag,
            r.to_tag,
            r.range_expr,
            COUNT(DISTINCT rc.commit_sha) AS commits,
            COALESCE(SUM(cs.files_changed), 0) AS files_changed,
            COALESCE(SUM(cs.hunk_count), 0) AS hunks,
            COALESCE(SUM(cs.line_additions), 0) AS line_additions,
            COALESCE(SUM(cs.line_deletions), 0) AS line_deletions,
            COALESCE(SUM(cs.block_additions), 0) AS block_additions,
            COALESCE(SUM(cs.block_deletions), 0) AS block_deletions
        FROM ranges r
        LEFT JOIN range_commits rc ON rc.range_id = r.range_id
        LEFT JOIN commit_stats cs ON cs.commit_sha = rc.commit_sha
        GROUP BY r.range_id, r.seq, r.from_tag, r.to_tag, r.range_expr
        ORDER BY r.seq;

        DROP VIEW IF EXISTS v_block_lifecycle;
        CREATE VIEW v_block_lifecycle AS
        WITH ordered AS (
            SELECT
                be.block_hash,
                be.event_id,
                be.op,
                be.line_count,
                r.from_tag,
                r.to_tag,
                rc.ordinal,
                be.commit_sha
            FROM block_events be
            JOIN ranges r ON r.range_id = be.range_id
            JOIN range_commits rc
              ON rc.range_id = be.range_id
             AND rc.commit_sha = be.commit_sha
        )
        SELECT
            cb.block_hash,
            cb.line_count,
            cb.byte_count,
            SUM(CASE WHEN o.op = 'add' THEN 1 ELSE 0 END) AS add_events,
            SUM(CASE WHEN o.op = 'del' THEN 1 ELSE 0 END) AS del_events,
            SUM(CASE WHEN o.op = 'add' THEN o.line_count ELSE -o.line_count END) AS net_lines,
            CASE
                WHEN SUM(CASE WHEN o.op = 'add' THEN o.line_count ELSE -o.line_count END) > 0
                THEN 1 ELSE 0
            END AS is_active_estimate,
            (
                SELECT o1.commit_sha
                FROM ordered o1
                WHERE o1.block_hash = cb.block_hash
                ORDER BY o1.event_id ASC
                LIMIT 1
            ) AS first_commit,
            (
                SELECT o2.commit_sha
                FROM ordered o2
                WHERE o2.block_hash = cb.block_hash
                ORDER BY o2.event_id DESC
                LIMIT 1
            ) AS last_commit
        FROM code_blocks cb
        LEFT JOIN ordered o ON o.block_hash = cb.block_hash
        GROUP BY cb.block_hash, cb.line_count, cb.byte_count;
        """
    )


def build_db(repo: Path, db_path: Path, tags: list[str]) -> None:
    ranges = [(tags[i], tags[i + 1]) for i in range(len(tags) - 1)]
    db_path.parent.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    try:
        create_schema(conn)
        reset_data(conn)

        conn.execute(
            "INSERT INTO metadata(key, value) VALUES (?, ?)",
            ("source_repo", str(repo)),
        )
        conn.execute(
            "INSERT INTO metadata(key, value) VALUES (?, ?)",
            ("tags_json", json.dumps(tags)),
        )

        for tag in tags:
            commit_sha = get_tag_commit(repo, tag)
            commit_date = get_commit_date(repo, commit_sha)
            conn.execute(
                """
                INSERT INTO tags(tag_name, commit_sha, commit_date)
                VALUES (?, ?, ?)
                """,
                (tag, commit_sha, commit_date),
            )

        for seq, (from_tag, to_tag) in enumerate(ranges, start=1):
            range_expr = f"{from_tag}..{to_tag}"
            cur = conn.execute(
                """
                INSERT INTO ranges(seq, from_tag, to_tag, range_expr)
                VALUES (?, ?, ?, ?)
                """,
                (seq, from_tag, to_tag, range_expr),
            )
            range_id = int(cur.lastrowid)

            commit_shas = list_commits_in_range(repo, range_expr)

            for ordinal, commit_sha in enumerate(commit_shas, start=1):
                meta = commit_metadata(repo, commit_sha)
                conn.execute(
                    """
                    INSERT OR IGNORE INTO commits(
                        commit_sha, parent_shas, author_name, author_email,
                        author_date, subject, body
                    ) VALUES (?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        meta["commit_sha"],
                        meta["parent_shas"],
                        meta["author_name"],
                        meta["author_email"],
                        meta["author_date"],
                        meta["subject"],
                        meta["body"],
                    ),
                )
                conn.execute(
                    """
                    INSERT INTO range_commits(range_id, commit_sha, ordinal)
                    VALUES (?, ?, ?)
                    """,
                    (range_id, commit_sha, ordinal),
                )

                patch_text = commit_patch_text(repo, commit_sha)
                file_patches = parse_patch(patch_text)

                files_changed = 0
                hunk_count = 0
                line_additions = 0
                line_deletions = 0
                block_additions = 0
                block_deletions = 0

                for fp in file_patches:
                    files_changed += 1
                    hunk_count += len(fp.hunks)
                    line_additions += fp.additions
                    line_deletions += fp.deletions

                    cur_fp = conn.execute(
                        """
                        INSERT INTO patch_files(
                            commit_sha, file_index, old_path, new_path,
                            change_type, is_binary, similarity,
                            additions, deletions
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            commit_sha,
                            fp.file_index,
                            fp.old_path,
                            fp.new_path,
                            fp.change_type,
                            1 if fp.is_binary else 0,
                            fp.similarity,
                            fp.additions,
                            fp.deletions,
                        ),
                    )
                    file_patch_id = int(cur_fp.lastrowid)

                    for hunk in fp.hunks:
                        cur_h = conn.execute(
                            """
                            INSERT INTO hunks(
                                file_patch_id, hunk_index,
                                old_start, old_lines, new_start, new_lines,
                                header_text, hunk_text
                            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                            """,
                            (
                                file_patch_id,
                                hunk.hunk_index,
                                hunk.old_start,
                                hunk.old_lines,
                                hunk.new_start,
                                hunk.new_lines,
                                hunk.header_text,
                                hunk.hunk_text,
                            ),
                        )
                        hunk_id = int(cur_h.lastrowid)

                        for block_index, block in enumerate(hunk.blocks, start=1):
                            conn.execute(
                                """
                                INSERT OR IGNORE INTO code_blocks(
                                    block_hash, content, line_count, byte_count
                                ) VALUES (?, ?, ?, ?)
                                """,
                                (
                                    block.block_hash,
                                    block.content,
                                    block.line_count,
                                    len(block.content.encode("utf-8")),
                                ),
                            )
                            conn.execute(
                                """
                                INSERT INTO block_events(
                                    range_id, commit_sha, file_patch_id, hunk_id,
                                    block_index, op, start_old, start_new,
                                    line_count, block_hash, block_diff
                                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                                """,
                                (
                                    range_id,
                                    commit_sha,
                                    file_patch_id,
                                    hunk_id,
                                    block_index,
                                    block.op,
                                    block.start_old,
                                    block.start_new,
                                    block.line_count,
                                    block.block_hash,
                                    block.diff_text,
                                ),
                            )
                            if block.op == "add":
                                block_additions += 1
                            else:
                                block_deletions += 1

                conn.execute(
                    """
                    INSERT OR REPLACE INTO commit_stats(
                        commit_sha, files_changed, hunk_count, line_additions,
                        line_deletions, block_additions, block_deletions
                    ) VALUES (?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        commit_sha,
                        files_changed,
                        hunk_count,
                        line_additions,
                        line_deletions,
                        block_additions,
                        block_deletions,
                    ),
                )

        create_views(conn)
        conn.execute(
            "INSERT OR REPLACE INTO metadata(key, value) VALUES (?, ?)",
            ("build_complete_utc", run_git(repo, ["show", "-s", "--format=%cI", "HEAD"]).strip()),
        )
        conn.commit()
    finally:
        conn.close()


def write_query_pack(db_dir: Path) -> None:
    sql_path = db_dir / "codex_patch_queries.sql"
    sql_path.write_text(
        """-- Token-efficient query pack for codex version patch DB.
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
""",
        encoding="utf-8",
    )


def write_summary(db_path: Path) -> None:
    conn = sqlite3.connect(str(db_path))
    try:
        cur = conn.cursor()
        metrics = {
            "ranges": cur.execute("SELECT COUNT(*) FROM ranges").fetchone()[0],
            "commits": cur.execute("SELECT COUNT(*) FROM commits").fetchone()[0],
            "files": cur.execute("SELECT COUNT(*) FROM patch_files").fetchone()[0],
            "hunks": cur.execute("SELECT COUNT(*) FROM hunks").fetchone()[0],
            "block_events": cur.execute("SELECT COUNT(*) FROM block_events").fetchone()[0],
            "unique_blocks": cur.execute("SELECT COUNT(*) FROM code_blocks").fetchone()[0],
        }
        range_rows = cur.execute(
            """
            SELECT from_tag, to_tag, commits, files_changed, hunks,
                   line_additions, line_deletions,
                   block_additions, block_deletions
            FROM v_range_summary
            ORDER BY seq
            """
        ).fetchall()
    finally:
        conn.close()

    lines: list[str] = [
        "# Codex Patch Timeline Summary",
        "",
        f"Database: `{db_path}`",
        "",
        "## Totals",
        "",
        f"- ranges: {metrics['ranges']}",
        f"- commits: {metrics['commits']}",
        f"- files: {metrics['files']}",
        f"- hunks: {metrics['hunks']}",
        f"- block_events: {metrics['block_events']}",
        f"- unique_blocks: {metrics['unique_blocks']}",
        "",
        "## By Range",
        "",
        "| from_tag | to_tag | commits | files_changed | hunks | +lines | -lines | +blocks | -blocks |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in range_rows:
        lines.append(
            f"| {row[0]} | {row[1]} | {row[2]} | {row[3]} | {row[4]} "
            f"| {row[5]} | {row[6]} | {row[7]} | {row[8]} |"
        )

    summary_path = db_path.parent / "SUMMARY.md"
    summary_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build SQLite DB of exact patch blocks across Codex version ranges."
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path("/home/zack/dev/codex"),
        help="Path to the git repository containing codex tags.",
    )
    parser.add_argument(
        "--db-path",
        type=Path,
        default=Path("/home/zack/dev/codex-patcher/db/codex-version-patches.sqlite3"),
        help="Output SQLite database path.",
    )
    parser.add_argument(
        "--tags",
        nargs="+",
        default=DEFAULT_TAGS,
        help="Ordered tags to diff sequentially.",
    )
    parser.add_argument(
        "--force-rebuild",
        action="store_true",
        help="Rebuild in-place by clearing existing DB tables.",
    )
    return parser.parse_args(list(argv))


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)
    repo = args.repo.resolve()
    db_path = args.db_path.resolve()

    if not repo.exists():
        print(f"error: repo path does not exist: {repo}", file=sys.stderr)
        return 2
    if not (repo / ".git").exists():
        print(f"error: repo is not a git repository: {repo}", file=sys.stderr)
        return 2
    if db_path.exists() and not args.force_rebuild:
        print(
            (
                "error: database already exists. "
                "Use --force-rebuild to clear and rebuild in-place:\n"
                f"  {db_path}"
            ),
            file=sys.stderr,
        )
        return 2

    try:
        build_db(repo=repo, db_path=db_path, tags=args.tags)
        write_query_pack(db_path.parent)
        write_summary(db_path)
    except Exception as exc:  # noqa: BLE001
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(f"built database: {db_path}")
    print(f"wrote query pack: {db_path.parent / 'codex_patch_queries.sql'}")
    print(f"wrote summary: {db_path.parent / 'SUMMARY.md'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

