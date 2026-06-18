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

read_hook_payload() {
  [[ ! -t 0 ]] || return 0
  cat
}

hook_payload="$(read_hook_payload)"
notification_type=""
should_emit_claude_memory_context=false

if [[ -n "${hook_owner:-}" && "${DMUX_RUNTIME_OWNER:-}" != "${hook_owner}" ]]; then
  exit 0
fi

if [[ -z "${DMUX_SESSION_ID:-}" || -z "${DMUX_PROJECT_ID:-}" || -z "${tool_name:-}" ]]; then
  exit 0
fi

case "${action}" in
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
    ;;
  session-start|prompt-submit|before-agent|permission-request|permission-denied|elicitation|elicitation-result|pre-compact|post-compact|stop|stop-failure|session-end|idle|after-agent|codex-session-start|codex-prompt-submit|codex-pre-tool-use|codex-post-tool-use|codex-permission-request|codex-stop|codex-session-end|codewhale-session-start|codewhale-message-submit|codewhale-tool-call-before|codewhale-tool-call-after|codewhale-error|codewhale-session-end)
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
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [hook-file] $1" >> "${DMUX_LOG_FILE}"
}

claude_memory_additional_context() {
  [[ -n "${DMUX_AI_MEMORY_INDEX_FILE:-}" && -f "${DMUX_AI_MEMORY_INDEX_FILE}" ]] || return 0
  MEMORY_INDEX_FILE="${DMUX_AI_MEMORY_INDEX_FILE}" CLAUDE_HOOK_EVENT_NAME="${CLAUDE_HOOK_EVENT_NAME:-UserPromptSubmit}" /usr/bin/python3 - <<'PY'
import json
import os
from pathlib import Path

path = Path(os.environ.get("MEMORY_INDEX_FILE", ""))
try:
    text = path.read_text(encoding="utf-8").strip()
except Exception:
    raise SystemExit(0)

if not text:
    raise SystemExit(0)

prefix = (
    "Codux memory refresh: the conversation may have been compacted, "
    "or this is a new user turn. Re-apply relevant durable memory below. "
    "Prefer current user instructions and repository state over stale memory. "
    f"Memory index file: {path}\n\n"
)
limit = 9500
payload = prefix + text
if len(payload) > limit:
    payload = payload[: limit - len("\n[Codux memory refresh truncated]")] + "\n[Codux memory refresh truncated]"
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": os.environ.get("CLAUDE_HOOK_EVENT_NAME") or "UserPromptSubmit",
        "additionalContext": payload,
    },
    "suppressOutput": True,
}, ensure_ascii=False, separators=(",", ":")))
PY
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
    return 0
  fi

  case "${tool_name}" in
    codewhale|codewhale-tui|deepseek|deepseek-tui)
      codewhale_default_model
      ;;
  esac
}

codewhale_default_model() {
  local settings_file="${HOME}/.codewhale/settings.toml"
  [[ -f "${settings_file}" ]] || return 0
  awk -F '=' '
    /^[[:space:]]*default_text_model[[:space:]]*=/ {
      value=$2
      sub(/^[[:space:]]*/, "", value)
      sub(/[[:space:]]*$/, "", value)
      gsub(/^"|"$/, "", value)
      if (value != "") {
        print value
        exit
      }
    }
  ' "${settings_file}" 2>/dev/null
}

configured_permission_mode() {
  [[ -n "${DMUX_TOOL_PERMISSION_SETTINGS_FILE:-}" && -f "${DMUX_TOOL_PERMISSION_SETTINGS_FILE}" ]] || return 0

  local config_key=""
  case "${tool_name}" in
    codex)
      config_key="codex"
      ;;
    claude|claude-code)
      config_key="claudeCode"
      ;;
    gemini|agy)
      config_key="gemini"
      ;;
    opencode)
      config_key="opencode"
      ;;
    mimo)
      config_key="mimo"
      ;;
    kiro)
      config_key="kiro"
      ;;
    *)
      return 0
      ;;
  esac

  CONFIG_PATH="${DMUX_TOOL_PERMISSION_SETTINGS_FILE}" CONFIG_KEY="${config_key}" /usr/bin/python3 - <<'PY'
import json
import os

path = os.environ.get("CONFIG_PATH", "")
key = os.environ.get("CONFIG_KEY", "")
if not path or not key:
    raise SystemExit(0)

try:
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except Exception:
    raise SystemExit(0)

value = payload.get(key)
if isinstance(value, str) and value:
    print(value)
PY
}

