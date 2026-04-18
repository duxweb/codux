#!/usr/bin/env python3
"""
Developer-only runtime scenario runner for real codex/claude wrappers.

Scenarios are synchronized on dmux hook/runtime events instead of fixed sleeps.
This keeps the script useful for reproducing loading / completion / interrupt /
resume regressions with low-cost models.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import pty
import select
import socket
import subprocess
import tempfile
import threading
import time
import uuid
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[2]
WRAPPER_BIN = ROOT / "scripts" / "wrappers" / "bin"

DEFAULT_MODELS = {
    "claude": "claude-haiku-4-5",
    "codex": "gpt-5.1-codex-mini",
}


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


class RuntimeSocketServer:
    def __init__(self, socket_path: Path) -> None:
        self.socket_path = socket_path
        self.events: list[dict] = []
        self._lock = threading.Lock()
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
        with self._lock:
            return list(self.events)

    def event_count(self) -> int:
        with self._lock:
            return len(self.events)

    def append(self, event: dict) -> None:
        with self._lock:
            self.events.append(event)

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
                        self.append(json.loads(payload.decode("utf-8")))
                    except Exception:
                        self.append({"decode_error": payload.decode("utf-8", "replace")})
        finally:
            server.close()


@dataclasses.dataclass
class StepResult:
    name: str
    status: str
    detail: str
    event_count: int


def extract_event_name(event: dict) -> str | None:
    payload = event.get("payload") or {}
    return payload.get("event")


def extract_response_state(event: dict) -> str | None:
    payload = event.get("payload") or {}
    return payload.get("responseState")


def extract_external_session_id(event: dict) -> str | None:
    payload = event.get("payload") or {}
    if isinstance(payload.get("externalSessionID"), str):
        return payload["externalSessionID"]
    raw_payload = payload.get("payload")
    if isinstance(raw_payload, str) and raw_payload:
        try:
            obj = json.loads(raw_payload)
        except Exception:
            return None
        for key in ("session_id", "sessionId", "externalSessionID"):
            value = obj.get(key)
            if isinstance(value, str) and value:
                return value
    return None


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


class InteractiveRunner:
    def __init__(self, tool: str, model: str, env: dict[str, str], args: list[str] | None = None) -> None:
        self.tool = tool
        self.model = model
        self.env = env
        self.master_fd, slave_fd = pty.openpty()
        cmd = [str(WRAPPER_BIN / tool)]
        if tool == "codex":
            cmd.extend(["--model", model])
        else:
            cmd.extend(["--model", model])
        if args:
            cmd.extend(args)
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

    def read_available(self, duration: float = 0.25) -> str:
        end = time.time() + duration
        while time.time() < end and self.proc.poll() is None:
            ready, _, _ = select.select([self.master_fd], [], [], 0.05)
            if not ready:
                continue
            try:
                self.output.extend(os.read(self.master_fd, 8192))
            except OSError:
                break
        return self.output.decode("utf-8", "replace")

    def send_prompt(self, text: str) -> None:
        os.write(self.master_fd, text.encode("utf-8") + b"\r")

    def interrupt(self) -> None:
        os.write(self.master_fd, b"\x03")

    def output_text(self) -> str:
        return self.output.decode("utf-8", "replace")


class ScenarioContext:
    def __init__(self, tool: str, model: str) -> None:
        self.tool = tool
        self.model = model
        self.temp_dir_obj = tempfile.TemporaryDirectory(prefix=f"dmux-{tool}-scenario-")
        self.tmpdir = Path(self.temp_dir_obj.name)
        self.server = RuntimeSocketServer(self.tmpdir / "runtime.sock")
        self.server.start()
        self._runner: InteractiveRunner | None = None
        self.external_session_id: str | None = None
        self.steps: list[StepResult] = []

    def close(self) -> None:
        if self._runner is not None:
            self._runner.close()
            self._runner = None
        self.server.stop()
        self.temp_dir_obj.cleanup()

    def start(self, args: list[str] | None = None) -> InteractiveRunner:
        env = build_env(self.tool, self.server.socket_path, self.tmpdir)
        runner = InteractiveRunner(self.tool, self.model, env, args=args)
        self._runner = runner
        runner.read_available(1.0)
        return runner

    @property
    def runner(self) -> InteractiveRunner:
        if self._runner is None:
            raise RuntimeError("scenario runner has not started a process")
        return self._runner

    def wait_for(self, predicate: Callable[[dict], bool], timeout: float = 30.0, include_existing: bool = False) -> dict:
        start_index = 0 if include_existing else self.server.event_count()
        deadline = time.time() + timeout
        while time.time() < deadline:
            self.runner.read_available(0.1)
            events = self.server.snapshot()
            for event in events[start_index:]:
                ext = extract_external_session_id(event)
                if ext and not self.external_session_id:
                    self.external_session_id = ext
                if predicate(event):
                    return event
            time.sleep(0.05)
        event_summary = "\n".join(summarize_events(self.server.snapshot())[-20:])
        output_tail = self.runner.output_text()[-2000:]
        raise TimeoutError(
            "timed out waiting for runtime event\n"
            f"--- output tail ---\n{output_tail}\n"
            f"--- recent events ---\n{event_summary}"
        )

    def wait_until_ready(self, timeout: float = 20.0) -> None:
        if self.tool == "codex":
            self.runner.read_available(min(timeout, 3.0))
            return
        hook_kind = "claude-hook" if self.tool == "claude" else "codex-hook"
        self.wait_for(
            lambda event: (
                event.get("kind") == "response" and extract_response_state(event) == "idle"
            ) or (
                event.get("kind") == hook_kind and extract_event_name(event) == "SessionStart"
            ),
            timeout=timeout,
            include_existing=True,
        )

    def send_and_wait_for_prompt_submit(self, prompt: str) -> dict:
        self.runner.send_prompt(prompt)
        return self.wait_for(
            lambda event: event.get("kind") in {f"{self.tool}-hook", "claude-hook", "codex-hook"}
            and extract_event_name(event) == "UserPromptSubmit"
        )

    def wait_for_response_state(self, state: str, timeout: float = 30.0) -> dict:
        return self.wait_for(
            lambda event: event.get("kind") == "response"
            and extract_response_state(event) == state,
            timeout=timeout,
        )

    def wait_for_stop_like(self, timeout: float = 30.0) -> dict:
        hook_kind = "claude-hook" if self.tool == "claude" else "codex-hook"
        stop_events = {"Stop", "SessionEnd"} if self.tool == "claude" else {"Stop"}
        return self.wait_for(
            lambda event: event.get("kind") == hook_kind and extract_event_name(event) in stop_events,
            timeout=timeout,
        )

    def print_report(self) -> None:
        print(f"tool={self.tool}")
        print(f"model={self.model}")
        print(f"external_session_id={self.external_session_id}")
        if self.steps:
            print("--- steps ---")
            for step in self.steps:
                print(f"{step.status.upper()} {step.name}: {step.detail}")
        print("--- output tail ---")
        print(self.runner.output_text()[-4000:])
        print("--- event summary ---")
        for line in summarize_events(self.server.snapshot()):
            print(line)

    def report_data(self) -> dict:
        return {
            "tool": self.tool,
            "model": self.model,
            "external_session_id": self.external_session_id,
            "steps": [dataclasses.asdict(step) for step in self.steps],
            "events": self.server.snapshot(),
            "output_tail": self.runner.output_text()[-4000:] if self._runner else "",
        }

    def record_step(self, name: str, status: str, detail: str) -> None:
        self.steps.append(
            StepResult(
                name=name,
                status=status,
                detail=detail,
                event_count=self.server.event_count(),
            )
        )


def resume_args(tool: str, external_session_id: str) -> list[str]:
    if tool == "claude":
        return ["--resume", external_session_id]
    if tool == "codex":
        return ["resume", external_session_id]
    raise ValueError(tool)


def scenario_interrupt(tool: str, model: str) -> tuple[int, ScenarioContext]:
    ctx = ScenarioContext(tool, model)
    try:
        ctx.start()
        ctx.wait_until_ready()
        ctx.record_step("ready", "ok", "initial runtime ready")
        ctx.send_and_wait_for_prompt_submit("你好")
        ctx.record_step("prompt_submit", "ok", "captured UserPromptSubmit")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("responding", "ok", "captured responding state")
        except TimeoutError:
            ctx.record_step("responding", "warn", "responding state did not arrive before interrupt")
        ctx.runner.interrupt()
        ctx.record_step("interrupt", "ok", "sent Ctrl-C")
        try:
            ctx.wait_for_stop_like(timeout=10.0)
            ctx.record_step("stop_like", "ok", "captured stop/session-end hook")
        except TimeoutError:
            ctx.record_step("stop_like", "warn", "no stop-like hook observed after interrupt")
        try:
            ctx.wait_for_response_state("idle", timeout=10.0)
            ctx.record_step("idle", "ok", "captured idle response after interrupt")
        except TimeoutError:
            ctx.record_step("idle", "warn", "idle response not observed after interrupt")
        ctx.runner.read_available(1.0)
        return 0, ctx
    except Exception as error:
        ctx.record_step("scenario_error", "error", str(error))
        return 1, ctx


def scenario_flow(tool: str, model: str) -> tuple[int, ScenarioContext]:
    ctx = ScenarioContext(tool, model)
    try:
        ctx.start()
        ctx.wait_until_ready()
        ctx.record_step("ready", "ok", "initial runtime ready")

        # 1. Two prompts in a fresh session
        ctx.send_and_wait_for_prompt_submit("Reply with exactly OK.")
        ctx.record_step("fresh_prompt_1_submit", "ok", "first prompt submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("fresh_prompt_1_responding", "ok", "first prompt entered responding")
        except TimeoutError:
            ctx.record_step("fresh_prompt_1_responding", "warn", "responding not observed")
        ctx.wait_for_stop_like(timeout=60.0)
        ctx.record_step("fresh_prompt_1_stop", "ok", "first prompt stop observed")
        ctx.wait_for_response_state("idle", timeout=10.0)
        ctx.record_step("fresh_prompt_1_idle", "ok", "first prompt idle observed")

        ctx.send_and_wait_for_prompt_submit("Reply with exactly SECOND.")
        ctx.record_step("fresh_prompt_2_submit", "ok", "second prompt submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("fresh_prompt_2_responding", "ok", "second prompt entered responding")
        except TimeoutError:
            ctx.record_step("fresh_prompt_2_responding", "warn", "responding not observed")
        ctx.wait_for_stop_like(timeout=60.0)
        ctx.record_step("fresh_prompt_2_stop", "ok", "second prompt stop observed")
        ctx.wait_for_response_state("idle", timeout=10.0)
        ctx.record_step("fresh_prompt_2_idle", "ok", "second prompt idle observed")

        # 2. Interrupt during a third turn
        ctx.send_and_wait_for_prompt_submit("Write a long answer and keep going.")
        ctx.record_step("interrupt_prompt_submit", "ok", "interrupt prompt submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("interrupt_prompt_responding", "ok", "interrupt prompt entered responding")
        except TimeoutError:
            ctx.record_step("interrupt_prompt_responding", "warn", "responding not observed before interrupt")
        ctx.runner.interrupt()
        ctx.record_step("interrupt_sent", "ok", "sent Ctrl-C")
        try:
            ctx.wait_for_stop_like(timeout=10.0)
            ctx.record_step("interrupt_stop", "ok", "stop/session-end observed after interrupt")
        except TimeoutError:
            ctx.record_step("interrupt_stop", "warn", "no stop/session-end observed after interrupt")
        try:
            ctx.wait_for_response_state("idle", timeout=10.0)
            ctx.record_step("interrupt_idle", "ok", "idle observed after interrupt")
        except TimeoutError:
            ctx.record_step("interrupt_idle", "warn", "idle not observed after interrupt")

        external_session_id = ctx.external_session_id
        if not external_session_id:
            raise RuntimeError("failed to capture external session id for resume")
        ctx.record_step("capture_external_session", "ok", external_session_id)

        # 3. Close and reopen into resume flow
        ctx.runner.close()
        ctx._runner = None
        ctx.start(args=resume_args(tool, external_session_id))
        ctx.wait_until_ready()
        ctx.record_step("resume_open", "ok", "reopened historical session")

        # 4. Resume and send one prompt
        ctx.send_and_wait_for_prompt_submit("Reply with exactly RESUMED.")
        ctx.record_step("resume_prompt_submit", "ok", "resume prompt submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("resume_prompt_responding", "ok", "resume prompt entered responding")
        except TimeoutError:
            ctx.record_step("resume_prompt_responding", "warn", "responding not observed")
        ctx.wait_for_stop_like(timeout=60.0)
        ctx.record_step("resume_prompt_stop", "ok", "resume prompt stop observed")
        ctx.wait_for_response_state("idle", timeout=10.0)
        ctx.record_step("resume_prompt_idle", "ok", "resume prompt idle observed")

        # 5. Reopen once more and start fresh before resuming historical session
        ctx.runner.close()
        ctx._runner = None
        fresh_runner = ctx.start()
        ctx.wait_until_ready()
        ctx.record_step("fresh_reopen", "ok", "opened fresh session after resume")
        ctx.send_and_wait_for_prompt_submit("Reply with exactly FRESH.")
        ctx.record_step("fresh_after_resume_submit", "ok", "fresh prompt after resume submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("fresh_after_resume_responding", "ok", "fresh prompt entered responding")
        except TimeoutError:
            ctx.record_step("fresh_after_resume_responding", "warn", "responding not observed")
        ctx.wait_for_stop_like(timeout=60.0)
        ctx.record_step("fresh_after_resume_stop", "ok", "fresh prompt stop observed")
        ctx.wait_for_response_state("idle", timeout=10.0)
        ctx.record_step("fresh_after_resume_idle", "ok", "fresh prompt idle observed")
        fresh_runner.close()
        ctx._runner = None

        ctx.start(args=resume_args(tool, external_session_id))
        ctx.wait_until_ready()
        ctx.record_step("history_reopen", "ok", "reopened original historical session")
        ctx.send_and_wait_for_prompt_submit("Reply with exactly HISTORY.")
        ctx.record_step("history_prompt_submit", "ok", "history prompt submitted")
        try:
            ctx.wait_for_response_state("responding", timeout=10.0)
            ctx.record_step("history_prompt_responding", "ok", "history prompt entered responding")
        except TimeoutError:
            ctx.record_step("history_prompt_responding", "warn", "responding not observed")
        ctx.wait_for_stop_like(timeout=60.0)
        ctx.record_step("history_prompt_stop", "ok", "history prompt stop observed")
        ctx.wait_for_response_state("idle", timeout=10.0)
        ctx.record_step("history_prompt_idle", "ok", "history prompt idle observed")

        ctx.runner.read_available(1.0)
        return 0, ctx
    except Exception as error:
        ctx.record_step("scenario_error", "error", str(error))
        return 1, ctx


def finalize(code: int, ctx: ScenarioContext, report_json_path: str | None) -> int:
    ctx.print_report()
    if report_json_path:
        path = Path(report_json_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(ctx.report_data(), ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"report_json={path}")
    ctx.close()
    return code


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tool", choices=["claude", "codex"], required=True)
    parser.add_argument("--scenario", choices=["interrupt", "flow"], default="interrupt")
    parser.add_argument("--model", default=None)
    parser.add_argument("--report-json", default=None)
    args = parser.parse_args()

    model = args.model or DEFAULT_MODELS[args.tool]
    if args.scenario == "interrupt":
        code, ctx = scenario_interrupt(args.tool, model)
        return finalize(code, ctx, args.report_json)
    if args.scenario == "flow":
        code, ctx = scenario_flow(args.tool, model)
        return finalize(code, ctx, args.report_json)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
