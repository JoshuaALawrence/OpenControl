from __future__ import annotations
import base64
import json
import os
import subprocess
import sys
import time
from pathlib import Path

# Live screen text (window titles, a11y trees, OCR) can contain characters the
# console's legacy codepage can't encode (e.g. \ufffc on a Windows cp1252
# terminal). Force UTF-8 with replacement so printing results never crashes.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

EXE = Path(__file__).resolve().parents[1] / "target" / "release" / "OpenControl.exe"


class Client:
    def __init__(self, exe: Path, env: dict | None = None):
        proc_env = None
        if env:
            proc_env = os.environ.copy()
            proc_env.update(env)
        self.p = subprocess.Popen(
            [str(exe)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, bufsize=0, env=proc_env,
        )
        self._id = 0

    def _send(self, obj: dict):
        self.p.stdin.write((json.dumps(obj) + "\n").encode("utf-8"))
        self.p.stdin.flush()

    def _read(self, want_id: int, timeout: float = 30.0):
        end = time.time() + timeout
        while time.time() < end:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("server closed stdout")
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == want_id:
                return msg
        raise TimeoutError(f"no response for id {want_id}")

    def request(self, method: str, params: dict | None = None, timeout: float = 30.0):
        self._id += 1
        self._send({"jsonrpc": "2.0", "id": self._id, "method": method, "params": params or {}})
        msg = self._read(self._id, timeout)
        if "error" in msg:
            raise RuntimeError(f"{method} error: {msg['error']}")
        return msg["result"]

    def notify(self, method: str, params: dict | None = None):
        self._send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def call_tool(self, name: str, args: dict | None = None, timeout: float = 40.0):
        return self.request("tools/call", {"name": name, "arguments": args or {}}, timeout)

    def expect_error(self, name: str, args: dict | None = None, timeout: float = 40.0) -> str:
        """Call a tool expecting it to fail; return the error message."""
        try:
            self.call_tool(name, args, timeout)
        except RuntimeError as e:
            return str(e)
        raise AssertionError(f"{name} unexpectedly succeeded (expected an error)")

    def initialize(self, name: str = "smoke"):
        init = self.request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": name, "version": "0"},
        })
        self.notify("notifications/initialized")
        return init

    def close(self):
        try:
            self.p.terminate()
        except Exception:
            pass


def summarize(result: dict) -> str:
    parts = []
    for c in result.get("content", []):
        if c.get("type") == "text":
            t = c["text"]
            parts.append(f"text[{len(t)}b]: " + t[:90].replace("\n", " "))
        elif c.get("type") == "image":
            data = c.get("data", "")
            try:
                n = len(base64.b64decode(data))
            except Exception:
                n = len(data)
            parts.append(f"image[{c.get('mimeType')}, {n}B]")
    return " | ".join(parts)


def text_content(result: dict) -> str:
    """Concatenate the text blocks of a tool result."""
    return "\n".join(c["text"] for c in result.get("content", []) if c.get("type") == "text")


def text_json(result: dict):
    """Parse the text content of a tool result as JSON (or None)."""
    try:
        return json.loads(text_content(result))
    except (json.JSONDecodeError, TypeError):
        return None


def has_image(result: dict) -> bool:
    return any(c.get("type") == "image" for c in result.get("content", []))


def find_app_window(list_windows: dict, needle: str):
    """Return (id, app, title) for the first window whose exe ends with `needle`."""
    data = text_json(list_windows) or {}
    for w in data.get("windows", []):
        app = (w.get("app") or "").lower()
        if app.endswith(needle.lower()):
            return w.get("id"), w.get("app"), w.get("title")
    return None


def blocklist_section(c: Client) -> None:
    """Exercise the application blocklist end-to-end against a real Notepad.

    Skips (does not fail) if Notepad never appears, e.g. on a headless desktop.
    """
    print("\n--- blocklist / redaction ---")
    try:
        c.call_tool("launch_app", {"app": "notepad"})
    except RuntimeError as e:
        print(f"[SKIP] could not launch notepad: {e}")
        return

    target = None
    for _ in range(15):
        time.sleep(0.3)
        hit = find_app_window(c.call_tool("list_windows"), "notepad.exe")
        if hit:
            target = hit
            break
    if not target:
        print("[SKIP] no Notepad window appeared (headless desktop?)")
        return

    handle, app, title = target
    print(f"[OK] launched Notepad: handle={handle} app={app} title={title!r}")

    blocked = Client(EXE, env={"OPENCONTROL_BLOCK_EXE": "notepad.exe"})
    try:
        blocked.initialize("smoke-blocked")

        bl = text_json(blocked.call_tool("get_blocklist")) or {}
        assert bl.get("active") is True, "blocklist should be active"
        assert bl.get("rule_count", 0) >= 1, "expected >=1 rule"
        print(f"[OK] get_blocklist:   active={bl.get('active')} rules={bl.get('rule_count')}")

        lw = blocked.call_tool("list_windows")
        assert find_app_window(lw, "notepad.exe") is None, "blocked Notepad must be hidden"
        print("[OK] list_windows:    Notepad correctly hidden")

        err = blocked.expect_error("focus_window", {"window_handle": handle})
        assert "block" in err.lower(), f"focus error should mention blocklist: {err}"
        print("[OK] focus_window:    refused (blocked)")

        blocked.expect_error("observe", {"window_handle": handle})
        print("[OK] observe:         refused (blocked)")

        ss = blocked.call_tool("take_screenshot", {"max_dimension": 800})
        assert has_image(ss), "screenshot should still return an image"
        print("[OK] take_screenshot: returned image (Notepad redacted)")
    finally:
        blocked.close()
        # Close Notepad via the unblocked client.
        try:
            c.call_tool("close_window", {"window_handle": handle})
        except RuntimeError:
            pass


def main() -> int:
    if not EXE.exists():
        print(f"server not built: {EXE}")
        return 1
    c = Client(EXE)
    try:
        init = c.request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "smoke", "version": "0"},
        })
        si = init.get("serverInfo", {})
        print(f"[OK] initialize -> {si.get('name')} v{si.get('version')} (proto {init.get('protocolVersion')})")
        c.notify("notifications/initialized")

        tools = c.request("tools/list").get("tools", [])
        print(f"[OK] tools/list -> {len(tools)} tools")
        names = sorted(t["name"] for t in tools)
        print("     " + ", ".join(names))
        assert "get_blocklist" in names, "get_blocklist tool missing"

        print("[OK] screen_info:     " + summarize(c.call_tool("screen_info")))
        print("[OK] get_system_info: " + summarize(c.call_tool("get_system_info")))
        wins = c.call_tool("list_windows")
        print("[OK] list_windows:    " + summarize(wins))
        print("[OK] take_screenshot: " + summarize(c.call_tool("take_screenshot", {"max_dimension": 1280})))
        print("[OK] get_active_window: " + summarize(c.call_tool("get_active_window")))
        print("[OK] observe(fg):     " + summarize(c.call_tool("observe", {"include_text": True, "marks": True})))
        print("[OK] ocr(region):     " + summarize(c.call_tool("ocr", {"region": [0, 0, 600, 200]})))

        blocklist_section(c)

        print("\nALL RUST MCP SERVER TESTS PASSED")
        return 0
    finally:
        c.close()


if __name__ == "__main__":
    sys.exit(main())
