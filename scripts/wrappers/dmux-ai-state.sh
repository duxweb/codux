#!/bin/zsh
set -uo pipefail

zmodload zsh/datetime 2>/dev/null || true

action="${1:-}"
hook_owner=""
if [[ "$#" -ge 3 ]]; then
  hook_owner="${2:-}"
  tool_name="${3:-${DMUX_ACTIVE_AI_TOOL:-}}"
else
  tool_name="${2:-${DMUX_ACTIVE_AI_TOOL:-}}"
fi
hook_payload="$(cat)"
notification_type=""
should_send_response_event=1

if [[ -n "${hook_owner:-}" && "${DMUX_RUNTIME_OWNER:-}" != "${hook_owner}" ]]; then
  exit 0
fi

if [[ -z "${DMUX_SESSION_ID:-}" || -z "${DMUX_PROJECT_ID:-}" || -z "${tool_name:-}" ]]; then
  exit 0
fi

case "${action}" in
  session-start)
    response_state="idle"
    ;;
  prompt-submit|pre-tool-use|post-tool-use|post-tool-use-failure|before-agent|permission-request)
    response_state="responding"
    ;;
  permission-denied|elicitation|elicitation-result)
    response_state=""
    should_send_response_event=0
    ;;
  notification)
    notification_type="$(HOOK_PAYLOAD="${hook_payload}" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
if not payload:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

def first_string(mapping, *keys):
    if not isinstance(mapping, dict):
        return None
    for key in keys:
        value = mapping.get(key)
        if isinstance(value, str) and value:
            return value
    return None

value = first_string(obj, "notification_type")
if value is None:
    value = first_string(obj.get("notification"), "notification_type", "type", "kind", "reason")
if value is None:
    value = first_string(obj.get("data"), "notification_type", "type", "kind", "reason")
if value:
    print(value)
PY
)"
    if [[ "${notification_type:-}" == "idle_prompt" ]]; then
      response_state="idle"
    else
      response_state=""
      should_send_response_event=0
    fi
    ;;
  stop|stop-failure|session-end|idle|after-agent)
    response_state="idle"
    ;;
  codex-prompt-submit|codex-stop)
    response_state=""
    ;;
  codex-pre-tool-use|codex-post-tool-use)
    response_state="responding"
    ;;
  *)
    exit 0
    ;;
esac

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  print -rn -- "$value"
}

now() {
  if [[ -n "${EPOCHREALTIME:-}" ]]; then
    printf "%.3f" "${EPOCHREALTIME}"
  elif [[ -n "${EPOCHSECONDS:-}" ]]; then
    printf "%.3f" "${EPOCHSECONDS}"
  else
    /bin/date +%s | awk '{ printf "%.3f", $1 }'
  fi
}

log_line() {
  [[ -n "${DMUX_LOG_FILE:-}" ]] || return 0
  /bin/mkdir -p -- "${DMUX_LOG_FILE:h}"
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [wrapper] $1" >> "${DMUX_LOG_FILE}"
}

hook_payload_hash() {
  [[ -n "${hook_payload}" ]] || return 0
  print -rn -- "${hook_payload}" | /usr/bin/shasum -a 256 2>/dev/null | /usr/bin/awk '{print $1}'
}

hook_payload_size() {
  [[ -n "${hook_payload}" ]] || return 0
  print -rn -- "${#hook_payload}"
}

