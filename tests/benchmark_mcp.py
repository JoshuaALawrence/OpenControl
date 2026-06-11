from __future__ import annotations
import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "target" / "release" / "OpenControl.exe"


class Client:
    def __init__(self, exe: Path, env: dict[str, str] | None = None):
        self.process = subprocess.Popen(
            [str(exe)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
            env=env,
        )
        self.next_id = 0

    def send(self, payload: dict) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write((json.dumps(payload) + "\n").encode("utf-8"))
        self.process.stdin.flush()

    def read(self, request_id: int, timeout: float = 60.0) -> dict:
        assert self.process.stdout is not None
        deadline = time.perf_counter() + timeout
        while time.perf_counter() < deadline:
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("server closed stdout")
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if message.get("id") == request_id:
                return message
        raise TimeoutError(f"no response for id {request_id}")

    def request(self, method: str, params: dict | None = None, timeout: float = 60.0) -> dict:
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params or {}})
        response = self.read(self.next_id, timeout)
        if "error" in response:
            raise RuntimeError(f"{method} error: {response['error']}")
        return response["result"]

    def notify(self, method: str, params: dict | None = None) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def call_tool(self, name: str, args: dict | None = None, timeout: float = 60.0) -> dict:
        return self.request("tools/call", {"name": name, "arguments": args or {}}, timeout)

    def close(self) -> None:
        try:
            self.process.terminate()
            self.process.wait(timeout=2)
        except Exception:
            try:
                self.process.kill()
            except Exception:
                pass


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * pct)))
    return ordered[index]


def text_bytes(result: dict) -> int:
    total = 0
    for content in result.get("content", []):
        if content.get("type") == "text":
            total += len(content.get("text", "").encode("utf-8"))
        elif content.get("type") == "image":
            total += len(content.get("data", ""))
    return total


def measure(client: Client, name: str, args: dict, iterations: int, label: str | None = None) -> dict:
    durations: list[float] = []
    sizes: list[int] = []
    for _ in range(iterations):
        start = time.perf_counter()
        result = client.call_tool(name, args)
        durations.append((time.perf_counter() - start) * 1000.0)
        sizes.append(text_bytes(result))
    return {
        "tool": name,
        "case": label or name,
        "iterations": iterations,
        "median_ms": round(statistics.median(durations), 2),
        "mean_ms": round(statistics.mean(durations), 2),
        "p95_ms": round(percentile(durations, 0.95), 2),
        "min_ms": round(min(durations), 2),
        "max_ms": round(max(durations), 2),
        "median_payload_bytes": round(statistics.median(sizes)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark OpenControl MCP tool latency.")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--label", default="baseline")
    args = parser.parse_args()

    if not EXE.exists():
        print(f"missing release binary: {EXE}", file=sys.stderr)
        return 1

    client = Client(EXE, env=os.environ.copy())
    try:
        client.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "benchmark", "version": "0"},
            },
        )
        client.notify("notifications/initialized")
        active = client.call_tool("get_active_window")
        active_text = active["content"][0]["text"]
        active_window = json.loads(active_text)
        handle = active_window["id"]

        cases = [
            ("screen_info", {}, "screen_info"),
            ("take_screenshot", {"max_dimension": 1280}, "take_screenshot"),
            ("observe", {"window_handle": handle, "include_text": True, "marks": True}, "observe"),
            (
                "find_elements",
                {
                    "window_handle": handle,
                    "name": "",
                    "control_type": "",
                    "max_results": 30,
                },
                "find_elements_30",
            ),
            (
                "find_elements",
                {
                    "window_handle": handle,
                    "name": "",
                    "control_type": "Button",
                    "max_results": 30,
                },
                "find_buttons_30",
            ),
            (
                "wait_for_screen_change",
                {"timeout": 0.5, "poll_interval": 0.1, "threshold": 1.0},
                "wait_screen",
            ),
            (
                "wait_for_screen_change",
                {
                    "window_handle": handle,
                    "timeout": 0.5,
                    "poll_interval": 0.1,
                    "threshold": 1.0,
                    "scope": "window",
                },
                "wait_window",
            ),
        ]

        print(
            json.dumps(
                {
                    "label": args.label,
                }
            )
        )
        for tool, tool_args, case_label in cases:
            result = measure(client, tool, tool_args, max(1, args.iterations), case_label)
            print(json.dumps(result, sort_keys=True))
        return 0
    finally:
        client.close()


if __name__ == "__main__":
    raise SystemExit(main())