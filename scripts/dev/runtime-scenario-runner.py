#!/usr/bin/env python3
"""
Developer-only multi-step runtime scenario runner.

This script drives the real CLI wrappers in PTYs and records the hook/socket
events they emit so we can reproduce loading / completion / interrupt flows.
"""

from __future__ import annotations

import argparse
import json
import os
import pty
import select
import signal
import socket
import subprocess
import tempfile
import threading
import time
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WRAPPER_BIN = ROOT / "scripts" / "wrappers" / "bin"

DEFAULT_MODELS = {
    "claude": "claude-haiku-4-5",
    "codex": "gpt-5.1-codex-mini",
}


class RuntimeSocketServer:
    def __init__(self, socket_path: Path) -> None:
        self.socket_path = socket_path
        self.events: list[dict] = []
        self._stop = threading.Event()
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self) -> None:
        self._thread.start()
        if not self._ready.wait(timeout=2):
            raise RuntimeError("runtime socket server did not become ready")

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=2)
        try:
            self.socket_path.unlink()
        except FileNotFoundError:
            pass

    def snapshot(self) -> list[dict]:
        return list(self.events)

    def _run(self) -> None:
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            try:
                self.socket_path.unlink()
            except FileNotFoundError:
                pass
            server.bind(str(self.socket_path))
            server.listen(16)
            server.settimeout(0.2)
            self._ready.set()
            while not self._stop.is_set():
                try:
                    conn, _ = server.accept()
                except socket.timeout:
                    continue
                with conn:
                    payload = bytearray()
                    while True:
                        chunk = conn.recv(4096)
                        if not chunk:
                            break
                        payload.extend(chunk)
                    if not payload:
                        continue
                    try:
                        self.events.append(json.loads(payload.decode("utf-8")))
                    except Exception:
                        self.events.append({"decode_error": payload.decode("utf-8", "replace")})
        finally:
            server.close()


class InteractiveRunner:
    def __init__(self, tool: str, model: str, env: dict[str, str]) -> None:
        self.tool = tool
        self.model = model
        self.env = env
        self.master_fd, slave_fd = pty.openpty()
        cmd = [str(WRAPPER_BIN / tool), "--model", model]
        self.proc = subprocess.Popen(
            cmd,
            cwd=ROOT,
            env=env,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            text=False,
            close_fds=True,
        )
        os.close(slave_fd)
        self.output = bytearray()

    def close(self) -> None:
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=5)
        try:
            os.close(self.master_fd)
        except OSError:
            pass

    def read_available(self, duration: float = 0.5) -> str:
        end = time.time() + duration
        while time.time() < end and self.proc.poll() is None:
            ready, _, _ = select.select([self.master_fd], [], [], 0.1)
            if not ready:
                continue
            try:
                self.output.extend(os.read(self.master_fd, 8192))
            except OSError:
                break
        return self.output.decode("utf-8", "replace")

    def send_prompt(self, text: str) -> str:
        os.write(self.master_fd, text.encode("utf-8") + b"\n")
        return self.read_available(1.0)

    def interrupt(self) -> str:
        os.write(self.master_fd, b"\x03")
        return self.read_available(1.0)

    def wait_for_idle(self, timeout: float = 30.0) -> str:
        end = time.time() + timeout
        while time.time() < end and self.proc.poll() is None:
            self.read_available(0.5)
        return self.output.decode("utf-8", "replace")


def build_env(tool: str, socket_path: Path, tmpdir: Path) -> dict[str, str]:
    env = os.environ.copy()
    original_path = env.get("PATH", "")
    env["PATH"] = f"{WRAPPER_BIN}:{original_path}"
    env["DMUX_WRAPPER_BIN"] = str(WRAPPER_BIN)
    env["DMUX_ORIGINAL_PATH"] = original_path
    env["DMUX_RUNTIME_SOCKET"] = str(socket_path)
    env["DMUX_SESSION_ID"] = str(uuid.uuid4()).upper()
    env["DMUX_SESSION_INSTANCE_ID"] = str(uuid.uuid4()).lower()
    env["DMUX_PROJECT_ID"] = str(uuid.uuid4()).upper()
    env["DMUX_PROJECT_NAME"] = "runtime-scenario-runner"
    env["DMUX_PROJECT_PATH"] = str(ROOT)
    env["DMUX_SESSION_TITLE"] = "runtime-scenario-runner"
    env["DMUX_SESSION_CWD"] = str(ROOT)
    env["DMUX_STATUS_DIR"] = str(tmpdir / "status")
    env["DMUX_CLAUDE_SESSION_MAP_DIR"] = str(tmpdir / "claude-session-map")
    env["DMUX_LOG_FILE"] = str(tmpdir / f"{tool}.log")
    for key in [
        "DMUX_ACTIVE_AI_TOOL",
        "DMUX_ACTIVE_AI_STARTED_AT",
        "DMUX_ACTIVE_AI_INVOCATION_ID",
        "DMUX_ACTIVE_AI_RESOLVED_PATH",
        "DMUX_EXTERNAL_SESSION_ID",
    ]:
        env.pop(key, None)
    os.makedirs(env["DMUX_STATUS_DIR"], exist_ok=True)
    os.makedirs(env["DMUX_CLAUDE_SESSION_MAP_DIR"], exist_ok=True)
    return env


def summarize_events(events: list[dict]) -> list[str]:
    lines: list[str] = []
    for event in events:
        kind = event.get("kind")
        payload = event.get("payload", {})
        if kind == "response":
            lines.append(f"response state={payload.get('responseState')} updatedAt={payload.get('updatedAt')}")
        elif kind in {"claude-hook", "codex-hook"}:
            lines.append(f"{kind} event={payload.get('event')}")
        elif kind == "usage":
            lines.append(f"usage status={payload.get('status')} response={payload.get('responseState')}")
        else:
            lines.append(json.dumps(event, ensure_ascii=False))
    return lines


def scenario_interrupt(tool: str, model: str) -> int:
    with tempfile.TemporaryDirectory(prefix=f"dmux-{tool}-scenario-") as td:
        tmpdir = Path(td)
        server = RuntimeSocketServer(tmpdir / "runtime.sock")
        server.start()
        try:
            runner = InteractiveRunner(tool, model, build_env(tool, server.socket_path, tmpdir))
            try:
                runner.read_available(1.0)
                runner.send_prompt("你好")
                time.sleep(0.5)
                runner.interrupt()
                time.sleep(1.0)
                output = runner.read_available(1.0)
            finally:
                runner.close()

            print(f"tool={tool}")
            print(f"model={model}")
            print("--- output ---")
            print(output[-4000:])
            print("--- event summary ---")
            for line in summarize_events(server.snapshot()):
                print(line)
            return 0
        finally:
            server.stop()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tool", choices=["claude", "codex"], required=True)
    parser.add_argument("--scenario", choices=["interrupt"], default="interrupt")
    parser.add_argument("--model", default=None)
    args = parser.parse_args()

    model = args.model or DEFAULT_MODELS[args.tool]
    if args.scenario == "interrupt":
        return scenario_interrupt(args.tool, model)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