extract_hook_debug_summary() {
  [[ -n "${hook_payload}" ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
if not payload:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception as exc:
    print(f"parse_error={type(exc).__name__}")
    raise SystemExit(0)

def truncate(value, limit=140):
    if not isinstance(value, str):
        return ""
    value = " ".join(value.split())
    if len(value) <= limit:
        return value
    return value[: limit - 1] + "…"

def first_string(node, *keys):
    if not isinstance(node, dict):
        return None
    for key in keys:
        value = node.get(key)
        if isinstance(value, str) and value:
            return value
    return None

def first_content_text(node):
    if not isinstance(node, dict):
        return None
    candidates = []
    content = node.get("content")
    if isinstance(content, list):
        for item in content:
            if not isinstance(item, dict):
                continue
            text = first_string(item, "text")
            if text:
                candidates.append(text)
    for key in ("message", "text"):
        value = node.get(key)
        if isinstance(value, str) and value:
            candidates.append(value)
    for candidate in candidates:
        candidate = truncate(candidate)
        if candidate:
            return candidate
    return None

stack = [obj]
seen = set()
session_id = None
cwd = None
transcript = None
reason = None
source = None
prompt_preview = None
top_type = first_string(obj, "type") if isinstance(obj, dict) else None
payload_node = obj.get("payload") if isinstance(obj, dict) and isinstance(obj.get("payload"), dict) else None
payload_type = first_string(payload_node, "type") if payload_node else None
payload_phase = first_string(payload_node, "phase") if payload_node else None

while stack:
    current = stack.pop()
    ident = id(current)
    if ident in seen:
        continue
    seen.add(ident)

    if isinstance(current, dict):
        if session_id is None:
            session_id = first_string(current, "session_id", "sessionId")
        if cwd is None:
            cwd = first_string(current, "cwd")
        if transcript is None:
            transcript = first_string(current, "transcript_path", "transcriptPath")
        if reason is None:
            reason = first_string(current, "stop_reason", "reason")
        if source is None:
            source = first_string(current, "source")
        if prompt_preview is None:
            prompt_preview = first_content_text(current)
        stack.extend(current.values())
    elif isinstance(current, list):
        stack.extend(current)

parts = []
for key, value in (
    ("topType", top_type),
    ("payloadType", payload_type),
    ("phase", payload_phase),
    ("source", source),
    ("reason", reason),
    ("session", session_id),
    ("cwd", cwd),
    ("transcript", transcript),
    ("preview", prompt_preview),
):
    if value:
        parts.append(f"{key}={json.dumps(value, ensure_ascii=True)}")

print(" ".join(parts))
PY
}

log_codex_hook_diagnostics() {
  local summary
  summary="$(extract_hook_debug_summary)"
  local payload_hash
  payload_hash="$(hook_payload_hash)"
  local payload_size
  payload_size="$(hook_payload_size)"
  log_line "codex hook diag action=${action} tool=${tool_name} bytes=${payload_size:-0} hash=${payload_hash:-nil}${summary:+ ${summary}}"
}

extract_hook_session_id() {
  [[ -n "${hook_payload}" ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" /usr/bin/python3 - <<'PY'
import json
import os
import sys

payload = os.environ.get("HOOK_PAYLOAD", "")
if not payload:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

stack = [obj]
seen = set()
while stack:
    current = stack.pop()
    ident = id(current)
    if ident in seen:
        continue
    seen.add(ident)

    if isinstance(current, dict):
        for key in ("session_id", "sessionId"):
            value = current.get(key)
            if isinstance(value, str) and value:
                print(value)
                raise SystemExit(0)
        stack.extend(current.values())
    elif isinstance(current, list):
        stack.extend(current)
PY
}

extract_hook_field() {
  local field_name="$1"
  [[ -n "${hook_payload}" && -n "${field_name}" ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" HOOK_FIELD_NAME="${field_name}" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
field_name = os.environ.get("HOOK_FIELD_NAME", "")
if not payload or not field_name:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

stack = [obj]
seen = set()
while stack:
    current = stack.pop()
    ident = id(current)
    if ident in seen:
        continue
    seen.add(ident)

    if isinstance(current, dict):
        value = current.get(field_name)
        if isinstance(value, str) and value:
            print(value)
            raise SystemExit(0)
        stack.extend(current.values())
    elif isinstance(current, list):
        stack.extend(current)
PY
}

extract_first_hook_field() {
  [[ -n "${hook_payload}" && "$#" -gt 0 ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" HOOK_FIELD_NAMES="$*" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
field_names = [name for name in os.environ.get("HOOK_FIELD_NAMES", "").split(" ") if name]
if not payload or not field_names:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

stack = [obj]
seen = set()
while stack:
    current = stack.pop()
    ident = id(current)
    if ident in seen:
        continue
    seen.add(ident)

    if isinstance(current, dict):
        for field_name in field_names:
            value = current.get(field_name)
            if isinstance(value, str) and value:
                print(value)
                raise SystemExit(0)
        stack.extend(current.values())
    elif isinstance(current, list):
        stack.extend(current)
PY
}

extract_hook_number_field() {
  [[ -n "${hook_payload}" && "$#" -gt 0 ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" HOOK_FIELD_NAMES="$*" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
field_names = [name for name in os.environ.get("HOOK_FIELD_NAMES", "").split(" ") if name]
if not payload or not field_names:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

stack = [obj]
seen = set()
while stack:
    current = stack.pop()
    ident = id(current)
    if ident in seen:
        continue
    seen.add(ident)

    if isinstance(current, dict):
        for field_name in field_names:
            value = current.get(field_name)
            if isinstance(value, bool):
                continue
            if isinstance(value, int):
                print(value)
                raise SystemExit(0)
            if isinstance(value, float) and value.is_integer():
                print(int(value))
                raise SystemExit(0)
        stack.extend(current.values())
    elif isinstance(current, list):
        stack.extend(current)
PY
}

extract_hook_notification_type() {
  [[ -n "${hook_payload}" ]] || return 0
  HOOK_PAYLOAD="${hook_payload}" /usr/bin/python3 - <<'PY'
import json
import os

payload = os.environ.get("HOOK_PAYLOAD", "")
if not payload:
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except Exception:
    raise SystemExit(0)

def first_string(mapping, *keys):
    if not isinstance(mapping, dict):
        return None
    for key in keys:
        value = mapping.get(key)
        if isinstance(value, str) and value:
            return value
    return None

value = first_string(obj, "notification_type")
if value is None:
    value = first_string(obj.get("notification"), "notification_type", "type", "kind", "reason")
if value is None:
    value = first_string(obj.get("data"), "notification_type", "type", "kind", "reason")
if value:
    print(value)
PY
}

resolved_claude_external_session_id() {
  local parsed_session_id
  parsed_session_id="$(extract_hook_session_id)"
  if [[ -n "${parsed_session_id}" ]]; then
    print -r -- "${parsed_session_id}"
    return 0
  fi

  if [[ -n "${DMUX_EXTERNAL_SESSION_ID:-}" ]]; then
    print -r -- "${DMUX_EXTERNAL_SESSION_ID}"
  fi
}

resolved_hook_model() {
  local model_value
  model_value="$(extract_first_hook_field model model_name modelName)"
  if [[ -n "${model_value}" ]]; then
    print -r -- "${model_value}"
    return 0
  fi

  if [[ -n "${DMUX_ACTIVE_AI_MODEL:-}" ]]; then
    print -r -- "${DMUX_ACTIVE_AI_MODEL}"
  fi
}

write_claude_session_map() {
  local external_session_id
  external_session_id="$(resolved_claude_external_session_id)"
  [[ -n "${DMUX_CLAUDE_SESSION_MAP_DIR:-}" && -n "${DMUX_SESSION_ID:-}" && -n "${external_session_id:-}" ]] || return 0
  local path="${DMUX_CLAUDE_SESSION_MAP_DIR}/${DMUX_SESSION_ID}.json"
  local tmp="${path}.tmp"
  /bin/mkdir -p -- "${DMUX_CLAUDE_SESSION_MAP_DIR}"
  {
    print -rn -- '{'
    print -rn -- "\"sessionId\":\"$(json_escape "${DMUX_SESSION_ID}")\","
    print -rn -- "\"externalSessionID\":\"$(json_escape "${external_session_id}")\","
    print -rn -- "\"updatedAt\":$(now)"
    print -r -- '}'
  } >| "${tmp}"
  /bin/mv -f -- "${tmp}" "${path}"
  log_line "claude map write session=${DMUX_SESSION_ID} externalSession=${external_session_id}"
}

clear_response_file() {
  return 0
}

send_runtime_event() {
  local payload="$1"
  [[ -n "${DMUX_RUNTIME_SOCKET:-}" ]] || {
    log_line "hook skip action=${action} tool=${tool_name} reason=no-runtime-socket"
    return 0
  }
  command -v /usr/bin/nc >/dev/null 2>&1 || {
    log_line "hook skip action=${action} tool=${tool_name} reason=no-nc"
    return 0
  }
  if print -r -- "${payload}" | /usr/bin/nc -U -w 1 "${DMUX_RUNTIME_SOCKET}" >/dev/null 2>&1; then
    log_line "hook sent action=${action} tool=${tool_name} socket=${DMUX_RUNTIME_SOCKET}"
  else
    log_line "hook send failed action=${action} tool=${tool_name} socket=${DMUX_RUNTIME_SOCKET}"
  fi
}

clear_claude_session_map() {
  [[ -n "${DMUX_CLAUDE_SESSION_MAP_DIR:-}" ]] || return 0
  /bin/rm -f -- "${DMUX_CLAUDE_SESSION_MAP_DIR}/${DMUX_SESSION_ID}.json"
}

write_ai_hook_event() {
  local event_kind="$1"
  local ai_session_id="${2:-}"
  local model_value="${3:-}"
  local total_tokens="${4:-}"
  local transcript_path="${5:-}"
  local notification_value="${6:-}"
  local source_value="${7:-}"
  local reason_value="${8:-}"
  local target_tool_name="${9:-}"
  local message_value="${10:-}"
  [[ -n "${total_tokens}" ]] || total_tokens="null"
  local event_json
  event_json="$(
    {
      print -rn -- '{"kind":"ai-hook","payload":{'
      print -rn -- "\"kind\":\"$(json_escape "${event_kind}")\","
      print -rn -- "\"terminalID\":\"$(json_escape "${DMUX_SESSION_ID}")\","
      if [[ -n "${DMUX_SESSION_INSTANCE_ID:-}" ]]; then
        print -rn -- "\"terminalInstanceID\":\"$(json_escape "${DMUX_SESSION_INSTANCE_ID}")\","
      else
        print -rn -- "\"terminalInstanceID\":null,"
      fi
      print -rn -- "\"projectID\":\"$(json_escape "${DMUX_PROJECT_ID}")\","
      print -rn -- "\"projectName\":\"$(json_escape "${DMUX_PROJECT_NAME:-Workspace}")\","
      print -rn -- "\"projectPath\":\"$(json_escape "${DMUX_PROJECT_PATH:-}")\","
      print -rn -- "\"sessionTitle\":\"$(json_escape "${DMUX_SESSION_TITLE:-Terminal}")\","
      print -rn -- "\"tool\":\"$(json_escape "${tool_name}")\","
      if [[ -n "${ai_session_id}" ]]; then
        print -rn -- "\"aiSessionID\":\"$(json_escape "${ai_session_id}")\","
      else
        print -rn -- "\"aiSessionID\":null,"
      fi
      if [[ -n "${model_value}" ]]; then
        print -rn -- "\"model\":\"$(json_escape "${model_value}")\","
      else
        print -rn -- "\"model\":null,"
      fi
      print -rn -- "\"totalTokens\":${total_tokens},"
      print -rn -- "\"updatedAt\":$(now),"
      print -rn -- "\"metadata\":{"
      if [[ -n "${transcript_path}" ]]; then
        print -rn -- "\"transcriptPath\":\"$(json_escape "${transcript_path}")\""
      else
        print -rn -- "\"transcriptPath\":null"
      fi
      if [[ -n "${notification_value}" ]]; then
        print -rn -- ",\"notificationType\":\"$(json_escape "${notification_value}")\""
      fi
      if [[ -n "${source_value}" ]]; then
        print -rn -- ",\"source\":\"$(json_escape "${source_value}")\""
      fi
      if [[ -n "${reason_value}" ]]; then
        print -rn -- ",\"reason\":\"$(json_escape "${reason_value}")\""
      fi
      if [[ -n "${target_tool_name}" ]]; then
        print -rn -- ",\"targetToolName\":\"$(json_escape "${target_tool_name}")\""
      fi
      if [[ -n "${message_value}" ]]; then
        print -rn -- ",\"message\":\"$(json_escape "${message_value}")\""
      fi
      print -rn -- "}"
      print -rn -- '}}'
    }
  )"
  send_runtime_event "${event_json}"
  log_line "ai hook kind=${event_kind} tool=${tool_name} session=${DMUX_SESSION_ID} externalSession=${ai_session_id:-nil}"
}

case "${action}" in
  codex-session-start)
    write_ai_hook_event \
      "sessionStarted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "" \
      "" \
      "$(extract_first_hook_field source)"
    exit 0
    ;;
  codex-prompt-submit)
    log_codex_hook_diagnostics
    write_ai_hook_event \
      "promptSubmitted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "" \
      "" \
      "user-input"
    exit 0
    ;;
  codex-stop)
    log_codex_hook_diagnostics
    codex_total_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
    [[ -z "${codex_total_tokens}" ]] && codex_total_tokens="null"
    write_ai_hook_event \
      "turnCompleted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "${codex_total_tokens}" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "" \
      "" \
      "$(extract_first_hook_field stop_reason reason)"
    exit 0
    ;;
esac

if [[ "${tool_name}" == "claude" || "${tool_name}" == "claude-code" ]]; then
  case "${action}" in
    session-start)
      claude_total_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
      [[ -z "${claude_total_tokens}" ]] && claude_total_tokens="null"
      write_ai_hook_event \
        "sessionStarted" \
        "$(resolved_claude_external_session_id)" \
        "$(resolved_hook_model)" \
        "${claude_total_tokens}" \
        "" \
        "" \
        "$(extract_first_hook_field source)"
      write_claude_session_map
      log_line "claude hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-} externalSession=$(resolved_claude_external_session_id || print -r -- nil)"
      ;;
    prompt-submit|permission-request|permission-denied|notification|elicitation|elicitation-result)
      write_claude_session_map
      if [[ "${action}" == "prompt-submit" ]]; then
        claude_prompt_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
        [[ -z "${claude_prompt_tokens}" ]] && claude_prompt_tokens="null"
        write_ai_hook_event \
          "promptSubmitted" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "${claude_prompt_tokens}" \
          "" \
          "" \
          "user-input"
      elif [[ "${action}" == "permission-request" ]]; then
        write_ai_hook_event \
          "needsInput" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "permission-request" \
          "" \
          "permission-request" \
          "$(extract_first_hook_field tool_name toolName tool)" \
          "$(extract_first_hook_field message prompt)"
      elif [[ "${action}" == "permission-denied" ]]; then
        write_ai_hook_event \
          "needsInput" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "permission-denied" \
          "" \
          "permission-denied" \
          "$(extract_first_hook_field tool_name toolName tool)" \
          "$(extract_first_hook_field message prompt)"
      elif [[ "${action}" == "elicitation" ]]; then
        write_ai_hook_event \
          "needsInput" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "elicitation" \
          "" \
          "elicitation" \
          "" \
          "$(extract_first_hook_field message prompt)"
      elif [[ "${action}" == "elicitation-result" ]]; then
        write_ai_hook_event \
          "promptSubmitted" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
          "" \
          "" \
          "user-input"
      fi
      if [[ "${action}" == "notification" ]]; then
        log_line "claude hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-} notificationType=${notification_type:-unknown}"
      else
        log_line "claude hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      fi
      ;;
    stop|stop-failure|idle)
      write_claude_session_map
      claude_stop_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
      [[ -z "${claude_stop_tokens}" ]] && claude_stop_tokens="null"
      if [[ "${action}" == "stop" || "${action}" == "stop-failure" ]]; then
        write_ai_hook_event \
          "turnCompleted" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "${claude_stop_tokens}" \
          "" \
          "" \
          "" \
          "$(extract_first_hook_field stop_reason reason)"
      else
        write_ai_hook_event \
          "sessionEnded" \
          "$(resolved_claude_external_session_id)" \
          "$(resolved_hook_model)" \
          "${claude_stop_tokens}" \
          "" \
          "" \
          "" \
          "$(extract_first_hook_field reason)"
      fi
      log_line "claude hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    session-end)
      write_ai_hook_event \
        "sessionEnded" \
        "$(resolved_claude_external_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason)"
      log_line "claude hook action=${action} clear-response session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      clear_response_file
      clear_claude_session_map
      ;;
  esac
fi

if [[ "${tool_name}" == "gemini" ]]; then
  case "${action}" in
    session-start|before-agent|after-agent)
      gemini_total_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
      [[ -z "${gemini_total_tokens}" ]] && gemini_total_tokens="null"
      case "${action}" in
        session-start)
          write_ai_hook_event \
            "sessionStarted" \
            "$(extract_hook_session_id)" \
            "$(resolved_hook_model)" \
            "${gemini_total_tokens}" \
            "" \
            "" \
            "$(extract_first_hook_field source)"
          ;;
        before-agent)
          write_ai_hook_event \
            "promptSubmitted" \
            "$(extract_hook_session_id)" \
            "$(resolved_hook_model)" \
            "${gemini_total_tokens}" \
            "" \
            "" \
            "user-input"
          ;;
        after-agent)
          write_ai_hook_event \
            "turnCompleted" \
            "$(extract_hook_session_id)" \
            "$(resolved_hook_model)" \
            "${gemini_total_tokens}"
          ;;
      esac
      log_line "gemini hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    notification)
      gemini_notification_type="$(extract_hook_notification_type)"
      if [[ -n "${gemini_notification_type}" ]]; then
        write_ai_hook_event \
          "needsInput" \
          "$(extract_hook_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "${gemini_notification_type}" \
          "" \
          "${gemini_notification_type}" \
          "$(extract_first_hook_field tool_name toolName tool)" \
          "$(extract_first_hook_field message)"
      fi
      ;;
    session-end)
      write_ai_hook_event \
        "sessionEnded" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason)"
      log_line "gemini hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
  esac
