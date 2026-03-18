#!/usr/bin/env python3
"""
DixScript CLI Test Server
Serves the CLI test page and proxies real `mdix` binary calls.

Usage:
    python3 tools/cli_test_server.py

Then open: http://localhost:7373

The server auto-detects the mdix binary in this order:
  1. target/debug/mdix   (or .exe on Windows)
  2. target/release/mdix
  3. mdix on PATH

Run from the workspace root:
    cd /path/to/DixScript-Rust
    python3 tools/cli_test_server.py
"""

import http.server
import json
import os
import platform
import shutil
import subprocess
import sys
import urllib.parse
from pathlib import Path

PORT       = 7373
HTML_FILE  = Path(__file__).parent.parent / ".github" / "dixscript-cli-test.html"
REPO_ROOT  = Path(__file__).parent.parent.resolve()
IS_WINDOWS = platform.system() == "Windows"
EXE_SUFFIX = ".exe" if IS_WINDOWS else ""

# ── Binary discovery ──────────────────────────────────────────────────────────

def find_binary() -> str | None:
    candidates = [
        REPO_ROOT / "target" / "debug"   / f"mdix{EXE_SUFFIX}",
        REPO_ROOT / "target" / "release" / f"mdix{EXE_SUFFIX}",
    ]
    for c in candidates:
        if c.exists():
            return str(c)
    # fall back to PATH
    found = shutil.which("mdix")
    if found:
        return found
    return None


BINARY = find_binary()

if BINARY:
    print(f"[dix-test] binary: {BINARY}")
else:
    print("[dix-test] WARNING: mdix binary not found.")
    print("           Run `cargo build -p mdix-cli` first.")
    print("           Commands will return an error until the binary is built.")

# ── Request handler ───────────────────────────────────────────────────────────

class Handler(http.server.BaseHTTPRequestHandler):

    # Silence the per-request access log — the terminal gets noisy otherwise.
    def log_message(self, fmt, *args):  # noqa: ANN001
        pass

    def log_error(self, fmt, *args):    # noqa: ANN001
        print(f"[dix-test] ERROR: {fmt % args}", file=sys.stderr)

    # ── CORS ──────────────────────────────────────────────────────────────────

    def _cors(self):
        self.send_header("Access-Control-Allow-Origin",  "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors()
        self.end_headers()

    # ── GET ───────────────────────────────────────────────────────────────────

    def do_GET(self):
        path = urllib.parse.urlparse(self.path).path

        if path in ("/", "/index.html"):
            self._serve_html()
        elif path == "/ping":
            self._json({"ok": True, "binary": BINARY})
        elif path.startswith("/file/"):
            self._serve_mdix(path[6:])   # strip "/file/"
        else:
            self.send_error(404, "Not found")

    def _serve_html(self):
        if not HTML_FILE.exists():
            self.send_error(404, f"HTML not found: {HTML_FILE}")
            return
        data = HTML_FILE.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self._cors()
        self.end_headers()
        self.wfile.write(data)

    def _serve_mdix(self, rel: str):
        """Serve a .mdix file from the repo root so the page can display it."""
        target = (REPO_ROOT / rel).resolve()
        # Safety: must stay inside repo root
        try:
            target.relative_to(REPO_ROOT)
        except ValueError:
            self.send_error(403, "Forbidden")
            return
        if not target.exists() or not target.is_file():
            self.send_error(404, f"File not found: {rel}")
            return
        data = target.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self._cors()
        self.end_headers()
        self.wfile.write(data)

    # ── POST /run ─────────────────────────────────────────────────────────────

    def do_POST(self):
        path = urllib.parse.urlparse(self.path).path

        if path == "/run":
            self._run_command()
        else:
            self.send_error(404, "Not found")

    def _run_command(self):
        length = int(self.headers.get("Content-Length", 0))
        body   = self.rfile.read(length)

        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            self._json({"error": "invalid JSON"}, status=400)
            return

        cmd_args = payload.get("args", [])   # list of string tokens
        if not cmd_args:
            self._json({"error": "args must be a non-empty list"}, status=400)
            return

        if not BINARY:
            self._json({
                "stdout": "",
                "stderr": "mdix binary not found. Run: cargo build -p mdix-cli",
                "exit_code": 127,
            })
            return

        full_cmd = [BINARY] + cmd_args
        print(f"[dix-test] $ {' '.join(full_cmd)}")

        try:
            proc = subprocess.run(
                full_cmd,
                cwd=str(REPO_ROOT),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self._json({
                "stdout":    proc.stdout,
                "stderr":    proc.stderr,
                "exit_code": proc.returncode,
            })
        except subprocess.TimeoutExpired:
            self._json({
                "stdout":    "",
                "stderr":    "Command timed out after 30 seconds.",
                "exit_code": -1,
            })
        except Exception as exc:  # noqa: BLE001
            self._json({
                "stdout":    "",
                "stderr":    f"Failed to execute binary: {exc}",
                "exit_code": -1,
            })

    # ── Helpers ───────────────────────────────────────────────────────────────

    def _json(self, obj: dict, status: int = 200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self._cors()
        self.end_headers()
        self.wfile.write(data)


# ── Entry point ───────────────────────────────────────────────────────────────

def main():
    addr = ("127.0.0.1", PORT)
    print(f"[dix-test] server starting on http://localhost:{PORT}")
    print(f"[dix-test] repo root: {REPO_ROOT}")
    print(f"[dix-test] HTML file: {HTML_FILE}")
    print(f"[dix-test] press Ctrl+C to stop")
    print()

    server = http.server.HTTPServer(addr, Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n[dix-test] stopped.")


if __name__ == "__main__":
    main()
