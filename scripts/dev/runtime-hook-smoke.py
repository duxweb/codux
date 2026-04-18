#!/usr/bin/env python3
"""
Developer-only smoke runner for dmux AI wrappers.

This script exercises the real codex/claude binaries through the bundled dmux
wrapper scripts and captures the hook/socket events they emit.

Examples:
  python3 scripts/dev/runtime-hook-smoke.py --tool claude
  python3 scripts/dev/runtime-hook-smoke.py --tool codex --model gpt-5-mini
  python3 scripts/dev/runtime-hook-smoke.py --tool codex --mode interrupt
"""

from __future__ import annotations

import argparse
import json
import os
import pty
import select
import socket
import subprocess
import sys
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
    env["DMUX_PROJECT_NAME"] = "runtime-hook-smoke"
    env["DMUX_PROJECT_PATH"] = str(ROOT)
    env["DMUX_SESSION_TITLE"] = "runtime-hook-smoke"
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


def noninteractive_command(tool: str, model: str, prompt: str) -> list[str]:
    wrapper = str(WRAPPER_BIN / tool)
    if tool == "claude":
        return [wrapper, "--model", model, "--print", prompt]
    if tool == "codex":
        return [wrapper, "exec", "--model", model, prompt]
    raise ValueError(f"unsupported tool: {tool}")


def interactive_command(tool: str, model: str, prompt: str) -> list[str]:
    wrapper = str(WRAPPER_BIN / tool)
    return [wrapper, "--model", model, prompt]


def run_noninteractive(tool: str, model: str, prompt: str, env: dict[str, str]) -> tuple[int, str, str]:
    proc = subprocess.run(
        noninteractive_command(tool, model, prompt),
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=90,
        check=False,
    )
    return proc.returncode, proc.stdout, proc.stderr


def run_interrupt(tool: str, model: str, prompt: str, env: dict[str, str], interrupt_after: float) -> tuple[int, str]:
    master_fd, slave_fd = pty.openpty()
    try:
        proc = subprocess.Popen(
            interactive_command(tool, model, prompt),
            cwd=ROOT,
            env=env,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            text=False,
            close_fds=True,
        )
        os.close(slave_fd)
        deadline = time.time() + interrupt_after
        output = bytearray()
        while time.time() < deadline and proc.poll() is None:
            ready, _, _ = select.select([master_fd], [], [], 0.1)
            if ready:
                try:
                    output.extend(os.read(master_fd, 8192))
                except OSError:
                    break
        if proc.poll() is None:
            os.write(master_fd, b"\x03")
        end = time.time() + 10
        while time.time() < end and proc.poll() is None:
            ready, _, _ = select.select([master_fd], [], [], 0.1)
            if ready:
                try:
                    output.extend(os.read(master_fd, 8192))
                except OSError:
                    break
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)
        return proc.returncode or 0, output.decode("utf-8", "replace")
    finally:
        try:
            os.close(master_fd)
        except OSError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tool", choices=["claude", "codex"], required=True)
    parser.add_argument("--mode", choices=["noninteractive", "interrupt"], default="noninteractive")
    parser.add_argument("--model", default=None)
    parser.add_argument("--prompt", default="Reply with exactly OK.")
    parser.add_argument("--interrupt-after", type=float, default=2.0)
    args = parser.parse_args()

    model = args.model or DEFAULT_MODELS[args.tool]

    with tempfile.TemporaryDirectory(prefix=f"dmux-{args.tool}-smoke-") as td:
        tmpdir = Path(td)
        socket_path = tmpdir / "runtime.sock"
        server = RuntimeSocketServer(socket_path)
        server.start()
        try:
            env = build_env(args.tool, socket_path, tmpdir)
            if args.mode == "noninteractive":
                code, stdout, stderr = run_noninteractive(args.tool, model, args.prompt, env)
            else:
                code, stdout = run_interrupt(args.tool, model, args.prompt, env, args.interrupt_after)
                stderr = ""

            time.sleep(0.5)

            print(f"tool={args.tool}")
            print(f"mode={args.mode}")
            print(f"model={model}")
            print(f"exit_code={code}")
            if stdout.strip():
                print("--- stdout ---")
                print(stdout[-4000:])
            if stderr.strip():
                print("--- stderr ---")
                print(stderr[-4000:])
            print("--- events ---")
            for event in server.events:
                print(json.dumps(event, ensure_ascii=False))

            return 0 if server.events else 1
        finally:
            server.stop()


if __name__ == "__main__":
    raise SystemExit(main())
