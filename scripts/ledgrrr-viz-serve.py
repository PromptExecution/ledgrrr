#!/usr/bin/env python3
"""ledgrrr-viz-serve — lightweight HTTP server for ledgrrr visualization dashboard.

Usage:
  python3 ledgrrr-viz-serve.py [--port 8080] [--cdp-port 19222]

Serves the Cytoscape.js dashboard on HTTP and exposes a minimal CDP-compatible
JSON endpoint at /json/version and /json/list for VizObserver compatibility.

Hermes management:
  python3 ledgrrr-viz-serve.py start   -- daemonize, write pidfile
  python3 ledgrrr-viz-serve.py stop    -- kill by pidfile
  python3 ledgrrr-viz-serve.py status  -- check if running
"""

import http.server
import json
import os
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

# ── paths ──────────────────────────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).parent.resolve()
VENDOR_DIR = SCRIPT_DIR.parent
CRATE_DIR = VENDOR_DIR / "crates" / "holon-viz"
TARGET_DIR = CRATE_DIR / "target"
DASHBOARD_HTML = TARGET_DIR / "ledgrrr-viz-dashboard.html"
PIDFILE = Path("/tmp/ledgrrr-viz-serve.pid")
DEFAULT_PORT = 8080
CDP_PORT = 19222

# ═══════════════════════════════════════════════════════════════════════════
# Minimal CDP-compatible endpoints
# ═══════════════════════════════════════════════════════════════════════════

CDP_VERSION_RESPONSE = {
    "Browser": "ledgrrr-viz-serve/0.8.0",
    "Protocol-Version": "1.3",
    "User-Agent": "ledgrrr-viz-serve",
    "V8-Version": "0.0.0",
    "WebKit-Version": "0.0.0",
    "webSocketDebuggerUrl": f"ws://localhost:{CDP_PORT}/devtools/page/ledgrrr-1",
}

CDP_LIST_RESPONSE = [
    {
        "id": "ledgrrr-1",
        "title": "ledgrrr Viz Dashboard",
        "url": f"http://localhost:{DEFAULT_PORT}/",
        "webSocketDebuggerUrl": f"ws://localhost:{CDP_PORT}/devtools/page/ledgrrr-1",
        "devtoolsFrontendUrl": f"devtools://devtools/bundled/js_app.html?ws=localhost:{CDP_PORT}/devtools/page/ledgrrr-1",
        "type": "page",
    }
]


