# Agent Instructions

- Read project docs before changes.
- Use full file paths in responses; `~` is allowed for home.
- No emojis; use icon PNGs if needed.
- For reviews/debugging, inspect exact lines and cite line numbers.
- Require explicit user approval for high-risk operations (sudo, deletes, DB ops, network config, remote commands).
- Prefer privacy- and security-first choices; avoid leaking secrets.
- Use `uv` for Python; add `.mise.toml` with Python 3.13 when introducing per-project Python.
- When starting dev servers, use `nohup ... >server.log 2>&1 & echo $! > server.pid` and bind to 127.0.0.1/::1.
