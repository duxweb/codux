if [[ -n "${DMUX_AI_HOOK_INSTALLED:-}" ]]; then
  return 0
fi
export DMUX_AI_HOOK_INSTALLED=1

zmodload zsh/datetime 2>/dev/null || true
autoload -Uz add-zsh-hook

typeset -g DMUX_ACTIVE_AI_TOOL=""
typeset -g DMUX_ACTIVE_AI_STARTED_AT=""
typeset -g DMUX_ACTIVE_AI_INVOCATION_ID=""
export DMUX_ACTIVE_AI_TOOL
export DMUX_ACTIVE_AI_STARTED_AT
export DMUX_ACTIVE_AI_INVOCATION_ID

_dmux_log_line() {
  [[ -n "${DMUX_LOG_FILE:-}" ]] || return 0
  /bin/mkdir -p -- "${DMUX_LOG_FILE:h}"
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [zsh-hook] $1" >> "${DMUX_LOG_FILE}"
}

_dmux_json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  print -rn -- "$value"
}

_dmux_now() {
  if [[ -n "${EPOCHREALTIME:-}" ]]; then
    printf "%.3f" "${EPOCHREALTIME}"
  else
    printf "%.3f" "${EPOCHSECONDS:-0}"
  fi
}

_dmux_new_invocation_id() {
  uuidgen | tr '[:upper:]' '[:lower:]'
}

_dmux_status_path() {
  [[ -n "${DMUX_STATUS_DIR:-}" && -n "${DMUX_PROJECT_ID:-}" ]] || return 1
  print -r -- "${DMUX_STATUS_DIR}/${DMUX_PROJECT_ID}.json"
}

_dmux_send_runtime_event() {
  local payload="$1"
  [[ -n "${DMUX_RUNTIME_SOCKET:-}" ]] || return 0
  command -v /usr/bin/nc >/dev/null 2>&1 || return 0
  print -r -- "${payload}" | /usr/bin/nc -U -w 1 "${DMUX_RUNTIME_SOCKET}" >/dev/null 2>&1 || true
}

_dmux_reset_terminal_input_modes() {
  [[ -t 1 ]] || return 0
  printf '\033[<u' || true
}

_dmux_prepend_wrapper_bin() {
  [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]] || return 0
  typeset -gaU path
  path=("${DMUX_WRAPPER_BIN}" ${path:#"${DMUX_WRAPPER_BIN}"})
  export PATH
}

_dmux_write_usage_event() {
  local phase="${1:-running}"
  local payload=""
  local now
  now="$(_dmux_now)"
  payload="$(
    {
      print -rn -- '{"kind":"usage","payload":{'
      print -rn -- "\"sessionId\":\"$(_dmux_json_escape "${DMUX_SESSION_ID}")\","
      print -rn -- "\"sessionInstanceId\":\"$(_dmux_json_escape "${DMUX_SESSION_INSTANCE_ID:-}")\","
      if [[ -n "${DMUX_ACTIVE_AI_INVOCATION_ID:-}" ]]; then
        print -rn -- "\"invocationId\":\"$(_dmux_json_escape "${DMUX_ACTIVE_AI_INVOCATION_ID}")\","
      else
        print -rn -- "\"invocationId\":null,"
      fi
      print -rn -- "\"projectId\":\"$(_dmux_json_escape "${DMUX_PROJECT_ID}")\","
      print -rn -- "\"projectName\":\"$(_dmux_json_escape "${DMUX_PROJECT_NAME:-Workspace}")\","
      print -rn -- "\"projectPath\":\"$(_dmux_json_escape "${DMUX_PROJECT_PATH:-}")\","
      print -rn -- "\"sessionTitle\":\"$(_dmux_json_escape "${DMUX_SESSION_TITLE:-Terminal}")\","
      print -rn -- "\"tool\":\"$(_dmux_json_escape "${DMUX_ACTIVE_AI_TOOL}")\","
      print -rn -- "\"model\":null,"
      print -rn -- "\"status\":\"$(_dmux_json_escape "${phase}")\","
      if [[ "${phase}" == "running" && ( "${DMUX_ACTIVE_AI_TOOL}" == "codex" || "${DMUX_ACTIVE_AI_TOOL}" == "claude" || "${DMUX_ACTIVE_AI_TOOL}" == "claude-code" || "${DMUX_ACTIVE_AI_TOOL}" == "gemini" ) ]]; then
        print -rn -- "\"responseState\":null,"
      else
        print -rn -- "\"responseState\":\"idle\","
      fi
      print -rn -- "\"updatedAt\":${now},"
      print -rn -- "\"startedAt\":${DMUX_ACTIVE_AI_STARTED_AT:-$now},"
      if [[ "${phase}" == "running" ]]; then
        print -rn -- "\"finishedAt\":null,"
      else
        print -rn -- "\"finishedAt\":${now},"
      fi
      print -rn -- "\"inputTokens\":0,"
      print -rn -- "\"outputTokens\":0,"
      print -rn -- "\"totalTokens\":0,"
      print -rn -- "\"contextWindow\":null,"
      print -rn -- "\"contextUsedTokens\":null,"
      print -rn -- "\"contextUsagePercent\":null"
      print -rn -- '}}'
    }
  )"
  _dmux_send_runtime_event "${payload}"
}