class LedgrrrHTTPHandler(http.server.SimpleHTTPRequestHandler):
    """Serves the dashboard HTML and CDP JSON endpoints."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(DASHBOARD_HTML.parent), **kwargs)

    def do_GET(self):
        path = self.path.rstrip("/")

        # CDP-compatible endpoints (VizObserver integration)
        if path == "/json/version":
            self._json_response(CDP_VERSION_RESPONSE)
            return
        if path == "/json/list":
            self._json_response(CDP_LIST_RESPONSE)
            return
        if path == "/json":
            self._json_response(CDP_LIST_RESPONSE)
            return

        # Serve the dashboard HTML at /
        if path == "" or path == "/":
            self._serve_dashboard()
            return

        # Serve static files from the target directory
        super().do_GET()

    def _json_response(self, data):
        body = json.dumps(data, indent=2).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def _serve_dashboard(self):
        if DASHBOARD_HTML.exists():
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            with open(DASHBOARD_HTML, "rb") as f:
                self.wfile.write(f.read())
        else:
            self.send_response(404)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(
                b"Dashboard not found. Run: cd vendor/ledgrrr && "
                b"cargo run -p holon-viz --bin holon-viz-demo\n"
            )

    def log_message(self, format, *args):
        """Quiet logging — only log non-static requests."""
        if self.path not in ("/json/version", "/json/list", "/json"):
            super().log_message(format, *args)


# ═══════════════════════════════════════════════════════════════════════════
# Server management
# ═══════════════════════════════════════════════════════════════════════════


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("", 0))
        return s.getsockname()[1]


def cmd_start(port: int, cdp_port: int, foreground: bool):
    if PIDFILE.exists():
        try:
            pid = int(PIDFILE.read_text().strip())
            os.kill(pid, 0)
            print(f"ledgrrr-viz-serve already running (pid {pid}) on :{port}")
            return
        except (ProcessLookupError, ValueError):
            PIDFILE.unlink(missing_ok=True)

    # Find free ports if requested
    if port == 0:
        port = find_free_port()
    if cdp_port == 0:
        cdp_port = find_free_port()

    # Build the CDP-compatible response dynamically
    global CDP_VERSION_RESPONSE, CDP_LIST_RESPONSE
    CDP_VERSION_RESPONSE["webSocketDebuggerUrl"] = f"ws://localhost:{cdp_port}/devtools/page/ledgrrr-1"
    CDP_LIST_RESPONSE[0]["url"] = f"http://localhost:{port}/"
    CDP_LIST_RESPONSE[0]["webSocketDebuggerUrl"] = f"ws://localhost:{cdp_port}/devtools/page/ledgrrr-1"
    CDP_LIST_RESPONSE[0]["devtoolsFrontendUrl"] = (
        f"devtools://devtools/bundled/js_app.html?ws=localhost:{cdp_port}/devtools/page/ledgrrr-1"
    )

    if foreground:
        _run_server(port)
        return

    # Daemonize
    pid = os.fork()
    if pid > 0:
        # Parent
        PIDFILE.write_text(str(pid))
        print(f"ledgrrr-viz-serve started (pid {pid}) on http://localhost:{port}")
        print(f"  CDP endpoint: http://localhost:{port}/json/version")
        print(f"  Dashboard:    http://localhost:{port}/")
        return

    # Child (daemon)
    os.setsid()
    _run_server(port)


def _run_server(port: int):
    server = http.server.HTTPServer(("0.0.0.0", port), LedgrrrHTTPHandler)
    print(f"Serving ledgrrr viz dashboard on http://localhost:{port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.shutdown()


def cmd_stop():
    if not PIDFILE.exists():
        print("ledgrrr-viz-serve is not running")
        return
    try:
        pid = int(PIDFILE.read_text().strip())
        os.kill(pid, signal.SIGTERM)
        time.sleep(0.5)
        # Force kill if still alive
        try:
            os.kill(pid, 0)
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        PIDFILE.unlink(missing_ok=True)
        print(f"ledgrrr-viz-serve (pid {pid}) stopped")
    except (ProcessLookupError, ValueError):
        PIDFILE.unlink(missing_ok=True)
        print("ledgrrr-viz-serve was not running (stale pidfile cleaned)")


def cmd_status():
    if not PIDFILE.exists():
        print("ledgrrr-viz-serve: STOPPED")
        return
    try:
        pid = int(PIDFILE.read_text().strip())
        os.kill(pid, 0)
        print(f"ledgrrr-viz-serve: RUNNING (pid {pid})")
    except (ProcessLookupError, ValueError):
        PIDFILE.unlink(missing_ok=True)
        print("ledgrrr-viz-serve: STOPPED (stale pidfile cleaned)")


def cmd_regenerate():
    """Regenerate the dashboard HTML from the holon-viz crate."""
    vendor_dir = Path(__file__).parent
    print("Rebuilding dashboard...")
    result = subprocess.run(
        ["cargo", "run", "-p", "holon-viz", "--bin", "holon-viz-demo"],
        cwd=vendor_dir,
        capture_output=True,
        text=True,
        timeout=300,
    )
    if result.returncode == 0:
        print("Dashboard regenerated.")
    else:
        print(f"Build failed:\n{result.stderr}")


# ═══════════════════════════════════════════════════════════════════════════
# CLI
# ═══════════════════════════════════════════════════════════════════════════

def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return

    cmd = sys.argv[1]
    port = DEFAULT_PORT
    cdp_port_val = CDP_PORT
    foreground = False

    for arg in sys.argv[2:]:
        if arg == "--foreground":
            foreground = True
        elif arg.startswith("--port="):
            port = int(arg.split("=", 1)[1])
        elif arg.startswith("--cdp-port="):
            cdp_port_val = int(arg.split("=", 1)[1])

    if cmd == "start":
        cmd_start(port, cdp_port_val, foreground)
    elif cmd == "stop":
        cmd_stop()
    elif cmd == "status":
        cmd_status()
    elif cmd == "regenerate":
        cmd_regenerate()
    elif cmd == "--help" or cmd == "-h":
        print(__doc__)
    else:
        print(f"Unknown command: {cmd}")
        sys.exit(1)


if __name__ == "__main__":
    main()