has_full_access_permission() {
  [[ "$(configured_permission_mode)" == "fullAccess" ]]
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

send_runtime_event() {
  local payload="$1"
  [[ -n "${DMUX_RUNTIME_EVENT_DIR:-}" ]] || {
    log_line "hook skip action=${action} tool=${tool_name} reason=no-runtime-event-dir"
    return 0
  }
  /bin/mkdir -p -- "${DMUX_RUNTIME_EVENT_DIR}"
  local name
  name="$(printf '%s-%s.json' "$(/bin/date +%s%3N 2>/dev/null || /bin/date +%s)" "$(/usr/bin/uuidgen 2>/dev/null | /usr/bin/tr '[:upper:]' '[:lower:]')")"
  local path="${DMUX_RUNTIME_EVENT_DIR}/${name}"
  local tmp="${path}.tmp"
  if print -rn -- "${payload}" >| "${tmp}" && /bin/mv -f -- "${tmp}" "${path}"; then
    :
  else
    /bin/rm -f -- "${tmp}" 2>/dev/null || true
    log_line "hook write failed action=${action} tool=${tool_name} dir=${DMUX_RUNTIME_EVENT_DIR}"
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
  local cwd_value="${9:-}"
  local target_tool_name="${10:-}"
  local message_value="${11:-}"
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
      if [[ -n "${cwd_value}" ]]; then
        print -rn -- ",\"cwd\":\"$(json_escape "${cwd_value}")\""
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
      "$(extract_first_hook_field source)" \
      "" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)"
    exit 0
    ;;
  codex-prompt-submit)
    write_ai_hook_event \
      "promptSubmitted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "" \
      "user-input" \
      "" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)"
    exit 0
    ;;
  codex-pre-tool-use)
    write_ai_hook_event \
      "promptSubmitted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "" \
      "tool-use" \
      "" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)" \
      "$(extract_first_hook_field tool_name toolName tool)"
    exit 0
    ;;
  codex-post-tool-use)
    write_ai_hook_event \
      "promptSubmitted" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "" \
      "tool-use" \
      "" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)" \
      "$(extract_first_hook_field tool_name toolName tool)"
    exit 0
    ;;
  codex-permission-request)
    write_ai_hook_event \
      "needsInput" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "null" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "permission-request" \
      "" \
      "permission-request" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)" \
      "$(extract_first_hook_field tool_name toolName tool)" \
      "$(extract_first_hook_field message prompt reason)"
    exit 0
    ;;
  codex-stop)
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
      "$(extract_first_hook_field stop_reason reason)" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)"
    exit 0
    ;;
  codex-session-end)
    write_ai_hook_event \
      "sessionEnded" \
      "$(extract_hook_session_id)" \
      "$(resolved_hook_model)" \
      "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
      "$(extract_first_hook_field transcript_path transcriptPath)" \
      "" \
      "" \
      "$(extract_first_hook_field reason stop_reason)" \
      "$(extract_first_hook_field cwd current_working_directory working_directory)"
    exit 0
    ;;
esac

if [[ "${tool_name}" == "claude" || "${tool_name}" == "claude-code" ]]; then
  case "${action}" in
    session-start)
      CLAUDE_HOOK_EVENT_NAME="SessionStart"
      claude_total_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
      [[ -z "${claude_total_tokens}" ]] && claude_total_tokens="null"
      claude_session_source="$(extract_first_hook_field source)"
      write_ai_hook_event \
        "sessionStarted" \
        "$(resolved_claude_external_session_id)" \
        "$(resolved_hook_model)" \
        "${claude_total_tokens}" \
        "" \
        "" \
        "${claude_session_source}"
      write_claude_session_map
      if [[ "${claude_session_source}" == "compact" ]]; then
        should_emit_claude_memory_context=true
      else
        CLAUDE_HOOK_EVENT_NAME=""
      fi
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
        if has_full_access_permission; then
          write_ai_hook_event \
            "promptSubmitted" \
            "$(resolved_claude_external_session_id)" \
            "$(resolved_hook_model)" \
            "null" \
            "" \
            "" \
            "permission-auto-allowed"
        else
          write_ai_hook_event \
            "needsInput" \
            "$(resolved_claude_external_session_id)" \
            "$(resolved_hook_model)" \
            "null" \
            "" \
            "permission-request" \
            "" \
            "permission-request" \
            "" \
            "$(extract_first_hook_field tool_name toolName tool)" \
            "$(extract_first_hook_field message prompt)"
        fi
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
          "" \
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
    pre-compact|post-compact)
      CLAUDE_HOOK_EVENT_NAME="$([[ "${action}" == "pre-compact" ]] && print -r -- "PreCompact" || print -r -- "PostCompact")"
      write_claude_session_map
      write_ai_hook_event \
        "memoryRefreshing" \
        "$(resolved_claude_external_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "${action}" \
        "$(extract_first_hook_field trigger reason)"
      log_line "claude hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
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
      log_line "claude hook action=${action} session-end session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      clear_claude_session_map
      ;;
  esac
fi

if [[ "${tool_name}" == "gemini" || "${tool_name}" == "agy" ]]; then
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
          "" \
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

