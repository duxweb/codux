#!/usr/bin/env python3
"""
Developer-only non-interactive flow runner for real codex/claude wrappers.

Uses:
- codex exec --json / codex exec resume --json
- claude --print --output-format stream-json --verbose

This bypasses interactive TUI issues while still exercising real models and
session persistence / resume behavior.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WRAPPER_BIN = ROOT / "scripts" / "wrappers" / "bin"

DEFAULT_MODELS = {
    "claude": "claude-haiku-4-5",
    "codex": "gpt-5.1-codex-mini",
    "gemini": "gemini-2.5-flash",
    "opencode": "minimax/minimax-m2.5-free",
}

DEFAULT_MODEL_CANDIDATES = {
    "codex": [
        "gpt-5.1-codex-mini",
        "gpt-5.2-codex",
        "gpt-5.4-mini",
        "gpt-5.4",
    ],
    "opencode": [
        "minimax/minimax-m2.5-free",
        "",
    ],
}


def parse_jsonl(text: str) -> list[dict]:
    items: list[dict] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            items.append(json.loads(line))
        except Exception:
            items.append({"decode_error": line})
    return items


def parse_first_json_object(text: str) -> dict:
    stripped = text.lstrip()
    if not stripped.startswith("{"):
        return {}
    decoder = json.JSONDecoder()
    try:
        obj, _ = decoder.raw_decode(stripped)
    except Exception:
        return {}
    return obj if isinstance(obj, dict) else {}


def summarize_events(items: list[dict]) -> list[dict]:
    summary: list[dict] = []
    for item in items:
        row_type = item.get("type")
        if row_type is None:
            if "response" in item and "session_id" in item:
                row_type = "result"
            else:
                row_type = "object"
        row: dict = {"type": row_type}
        if "thread_id" in item:
            row["thread_id"] = item.get("thread_id")
        if "session_id" in item:
            row["session_id"] = item.get("session_id")
        if "response" in item:
            row["response"] = item.get("response")
        if item.get("type") == "error":
            row["message"] = item.get("message")
        if item.get("type") == "result":
            row["subtype"] = item.get("subtype")
            row["is_error"] = item.get("is_error")
            row["result"] = item.get("result")
            row["stop_reason"] = item.get("stop_reason")
        elif row_type == "result":
            row["result"] = item.get("response")
        if item.get("type") == "turn.failed":
            error = item.get("error") or {}
            row["message"] = error.get("message")
        if item.get("type") == "item.completed":
            payload = item.get("item") or {}
            row["item_type"] = payload.get("type")
            row["text"] = payload.get("text")
        summary.append(row)
    return summary


def codex_run(model: str, prompt: str, resume_id: str | None = None) -> tuple[int, list[dict], str]:
    cmd = [str(WRAPPER_BIN / "codex"), "exec", "--json", "--model", model]
    if resume_id:
        cmd.extend(["resume", resume_id, prompt])
    else:
        cmd.append(prompt)
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=os.environ.copy(),
    )
    events = parse_jsonl(proc.stdout)
    session_id = resume_id or next((item.get("thread_id") for item in events if item.get("type") == "thread.started"), None) or ""
    return proc.returncode, events, session_id


def claude_run(model: str, prompt: str, resume_id: str | None = None) -> tuple[int, list[dict], str]:
    cmd = [
        str(WRAPPER_BIN / "claude"),
        "--print",
        "--verbose",
        "--output-format",
        "stream-json",
        "--include-hook-events",
        "--model",
        model,
    ]
    if resume_id:
        cmd.extend(["--resume", resume_id])
    cmd.append(prompt)
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=os.environ.copy(),
    )
    events = parse_jsonl(proc.stdout)
    session_id = resume_id or next((item.get("session_id") for item in events if item.get("type") == "system" and item.get("subtype") == "init"), None) or ""
    return proc.returncode, events, session_id


def gemini_run(model: str, prompt: str, resume_id: str | None = None) -> tuple[int, list[dict], str]:
    cmd = ["gemini", "-p", prompt, "-m", model, "-o", "json"]
    if resume_id:
        cmd = ["gemini", "--resume", resume_id, "-p", prompt, "-m", model, "-o", "json"]
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=os.environ.copy(),
    )
    events = parse_jsonl(proc.stdout)
    payload = parse_first_json_object(proc.stdout)
    session_id = resume_id or payload.get("session_id") or ""
    if payload:
        events.append(payload)
    return proc.returncode, events, session_id


def opencode_run(model: str, prompt: str, resume_id: str | None = None) -> tuple[int, list[dict], str]:
    cmd = ["opencode", "run", "--format", "json"]
    if model:
        cmd.extend(["--model", model])
    if resume_id:
        cmd.extend(["--session", resume_id])
    cmd.append(prompt)
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=os.environ.copy(),
    )
    events = parse_jsonl(proc.stdout)
    session_id = resume_id or next((item.get("sessionID") for item in events if item.get("sessionID")), None) or ""
    return proc.returncode, events, session_id


def opencode_provider_list() -> str:
    proc = subprocess.run(
        ["opencode", "providers", "list"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=os.environ.copy(),
    )
    return (proc.stdout or "") + ("\n" + proc.stderr if proc.stderr else "")


def opencode_has_provider_hint(name: str) -> bool:
    listing = opencode_provider_list().lower()
    return name.lower() in listing


def finalize_report(report: dict, report_json: str | None) -> int:
    text = json.dumps(report, ensure_ascii=False, indent=2)
    print(text)
    if report_json:
        path = Path(report_json)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    steps = report.get("steps")
    if isinstance(steps, list):
        return 0 if all(step.get("exit_code") == 0 for step in steps) else 1
    attempts = report.get("attempts")
    if isinstance(attempts, list):
        return 0 if any(
            isinstance(attempt.get("steps"), list)
            and all(step.get("exit_code") == 0 for step in attempt.get("steps", []))
            for attempt in attempts
        ) else 1
    return 1


def run_flow(tool: str, model: str, report_json: str | None = None) -> int:
    run = {
        "claude": claude_run,
        "codex": codex_run,
        "gemini": gemini_run,
        "opencode": opencode_run,
    }[tool]
    prompts = [
        "Reply with exactly FIRST.",
        "Reply with exactly SECOND.",
    ]

    report: dict = {"tool": tool, "model": model, "steps": []}

    code, events, session_id = run(model, prompts[0], None)
    report["steps"].append({
        "name": "fresh_prompt_1",
        "exit_code": code,
        "session_id": session_id,
        "events": summarize_events(events),
    })
    if code != 0 or not session_id:
        return finalize_report(report, report_json)

    code, events, _ = run(model, prompts[1], session_id)
    report["steps"].append({
        "name": "resume_prompt_2",
        "exit_code": code,
        "session_id": session_id,
        "events": summarize_events(events),
    })

    fresh_prompt = "Reply with exactly FRESH."
    code_fresh, fresh_events, fresh_session = run(model, fresh_prompt, None)
    report["steps"].append({
        "name": "fresh_after_resume",
        "exit_code": code_fresh,
        "session_id": fresh_session,
        "events": summarize_events(fresh_events),
    })

    code_hist, hist_events, _ = run(model, "Reply with exactly HISTORY.", session_id)
    report["steps"].append({
        "name": "resume_history_again",
        "exit_code": code_hist,
        "session_id": session_id,
        "events": summarize_events(hist_events),
    })

    return finalize_report(report, report_json)


def run_flow_with_candidates(tool: str, explicit_model: str | None, report_json: str | None = None) -> int:
    if tool not in {"codex", "opencode"}:
        return run_flow(tool, explicit_model or DEFAULT_MODELS[tool], report_json)

    if explicit_model:
        candidates = [explicit_model]
    elif tool == "opencode":
        candidates = []
        if opencode_has_provider_hint("minimax"):
            candidates.append("minimax/minimax-m2.5-free")
        candidates.append("")
    else:
        candidates = DEFAULT_MODEL_CANDIDATES[tool]
    attempts: list[dict] = []
    for model in candidates:
        selected_model = model or "(default)"
        report: dict = {"tool": tool, "model": selected_model, "steps": []}
        prompts = [
            "Reply with exactly FIRST.",
            "Reply with exactly SECOND.",
        ]

        run = codex_run if tool == "codex" else opencode_run
        code, events, session_id = run(model, prompts[0], None)
        report["steps"].append({
            "name": "fresh_prompt_1",
            "exit_code": code,
            "session_id": session_id,
            "events": summarize_events(events),
        })
        if code != 0 or not session_id:
            attempts.append(report)
            continue

        code, events, _ = run(model, prompts[1], session_id)
        report["steps"].append({
            "name": "resume_prompt_2",
            "exit_code": code,
            "session_id": session_id,
            "events": summarize_events(events),
        })

        fresh_prompt = "Reply with exactly FRESH."
        code_fresh, fresh_events, fresh_session = run(model, fresh_prompt, None)
        report["steps"].append({
            "name": "fresh_after_resume",
            "exit_code": code_fresh,
            "session_id": fresh_session,
            "events": summarize_events(fresh_events),
        })

        code_hist, hist_events, _ = run(model, "Reply with exactly HISTORY.", session_id)
        report["steps"].append({
            "name": "resume_history_again",
            "exit_code": code_hist,
            "session_id": session_id,
            "events": summarize_events(hist_events),
        })

        if all(step["exit_code"] == 0 for step in report["steps"]):
            return finalize_report(report, report_json)
        attempts.append(report)
    return finalize_report({"tool": tool, "attempts": attempts}, report_json)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tool", choices=["claude", "codex", "gemini", "opencode"], required=True)
    parser.add_argument("--model", default=None)
    parser.add_argument("--report-json", default=None)
    args = parser.parse_args()
    if args.tool in {"codex", "opencode"}:
        return run_flow_with_candidates(args.tool, args.model, args.report_json)
    model = args.model or DEFAULT_MODELS[args.tool]
    return run_flow(args.tool, model, args.report_json)


if __name__ == "__main__":
    raise SystemExit(main())
