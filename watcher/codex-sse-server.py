#!/usr/bin/env python3
"""
Codex Traffic Watcher - mitmproxy addon + SSE server

Usage:
    mitmdump -s codex-sse-server.py -p 8080

Then open codex-watcher.html in a browser.
The watcher connects to http://localhost:9119/stream via EventSource.

Requires: mitmproxy (pip install mitmproxy)
"""

import json
import queue
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

from mitmproxy import http

SSE_PORT = 9119
MAX_BODY_BYTES = 65_536  # 64KB cap before truncation

# --- SSE subscriber registry ---
_subscribers: list[queue.Queue] = []
_lock = threading.Lock()


def _broadcast(event_type: str, payload: dict) -> None:
    msg = f"event: {event_type}\ndata: {json.dumps(payload)}\n\n".encode()
    with _lock:
        dead = []
        for q in _subscribers:
            try:
                q.put_nowait(msg)
            except queue.Full:
                dead.append(q)
        for q in dead:
            _subscribers.remove(q)


# --- SSE HTTP server ---

class _SSEHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/stream":
            self._stream()
        elif self.path in ("/", "/health"):
            self._health()
        else:
            self.send_error(404)

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors()
        self.end_headers()

    def _cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def _health(self):
        body = b'{"status":"ok"}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self._cors()
        self.end_headers()
        self.wfile.write(body)

    def _stream(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Accel-Buffering", "no")
        self._cors()
        self.end_headers()

        q: queue.Queue = queue.Queue(maxsize=256)
        with _lock:
            _subscribers.append(q)

        try:
            while True:
                try:
                    msg = q.get(timeout=25)
                    self.wfile.write(msg)
                    self.wfile.flush()
                except queue.Empty:
                    # heartbeat keeps connection alive through proxies
                    self.wfile.write(b": ping\n\n")
                    self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            with _lock:
                try:
                    _subscribers.remove(q)
                except ValueError:
                    pass

    def log_message(self, fmt, *args):  # silence access logs
        pass


def _start_sse_server():
    server = HTTPServer(("127.0.0.1", SSE_PORT), _SSEHandler)
    server.serve_forever()


# --- Helpers ---

def _session_id_from_request(flow: http.HTTPFlow) -> str:
    hdrs = flow.request.headers
    return (
        hdrs.get("x-session-id")
        or hdrs.get("x-oai-request-id")
        or flow.client_conn.id[:12]
    )


def _session_id_from_response(flow: http.HTTPFlow) -> str:
    rhdrs = flow.response.headers if flow.response else {}
    return (
        rhdrs.get("x-request-id")
        or rhdrs.get("x-oai-request-id")
        or _session_id_from_request(flow)
    )


def _parse_body(raw: bytes, content_type: str) -> tuple[object, int]:
    """Return (parsed_body, token_estimate)."""
    if not raw:
        return None, 0

    # Streaming SSE from OpenAI: text/event-stream
    if "event-stream" in content_type:
        lines = raw.decode("utf-8", errors="replace").splitlines()
        data_lines = [l[6:] for l in lines if l.startswith("data: ") and l != "data: [DONE]"]
        parsed = []
        total_tokens = 0
        for dl in data_lines[:100]:  # cap at 100 chunks
            try:
                obj = json.loads(dl)
                parsed.append(obj)
                if isinstance(obj, dict):
                    usage = obj.get("usage") or {}
                    total_tokens = max(total_tokens, usage.get("total_tokens", 0))
            except json.JSONDecodeError:
                parsed.append(dl)
        estimate = total_tokens or (len(raw) // 4)
        return parsed, estimate

    # Regular JSON body
    truncated = raw[:MAX_BODY_BYTES]
    try:
        obj = json.loads(truncated)
        usage = {}
        if isinstance(obj, dict):
            usage = obj.get("usage") or {}
        total_tokens = usage.get("total_tokens", 0)
        estimate = total_tokens or (len(raw) // 4)
        return obj, estimate
    except json.JSONDecodeError:
        text = truncated.decode("utf-8", errors="replace")
        return text, len(raw) // 4


def _is_openai(flow: http.HTTPFlow) -> bool:
    host = flow.request.pretty_host()
    return "openai.com" in host or "api.openai" in host


# --- mitmproxy addon ---

class CodexWatcher:
    def load(self, loader):
        # Start SSE server when addon loads
        t = threading.Thread(target=_start_sse_server, daemon=True)
        t.start()
        print(f"[codex-watcher] SSE server listening on http://127.0.0.1:{SSE_PORT}/stream")

    def request(self, flow: http.HTTPFlow) -> None:
        if not _is_openai(flow):
            return

        body, tokens = _parse_body(
            flow.request.content,
            flow.request.headers.get("content-type", ""),
        )

        _broadcast("flow", {
            "session_id": _session_id_from_request(flow),
            "direction": "request",
            "method": flow.request.method,
            "url": flow.request.pretty_url(),
            "status": None,
            "body": body,
            "tokens": tokens,
            "ts": time.time(),
        })

    def response(self, flow: http.HTTPFlow) -> None:
        if not _is_openai(flow):
            return

        body, tokens = _parse_body(
            flow.response.content if flow.response else b"",
            flow.response.headers.get("content-type", "") if flow.response else "",
        )

        _broadcast("flow", {
            "session_id": _session_id_from_response(flow),
            "direction": "response",
            "method": flow.request.method,
            "url": flow.request.pretty_url(),
            "status": flow.response.status_code if flow.response else None,
            "body": body,
            "tokens": tokens,
            "ts": time.time(),
        })


addons = [CodexWatcher()]