_dmux_clear_status() {
  local path
  path="$(_dmux_status_path)" || return 0
  /bin/rm -f -- "${path}"
}

_dmux_resolve_tool_from_command() {
  local command_line="$1"
  local -a words
  words=(${(z)command_line})
  local index=1

  while (( index <= ${#words} )); do
    local candidate="${words[index]}"
    if [[ "${candidate}" == [A-Za-z_][A-Za-z0-9_]*=* ]]; then
      (( index++ ))
      continue
    fi
    case "${candidate}" in
      env|command|builtin|noglob|nocorrect|time|nohup)
        (( index++ ))
        continue
        ;;
    esac
    candidate="${candidate:t}"
    case "${candidate}" in
      codex|claude|claude-code|opencode|gemini)
        print -r -- "${candidate}"
        return 0
        ;;
    esac
    break
  done
  return 1
}

_dmux_ai_preexec() {
  local tool
  tool="$(_dmux_resolve_tool_from_command "$1")" || return 0
  _dmux_prepend_wrapper_bin
  DMUX_ACTIVE_AI_TOOL="${tool}"
  DMUX_ACTIVE_AI_STARTED_AT="$(_dmux_now)"
  DMUX_ACTIVE_AI_INVOCATION_ID="$(_dmux_new_invocation_id)"
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
  local resolved_path=""
  resolved_path="$(whence -p "${tool}" 2>/dev/null || true)"
  _dmux_log_line "preexec tool=${tool} resolved=${resolved_path:-nil} wrapper=${DMUX_WRAPPER_BIN:-nil} session=${DMUX_SESSION_ID:-nil} invocation=${DMUX_ACTIVE_AI_INVOCATION_ID:-nil}"
  _dmux_clear_status
  _dmux_write_usage_event running
}

_dmux_ai_precmd() {
  local exit_code=$?
  [[ -n "${DMUX_ACTIVE_AI_TOOL}" ]] || return 0
  _dmux_reset_terminal_input_modes
  _dmux_write_usage_event completed "${exit_code}"
  _dmux_clear_status
  DMUX_ACTIVE_AI_TOOL=""
  DMUX_ACTIVE_AI_STARTED_AT=""
  DMUX_ACTIVE_AI_INVOCATION_ID=""
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
}

_dmux_ai_zshexit() {
  if [[ -n "${DMUX_ACTIVE_AI_TOOL}" ]]; then
    _dmux_reset_terminal_input_modes
    _dmux_write_usage_event completed "$?"
    _dmux_clear_status
  fi
  DMUX_ACTIVE_AI_TOOL=""
  DMUX_ACTIVE_AI_STARTED_AT=""
  DMUX_ACTIVE_AI_INVOCATION_ID=""
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
}

add-zsh-hook preexec _dmux_ai_preexec
add-zsh-hook precmd _dmux_ai_precmd
add-zsh-hook zshexit _dmux_ai_zshexit

_dmux_prepend_wrapper_bin
_dmux_reset_terminal_input_modes
_dmux_log_line "loaded session=${DMUX_SESSION_ID:-nil} wrapper=${DMUX_WRAPPER_BIN:-nil} claude=$(whence -p claude 2>/dev/null || print -r -- nil) codex=$(whence -p codex 2>/dev/null || print -r -- nil)"