if [[ "${tool_name}" == "codewhale" || "${tool_name}" == "codewhale-tui" || "${tool_name}" == "deepseek" || "${tool_name}" == "deepseek-tui" ]]; then
  case "${action}" in
    codewhale-session-start)
      write_ai_hook_event \
        "sessionStarted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "$(extract_first_hook_field event source)" \
        "" \
        "$(extract_first_hook_field workspace cwd current_working_directory working_directory)"
      log_line "codewhale hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    codewhale-message-submit)
      codewhale_prompt_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
      [[ -z "${codewhale_prompt_tokens}" ]] && codewhale_prompt_tokens="null"
      write_ai_hook_event \
        "promptSubmitted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${codewhale_prompt_tokens}" \
        "" \
        "" \
        "user-input" \
        "" \
        "$(extract_first_hook_field workspace cwd current_working_directory working_directory)"
      log_line "codewhale hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    codewhale-tool-call-before|codewhale-tool-call-after)
      write_ai_hook_event \
        "promptSubmitted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "tool-use" \
        "" \
        "$(extract_first_hook_field workspace cwd current_working_directory working_directory)" \
        "$(extract_first_hook_field tool_name toolName tool name)"
      log_line "codewhale hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    codewhale-error)
      write_ai_hook_event \
        "turnCompleted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason error message)" \
        "$(extract_first_hook_field workspace cwd current_working_directory working_directory)"
      log_line "codewhale hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    codewhale-session-end)
      write_ai_hook_event \
        "sessionEnded" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "$(extract_hook_number_field total_tokens totalTokenCount totalTokens)" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason stop_reason)" \
        "$(extract_first_hook_field workspace cwd current_working_directory working_directory)"
      log_line "codewhale hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
  esac
fi

if [[ "${tool_name}" == "kimi" ]]; then
  kimi_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
  [[ -z "${kimi_tokens}" ]] && kimi_tokens="null"
  case "${action}" in
    session-start)
      write_ai_hook_event \
        "sessionStarted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "$(extract_first_hook_field source)"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    prompt-submit)
      write_ai_hook_event \
        "promptSubmitted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "user-input"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    before-agent|after-agent)
      write_ai_hook_event \
        "promptSubmitted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "tool-use" \
        "" \
        "" \
        "$(extract_first_hook_field tool_name toolName tool)"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    permission-request)
      if has_full_access_permission; then
        write_ai_hook_event \
          "promptSubmitted" \
          "$(extract_hook_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "" \
          "permission-auto-allowed"
      else
        write_ai_hook_event \
          "needsInput" \
          "$(extract_hook_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "permission-request" \
          "" \
          "permission-request" \
          "" \
          "$(extract_first_hook_field tool_name toolName tool)" \
          "$(extract_first_hook_field message prompt)"
      fi
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    notification)
      kimi_notification_type="$(extract_hook_notification_type)"
      if [[ -n "${kimi_notification_type}" ]]; then
        write_ai_hook_event \
          "needsInput" \
          "$(extract_hook_session_id)" \
          "$(resolved_hook_model)" \
          "null" \
          "" \
          "${kimi_notification_type}" \
          "" \
          "${kimi_notification_type}" \
          "" \
          "$(extract_first_hook_field tool_name toolName tool)" \
          "$(extract_first_hook_field message)"
      fi
      ;;
    pre-compact|post-compact)
      write_ai_hook_event \
        "memoryRefreshing" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "${action}"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    stop|stop-failure)
      write_ai_hook_event \
        "turnCompleted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field stop_reason reason)"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    session-end)
      write_ai_hook_event \
        "sessionEnded" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kimi_tokens}" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason)"
      log_line "kimi hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
  esac
fi

if [[ "${tool_name}" == "kiro" || "${tool_name}" == "kiro-cli" ]]; then
  kiro_tokens="$(extract_hook_number_field total_tokens totalTokenCount totalTokens)"
  [[ -z "${kiro_tokens}" ]] && kiro_tokens="null"
  case "${action}" in
    session-start)
      write_ai_hook_event \
        "sessionStarted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kiro_tokens}" \
        "" \
        "" \
        "$(extract_first_hook_field source)" \
        "" \
        "$(extract_first_hook_field cwd current_working_directory working_directory)"
      log_line "kiro hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    prompt-submit)
      write_ai_hook_event \
        "promptSubmitted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kiro_tokens}" \
        "" \
        "" \
        "user-input" \
        "" \
        "$(extract_first_hook_field cwd current_working_directory working_directory)"
      log_line "kiro hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
    stop)
      write_ai_hook_event \
        "turnCompleted" \
        "$(extract_hook_session_id)" \
        "$(resolved_hook_model)" \
        "${kiro_tokens}" \
        "" \
        "" \
        "" \
        "$(extract_first_hook_field reason stop_reason)" \
        "$(extract_first_hook_field cwd current_working_directory working_directory)"
      log_line "kiro hook action=${action} session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-}"
      ;;
  esac
fi

if [[ "${should_emit_claude_memory_context}" == true ]]; then
  claude_memory_additional_context
fi