fi

if [[ "${should_send_response_event}" != "1" || -z "${DMUX_RUNTIME_SOCKET:-}" ]]; then
  exit 0
fi

response_json="$(
  {
    print -rn -- '{"kind":"response","payload":{'
    print -rn -- "\"sessionId\":\"$(json_escape "${DMUX_SESSION_ID}")\","
    print -rn -- "\"sessionInstanceId\":\"$(json_escape "${DMUX_SESSION_INSTANCE_ID:-}")\","
    if [[ -n "${DMUX_ACTIVE_AI_INVOCATION_ID:-}" ]]; then
      print -rn -- "\"invocationId\":\"$(json_escape "${DMUX_ACTIVE_AI_INVOCATION_ID}")\","
    else
      print -rn -- "\"invocationId\":null,"
    fi
    print -rn -- "\"projectId\":\"$(json_escape "${DMUX_PROJECT_ID}")\","
    print -rn -- "\"projectPath\":\"$(json_escape "${DMUX_PROJECT_PATH:-}")\","
    print -rn -- "\"tool\":\"$(json_escape "${tool_name}")\","
    print -rn -- "\"responseState\":\"$(json_escape "${response_state}")\","
    print -rn -- "\"updatedAt\":$(now)"
    print -rn -- '}}'
  }
)"
send_runtime_event "${response_json}"
