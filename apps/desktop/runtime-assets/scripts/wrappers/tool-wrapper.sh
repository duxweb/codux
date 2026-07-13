#!/bin/zsh
set -uo pipefail

zmodload zsh/datetime 2>/dev/null || true

tool_name="$1"
shift
wrapper_dir="$(cd "$(dirname "$0")" && pwd)"
wrapper_bin_dir="${wrapper_dir}/bin"
wrapper_helper="${wrapper_dir}/codux-wrapper-helper"
current_path="${PATH:-}"
orig_path="${DMUX_ORIGINAL_PATH:-}"
search_path=""
runtime_path=""

is_wrapper_bin_dir() {
  local dir="$1"
  [[ -n "$dir" ]] || return 1
  local normalized="${dir:A}"
  [[ "$normalized" == "${wrapper_bin_dir:A}" ]] && return 0
  [[ "$normalized" == */Contents/Resources/runtime-root/scripts/wrappers/bin ]]
  [[ "$normalized" == */runtime-root/scripts/wrappers/bin ]]
}

filtered_tool_search_path() {
  local source_path="${1:-}"
  local -a path_parts filtered_parts
  path_parts=(${(s/:/)source_path})
  local dir
  for dir in "${path_parts[@]}"; do
    [[ -n "$dir" ]] || continue
    is_wrapper_bin_dir "$dir" && continue
    filtered_parts+=("$dir")
  done
  print -r -- "${(j/:/)filtered_parts}"
}

if [[ -n "$current_path" ]]; then
  search_path="$(filtered_tool_search_path "$current_path")"
fi

if [[ -z "$search_path" ]]; then
  search_path="$(filtered_tool_search_path "$orig_path")"
fi

if [[ -n "$search_path" ]]; then
  runtime_path="${wrapper_bin_dir}:${search_path}"
else
  runtime_path="${wrapper_bin_dir}"
fi

system_bin_prefix="/usr/bin:/bin:/usr/sbin:/sbin"
managed_system_first_path="${system_bin_prefix}${runtime_path:+:${runtime_path}}"

resolve_from_search_path() {
  local binary_name="$1"
  local resolved=""
  resolved="$(PATH="$search_path" whence -p "$binary_name" 2>/dev/null || true)"
  if [[ -n "$resolved" && -x "$resolved" ]] && ! is_wrapper_bin_dir "${resolved:h}"; then
    print -r -- "$resolved"
    return 0
  fi
  return 1
}

apply_process_limit_cap() {
  local maxproc="${1:-}"
  [[ -n "$maxproc" && "$maxproc" == <-> ]] || return 0

  local current_limit
  current_limit="$(ulimit -u 2>/dev/null || true)"
  if [[ "$current_limit" == "unlimited" || ( "$current_limit" == <-> && "$current_limit" -gt "$maxproc" ) ]]; then
    ulimit -u "$maxproc" 2>/dev/null || true
  fi
}

find_real_binary() {
  local active_resolved_path="${DMUX_ACTIVE_AI_RESOLVED_PATH:-}"
  local active_resolved_name="${active_resolved_path:t}"
  if [[ -n "${DMUX_ACTIVE_AI_RESOLVED_PATH:-}" \
    && -x "${active_resolved_path}" \
    && "${active_resolved_name}" == "$tool_name" ]] \
    && ! is_wrapper_bin_dir "${active_resolved_path:h}"; then
    print -r -- "${active_resolved_path}"
    return 0
  fi

  local -a candidate_names=()
  case "$tool_name" in
    claude)
      candidate_names=("claude" "claude-code" "reclaude")
      ;;
    claude-code)
      candidate_names=("claude-code" "claude" "reclaude")
      ;;
    reclaude)
      candidate_names=("reclaude" "claude" "claude-code")
      ;;
    *)
      candidate_names=("$tool_name")
      ;;
  esac

  local binary_name=""
  local resolved=""
  for binary_name in "${candidate_names[@]}"; do
    resolved="$(resolve_from_search_path "$binary_name" || true)"
    if [[ -n "$resolved" ]]; then
      print -r -- "$resolved"
      return 0
    fi
  done

  if [[ "$tool_name" == "claude" || "$tool_name" == "claude-code" || "$tool_name" == "reclaude" ]]; then
    local claude_code_root="${HOME}/Library/Application Support/Claude/claude-code"
    local -a bundle_candidates
    bundle_candidates=("${claude_code_root}"/*/claude.app/Contents/MacOS/claude(N))
    if [[ ${#bundle_candidates[@]} -gt 0 ]]; then
      local candidate="${bundle_candidates[-1]}"
      if [[ -x "$candidate" ]]; then
        print -r -- "$candidate"
        return 0
      fi
    fi
  fi

  return 1
}

real_bin="$(find_real_binary || true)"
if [[ -z "$real_bin" ]]; then
  print -u2 -- "wrapper: failed to locate real binary for $tool_name"
  if [[ -x "${wrapper_dir}/dmux-ai-state.sh" && -n "${DMUX_SESSION_ID:-}" ]]; then
    "${wrapper_dir}/dmux-ai-state.sh" stop "${DMUX_RUNTIME_OWNER:-}" "$tool_name" </dev/null >/dev/null 2>&1 || true
  fi
  exit 127
fi

# Seed the console's reported default colors (OSC 10/11 set) with the app
# theme: on Windows ConPTY answers color queries itself from its own black
# palette, so TUIs would detect a dark background under a light theme.
if [[ -t 1 ]]; then
  [[ -n "${DMUX_TERMINAL_OSC_FG:-}" ]] && printf '\033]10;%s\033\\' "${DMUX_TERMINAL_OSC_FG}"
  [[ -n "${DMUX_TERMINAL_OSC_BG:-}" ]] && printf '\033]11;%s\033\\' "${DMUX_TERMINAL_OSC_BG}"
fi

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  print -rn -- "$value"
}

runtime_now() {
  if [[ -n "${EPOCHREALTIME:-}" ]]; then
    LC_ALL=C printf "%.3f" "${EPOCHREALTIME/,/.}"
  elif [[ -n "${EPOCHSECONDS:-}" ]]; then
    LC_ALL=C printf "%.3f" "${EPOCHSECONDS/,/.}"
  else
    /bin/date +%s | LC_ALL=C /usr/bin/awk '{ printf "%.3f", $1 }'
  fi
}

log_line() {
  [[ -n "${DMUX_LOG_FILE:-}" ]] || return 0
  /bin/mkdir -p -- "${DMUX_LOG_FILE:h}"
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [wrapper] $1" >> "${DMUX_LOG_FILE}"
}

restore_working_directory() {
  if [[ "${PWD:-}" == /* && . -ef "${PWD}" ]]; then
    return 0
  fi
  local target="${DMUX_SESSION_CWD:-}"
  if [[ -z "${target}" || ! -d "${target}" ]]; then
    print -u2 -r -- "Codux: working directory is unavailable. Reconnect the project disk and try again."
    log_line "launch blocked: working directory unavailable target=${target:-none}"
    return 72
  fi
  if ! builtin cd -- "${target}" 2>/dev/null; then
    print -u2 -r -- "Codux: unable to restore working directory: ${target}"
    log_line "launch blocked: working directory restore failed target=${target}"
    return 72
  fi
  print -u2 -r -- "Codux: restored working directory: ${target}"
  log_line "restored working directory path=${target}"
}

is_passthrough_invocation() {
  local first="${1:-}"
  case "${first}" in
    --help|-h|help|--version|-V|version|features|--features|auth|login|logout|doctor|update|upgrade|config|info|export|mcp|plugin|vis|web|term|acp|app-server|__background-task-worker|__web-worker)
      return 0
      ;;
  esac
  return 1
}

wrapper_helper_available() {
  if [[ -x "${wrapper_helper}" ]]; then
    return 0
  fi
  log_line "wrapper helper missing path=${wrapper_helper}"
  return 1
}

run_wrapper_helper() {
  wrapper_helper_available || return 1
  "${wrapper_helper}" --codux-wrapper-helper "$@" 2>/dev/null
}

json_string_key_fallback() {
  local config_path="$1"
  local config_key="$2"
  [[ -f "${config_path}" && -n "${config_key}" ]] || return 0
  if command -v python3 >/dev/null 2>&1; then
    python3 - "${config_path}" "${config_key}" <<'PY'
import json
import sys
path, key = sys.argv[1], sys.argv[2]
try:
    with open(path, "r", encoding="utf-8") as handle:
        value = json.load(handle).get(key, "")
except Exception:
    value = ""
if isinstance(value, str):
    value = value.strip()
    if value:
        print(value)
PY
    return 0
  fi
  /usr/bin/sed -n 's/.*"'"${config_key}"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${config_path}" | /usr/bin/head -n 1
}

configured_json_string_key() {
  local config_path="$1"
  local config_key="$2"
  local value=""
  if wrapper_helper_available; then
    value="$(CONFIG_PATH="${config_path}" CONFIG_KEY="${config_key}" run_wrapper_helper json-string-key || true)"
  fi
  if [[ -z "${value}" ]]; then
    value="$(json_string_key_fallback "${config_path}" "${config_key}" || true)"
  fi
  [[ -n "${value}" ]] && print -r -- "${value}"
}

memory_prompt_file() {
  [[ -n "${DMUX_AI_MEMORY_PROMPT_FILE:-}" && -f "${DMUX_AI_MEMORY_PROMPT_FILE}" ]] || return 1
  print -r -- "${DMUX_AI_MEMORY_PROMPT_FILE}"
}

tool_memory_injection_strategy() {
  local config_path="${wrapper_dir}/tool-drivers.json"
  [[ -f "${config_path}" ]] || return 0
  TOOL_NAME="${tool_name}" CONFIG_PATH="${config_path}" run_wrapper_helper tool-memory-injection || true
}

tool_uses_memory_injection() {
  [[ "${memory_injection_strategy:-}" == "${1:-}" ]]
}

tool_permission_settings_path() {
  [[ -n "${DMUX_TOOL_PERMISSION_SETTINGS_FILE:-}" ]] || return 0
  print -r -- "${DMUX_TOOL_PERMISSION_SETTINGS_FILE}"
}

configured_permission_mode() {
  local config_path
  config_path="$(tool_permission_settings_path)"
  [[ -f "${config_path}" ]] || return 0

  local config_key=""
  case "${tool_name}" in
    codex)
      config_key="codex"
      ;;
    claude|claude-code|reclaude)
      config_key="claudeCode"
      ;;
    agy)
      config_key="agy"
      ;;
    kimi|kimi-code)
      config_key="kimi"
      ;;
    opencode)
      config_key="opencode"
      ;;
    mimo)
      config_key="mimo"
      ;;
    kiro-cli)
      config_key="kiro"
      ;;
    codewhale)
      config_key="codewhale"
      ;;
    *)
      return 0
      ;;
  esac

  configured_json_string_key "${config_path}" "${config_key}"
}


configured_tool_model() {
  local config_path
  config_path="$(tool_permission_settings_path)"
  [[ -f "${config_path}" ]] || return 0

  local config_key=""
  case "${tool_name}" in
    codex)
      config_key="codexModel"
      ;;
    claude|claude-code|reclaude)
      config_key="claudeCodeModel"
      ;;
    agy)
      config_key="agyModel"
      ;;
    kimi|kimi-code)
      config_key="kimiModel"
      ;;
    opencode)
      config_key="opencodeModel"
      ;;
    mimo)
      config_key="mimoModel"
      ;;
    kiro-cli)
      config_key="kiroModel"
      ;;
    codewhale)
      config_key="codewhaleModel"
      ;;
    *)
      return 0
      ;;
  esac

  configured_json_string_key "${config_path}" "${config_key}"
}

configured_codex_effort() {
  local config_path
  config_path="$(tool_permission_settings_path)"
  [[ -f "${config_path}" ]] || return 0

  local configured_effort
  if wrapper_helper_available; then
    configured_effort="$(CONFIG_PATH="${config_path}" run_wrapper_helper codex-effort || true)"
  fi
  if [[ -z "${configured_effort}" ]]; then
    configured_effort="$(json_string_key_fallback "${config_path}" "codexEffort" || true)"
  fi
  case "${configured_effort}" in
    minimal|low|medium|high|xhigh)
      print -r -- "${configured_effort}"
      ;;
  esac
}

apply_configured_model_arg() {
  local configured_model
  configured_model="$(configured_tool_model || true)"
  [[ -n "${configured_model}" ]] || return 0
  if has_exact_arg "--model" "${launch_args[@]}" \
    || has_exact_arg "-m" "${launch_args[@]}" \
    || has_prefix_arg "--model=" "${launch_args[@]}"; then
    return 0
  fi

  case "${tool_name}" in
    codex)
      launch_args=("--model=${configured_model}" "${launch_args[@]}")
      ;;
    claude|claude-code|reclaude|agy|kimi|kimi-code|opencode|mimo|codewhale)
      launch_args=(--model "${configured_model}" "${launch_args[@]}")
      ;;
  esac
}

has_config_key_arg() {
  local target="$1"
  shift
  local previous=""
  local arg
  for arg in "$@"; do
    if [[ "${previous}" == "-c" || "${previous}" == "--config" ]]; then
      [[ "${arg}" == "${target}="* ]] && return 0
    fi
    case "${arg}" in
      -c${target}=*|--config=${target}=*)
        return 0
        ;;
    esac
    previous="${arg}"
  done
  return 1
}

apply_configured_codex_effort_arg() {
  [[ "${tool_name}" == "codex" ]] || return 0
  local configured_effort
  configured_effort="$(configured_codex_effort || true)"
  [[ -n "${configured_effort}" ]] || return 0
  if has_config_key_arg "model_reasoning_effort" "${launch_args[@]}"; then
    return 0
  fi
  launch_args=(-c "model_reasoning_effort=\"${configured_effort}\"" "${launch_args[@]}")
}

codex_allows_additional_writable_roots() {
  local previous=""
  local arg
  for arg in "$@"; do
    if [[ "${previous}" == "--sandbox" || "${previous}" == "-s" ]]; then
      case "${arg}" in
        workspace-write|danger-full-access)
          return 0
          ;;
      esac
    fi
    case "${arg}" in
      --dangerously-bypass-approvals-and-sandbox|--full-auto)
        return 0
        ;;
      --sandbox=workspace-write|--sandbox=danger-full-access|-sworkspace-write|-sdanger-full-access)
        return 0
        ;;
    esac
    previous="${arg}"
  done
  return 1
}

codex_has_sandbox_mode_arg() {
  local arg
  for arg in "$@"; do
    case "${arg}" in
      --dangerously-bypass-approvals-and-sandbox|--full-auto|--sandbox|-s|--sandbox=*|-s*)
        return 0
        ;;
    esac
  done
  return 1
}

apply_default_codex_memory_sandbox_arg() {
  tool_uses_memory_injection "codexDeveloperInstructions" || return 0
  [[ -n "${DMUX_AI_MEMORY_WORKSPACE_ROOT:-}" && -d "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" ]] || return 0
  [[ -n "${DMUX_PROJECT_PATH:-}" && -d "${DMUX_PROJECT_PATH}" ]] || return 0
  codex_has_sandbox_mode_arg "${launch_args[@]}" && return 0
  launch_args=(--sandbox workspace-write "${launch_args[@]}")
}

apply_codex_memory_workspace_args() {
  tool_uses_memory_injection "codexDeveloperInstructions" || return 0
  [[ -n "${DMUX_AI_MEMORY_WORKSPACE_ROOT:-}" && -d "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" ]] || return 0
  [[ -n "${DMUX_PROJECT_PATH:-}" && -d "${DMUX_PROJECT_PATH}" ]] || return 0
  codex_allows_additional_writable_roots "${launch_args[@]}" || return 0
  if has_exact_arg "-C" "${launch_args[@]}" \
    || has_exact_arg "--cd" "${launch_args[@]}" \
    || has_prefix_arg "--cd=" "${launch_args[@]}"; then
    return 0
  fi
  launch_args=(-C "${DMUX_PROJECT_PATH}" --add-dir "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" "${launch_args[@]}")
}

codex_toml_string() {
  local value="$1"
  if wrapper_helper_available; then
    VALUE="${value}" run_wrapper_helper toml-string && return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    VALUE="${value}" python3 - <<'PY'
import json
import os
print(json.dumps(os.environ.get("VALUE", ""), ensure_ascii=False))
PY
    return 0
  fi
  return 1
}

apply_codex_memory_developer_instructions() {
  tool_uses_memory_injection "codexDeveloperInstructions" || return 0
  if [[ -z "${DMUX_AI_MEMORY_WORKSPACE_ROOT:-}" || ! -d "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" ]]; then
    log_line "codex instructions skipped: memory workspace missing"
    return 0
  fi
  local memory_agents="${DMUX_AI_MEMORY_WORKSPACE_ROOT}/AGENTS.md"
  if [[ ! -f "${memory_agents}" ]]; then
    log_line "codex instructions skipped: AGENTS.md missing path=${memory_agents}"
    return 0
  fi
  if has_config_key_arg "developer_instructions" "${launch_args[@]}"; then
    log_line "codex instructions skipped: developer_instructions already provided"
    return 0
  fi
  local memory_instructions
  memory_instructions="$(<"${memory_agents}")"
  if [[ -z "${memory_instructions}" ]]; then
    log_line "codex instructions skipped: AGENTS.md empty path=${memory_agents}"
    return 0
  fi
  launch_args=(-c "developer_instructions=$(codex_toml_string "${memory_instructions}")" "${launch_args[@]}")
  log_line "codex instructions injected path=${memory_agents} chars=${#memory_instructions}"
}

apply_kimi_memory_agent_file() {
  tool_uses_memory_injection "kimiAgentFile" || return 0
  case "${launch_args[1]:-}" in
    login|logout|info|export|mcp|plugin|vis|web|term|acp|__background-task-worker|__web-worker)
      log_line "kimi instructions skipped: subcommand=${launch_args[1]}"
      return 0
      ;;
  esac
  case "${launch_args[1]:-}" in
    --help|-h|--version|-V)
    log_line "kimi instructions skipped: metadata invocation"
    return 0
      ;;
  esac
  if has_exact_arg "--agent-file" "${launch_args[@]}" || has_prefix_arg "--agent-file=" "${launch_args[@]}" || has_exact_arg "--agent" "${launch_args[@]}" || has_prefix_arg "--agent=" "${launch_args[@]}"; then
    log_line "kimi instructions skipped: agent override already provided"
    return 0
  fi
  local prompt_file
  prompt_file="$(memory_prompt_file || true)"
  if [[ -z "${prompt_file}" ]]; then
    log_line "kimi instructions skipped: prompt file missing"
    return 0
  fi
  local prompt
  prompt="$(<"${prompt_file}")"
  if [[ -z "${prompt}" ]]; then
    log_line "kimi instructions skipped: prompt empty path=${prompt_file}"
    return 0
  fi
  local agent_key="${DMUX_SESSION_ID:-default}"
  agent_key="${agent_key//[^A-Za-z0-9_.-]/_}"
  local agent_dir="${wrapper_dir}/managed-kimi-agent/${agent_key}"
  local agent_file="${agent_dir}/agent.yaml"
  /bin/mkdir -p -- "${agent_dir}"
  {
    print -r -- "version: 1"
    print -r -- "agent:"
    print -r -- "  extend: default"
    print -r -- "  name: \"\""
    print -r -- "  system_prompt_args:"
    print -r -- "    ROLE_ADDITIONAL: |"
    local line
    while IFS= read -r line || [[ -n "$line" ]]; do
      print -r -- "      ${line}"
    done < "${prompt_file}"
  } >| "${agent_file}"
  launch_args=(--agent-file "${agent_file}" "${launch_args[@]}")
  log_line "kimi instructions injected path=${prompt_file} agent=${agent_file} chars=${#prompt}"
}

apply_append_system_prompt_memory_instructions() {
  local strategy="$1"
  local label="$2"
  tool_uses_memory_injection "${strategy}" || return 0
  if has_exact_arg "--append-system-prompt" "${launch_args[@]}" || has_prefix_arg "--append-system-prompt=" "${launch_args[@]}"; then
    log_line "${label} instructions skipped: append-system-prompt already provided"
    return 0
  fi
  local prompt_file
  prompt_file="$(memory_prompt_file || true)"
  if [[ -z "${prompt_file}" ]]; then
    log_line "${label} instructions skipped: prompt file missing"
    return 0
  fi
  local prompt
  prompt="$(<"${prompt_file}")"
  if [[ -z "${prompt}" ]]; then
    log_line "${label} instructions skipped: prompt empty path=${prompt_file}"
    return 0
  fi
  launch_args=(--append-system-prompt "${prompt}" "${launch_args[@]}")
  log_line "${label} instructions injected path=${prompt_file} chars=${#prompt}"
}

has_exact_arg() {
  local target="$1"
  shift
  local arg
  for arg in "$@"; do
    [[ "${arg}" == "${target}" ]] && return 0
  done
  return 1
}

has_prefix_arg() {
  local prefix="$1"
  shift
  local arg
  for arg in "$@"; do
    [[ "${arg}" == "${prefix}"* ]] && return 0
  done
  return 1
}

run_wrapped_command() {
  local external_session_id="${1:-}"
  local model="${2:-}"
  local launch_dir="${3:-}"
  shift 3

  if [[ -n "${launch_dir}" ]]; then
    (
      builtin cd "${launch_dir}" || exit 111
      "$@"
    )
  else
    "$@"
  fi
  local exit_code=$?
  if [[ ( "${tool_name}" == "opencode" || "${tool_name}" == "mimo" ) && -z "${external_session_id}" && -n "${DMUX_OPENCODE_SESSION_MAP_DIR:-}" && -n "${DMUX_SESSION_ID:-}" ]]; then
    local opencode_state_path="${DMUX_OPENCODE_SESSION_MAP_DIR}/opencode-session-${DMUX_SESSION_ID}.json"
    if [[ -f "${opencode_state_path}" ]]; then
      local resolved_state
      if wrapper_helper_available; then
        resolved_state="$(OPENCODE_STATE_PATH="${opencode_state_path}" "${wrapper_helper}" --codux-wrapper-helper opencode-session-state)"
      else
        resolved_state=""
      fi
      if [[ -n "${resolved_state}" ]]; then
        local resolved_lines
        resolved_lines=(${(f)resolved_state})
        if [[ ${#resolved_lines[@]} -ge 1 ]]; then
          external_session_id="${resolved_lines[1]}"
        fi
        if [[ ${#resolved_lines[@]} -ge 2 ]]; then
          model="${resolved_lines[2]}"
        fi
      fi
      rm -f -- "${opencode_state_path}"
    fi
  fi
  return "${exit_code}"
}

emit_wrapper_session_end() {
  [[ -x "${wrapper_dir}/dmux-ai-state.sh" ]] || return 0
  [[ -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_EVENT_DIR:-}" ]] || return 0
  "${wrapper_dir}/dmux-ai-state.sh" session-end "${DMUX_RUNTIME_OWNER:-}" "$tool_name" </dev/null >/dev/null 2>&1 || true
}

run_wrapped_ai_command() {
  apply_managed_lifecycle_env
  run_wrapped_command "$@"
  local exit_code=$?
  emit_wrapper_session_end
  return "${exit_code}"
}

extract_resume_target() {
  local previous=""
  for arg in "$@"; do
    case "${previous}" in
      --resume|-r|--resume-id)
        [[ -n "$arg" && "$arg" != -* ]] && print -r -- "$arg"
        return 0
        ;;
      --session|--session-id)
        [[ -n "$arg" && "$arg" != -* ]] && print -r -- "$arg"
        return 0
        ;;
      resume)
        [[ -n "$arg" && "$arg" != -* ]] && print -r -- "$arg"
        return 0
        ;;
    esac
    case "$arg" in
      --resume=*)
        print -r -- "${arg#--resume=}"
        return 0
        ;;
      --session=*)
        print -r -- "${arg#--session=}"
        return 0
        ;;
      --session-id=*)
        print -r -- "${arg#--session-id=}"
        return 0
        ;;
      --resume-id=*)
        print -r -- "${arg#--resume-id=}"
        return 0
        ;;
    esac
    previous="$arg"
  done
  return 1
}

extract_model_target() {
  local previous=""
  for arg in "$@"; do
    case "${previous}" in
      --model|-m)
        [[ -n "$arg" && "$arg" != -* ]] && print -r -- "$arg"
        return 0
        ;;
    esac
    case "$arg" in
      --model=*)
        print -r -- "${arg#--model=}"
        return 0
        ;;
    esac
    previous="$arg"
  done
  return 1
}

write_claude_session_map() {
  [[ -n "${DMUX_CLAUDE_SESSION_MAP_DIR:-}" && -n "${DMUX_SESSION_ID:-}" && -n "${1:-}" ]] || return 0
  local external_session_id="$1"
  local path="${DMUX_CLAUDE_SESSION_MAP_DIR}/${DMUX_SESSION_ID}.json"
  local tmp="${path}.tmp"
  /bin/mkdir -p -- "${DMUX_CLAUDE_SESSION_MAP_DIR}"
  {
    print -rn -- '{'
    print -rn -- "\"sessionId\":\"$(json_escape "${DMUX_SESSION_ID}")\","
    print -rn -- "\"externalSessionID\":\"$(json_escape "${external_session_id}")\","
    print -rn -- "\"updatedAt\":$(/bin/date +%s)"
    print -r -- '}'
  } >| "${tmp}"
  /bin/mv -f -- "${tmp}" "${path}"
}

write_runtime_binding() {
  [[ -n "${DMUX_AI_RUNTIME_BINDING_DIR:-}" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_PROJECT_ID:-}" && -n "${tool_name:-}" ]] || return 0
  local external_session_id="${1:-}"
  local model_value="${2:-}"
  local transcript_path="${3:-}"
  local session_origin="${4:-}"
  local binding_id="${DMUX_SESSION_INSTANCE_ID:-${DMUX_SESSION_ID}}-${tool_name}"
  local path="${DMUX_AI_RUNTIME_BINDING_DIR}/${DMUX_SESSION_ID}-${tool_name}.json"
  local tmp="${path}.tmp"
  local timestamp
  timestamp="$(runtime_now)"
  /bin/mkdir -p -- "${DMUX_AI_RUNTIME_BINDING_DIR}"
  {
    print -rn -- '{'
    print -rn -- "\"runtimeBindingId\":\"$(json_escape "${binding_id}")\","
    print -rn -- "\"terminalId\":\"$(json_escape "${DMUX_SESSION_ID}")\","
    if [[ -n "${DMUX_SESSION_INSTANCE_ID:-}" ]]; then
      print -rn -- "\"terminalInstanceId\":\"$(json_escape "${DMUX_SESSION_INSTANCE_ID}")\","
    else
      print -rn -- "\"terminalInstanceId\":null,"
    fi
    print -rn -- "\"tool\":\"$(json_escape "${tool_name}")\","
    print -rn -- "\"projectId\":\"$(json_escape "${DMUX_PROJECT_ID}")\","
    print -rn -- "\"projectName\":\"$(json_escape "${DMUX_PROJECT_NAME:-Workspace}")\","
    print -rn -- "\"projectPath\":\"$(json_escape "${DMUX_PROJECT_PATH:-}")\","
    print -rn -- "\"sessionTitle\":\"$(json_escape "${DMUX_SESSION_TITLE:-Terminal}")\","
    print -rn -- "\"launchStartedAt\":${timestamp},"
    if [[ -n "${external_session_id}" ]]; then
      print -rn -- "\"externalSessionId\":\"$(json_escape "${external_session_id}")\","
    else
      print -rn -- "\"externalSessionId\":null,"
    fi
    if [[ -n "${transcript_path}" ]]; then
      print -rn -- "\"transcriptPath\":\"$(json_escape "${transcript_path}")\","
    else
      print -rn -- "\"transcriptPath\":null,"
    fi
    if [[ -n "${model_value}" ]]; then
      print -rn -- "\"model\":\"$(json_escape "${model_value}")\","
    else
      print -rn -- "\"model\":null,"
    fi
    if [[ -n "${session_origin}" ]]; then
      print -rn -- "\"sessionOrigin\":\"$(json_escape "${session_origin}")\","
    else
      print -rn -- "\"sessionOrigin\":null,"
    fi
    print -rn -- "\"updatedAt\":${timestamp}"
    print -r -- '}'
  } >| "${tmp}" && /bin/mv -f -- "${tmp}" "${path}" || /bin/rm -f -- "${tmp}" 2>/dev/null || true
}

runtime_session_origin_for_resume() {
  [[ -n "${1:-}" ]] || return 0
  print -r -- "restored"
}

restore_working_directory || exit $?

apply_managed_lifecycle_env() {
  local env_path="${wrapper_dir}/managed-env/${tool_name}.env"
  [[ -f "${env_path}" ]] || return 0
  source "${env_path}"
  log_line "managed lifecycle env tool=${tool_name} path=${env_path}"
}

if is_passthrough_invocation "$@"; then
  exec env PATH="$runtime_path" "$real_bin" "$@"
fi

memory_injection_strategy="$(tool_memory_injection_strategy || true)"

if [[ "$tool_name" == "claude" || "$tool_name" == "claude-code" || "$tool_name" == "reclaude" ]]; then
  helper_script="${wrapper_dir}/dmux-ai-state.sh"
  if [[ -x "$helper_script" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_EVENT_DIR:-}" ]]; then
    local_permission_mode="$(configured_permission_mode || true)"
    claude_launch_path="${managed_system_first_path}"
    claude_maxproc="${DMUX_CLAUDE_MAXPROC:-2048}"
    apply_process_limit_cap "${claude_maxproc}"
    claude_memory_prompt_file="$(memory_prompt_file || true)"
    launch_args=("$@")
    apply_configured_model_arg
    if [[ "${local_permission_mode}" == "fullAccess" ]] \
      && ! has_exact_arg "--dangerously-skip-permissions" "${launch_args[@]}" \
      && ! has_exact_arg "--allow-dangerously-skip-permissions" "${launch_args[@]}" \
      && ! has_exact_arg "--permission-mode" "${launch_args[@]}" \
      && ! has_prefix_arg "--permission-mode=" "${launch_args[@]}"; then
      launch_args=(--dangerously-skip-permissions "${launch_args[@]}")
    fi
    if tool_uses_memory_injection "claudeAppendSystemPrompt" \
      && [[ -n "${claude_memory_prompt_file}" ]] \
      && ! has_exact_arg "--append-system-prompt" "${launch_args[@]}" \
      && ! has_prefix_arg "--append-system-prompt=" "${launch_args[@]}"; then
      claude_memory_prompt="$(<"${claude_memory_prompt_file}")"
      if [[ -n "${claude_memory_prompt}" ]]; then
        launch_args=(--append-system-prompt "${claude_memory_prompt}" "${launch_args[@]}")
        log_line "claude instructions injected path=${claude_memory_prompt_file} chars=${#claude_memory_prompt}"
      else
        log_line "claude instructions skipped: prompt empty path=${claude_memory_prompt_file}"
      fi
    elif tool_uses_memory_injection "claudeAppendSystemPrompt"; then
      if [[ -z "${claude_memory_prompt_file}" ]]; then
        log_line "claude instructions skipped: prompt file missing"
      else
        log_line "claude instructions skipped: append-system-prompt already provided"
      fi
    fi
    skip_session_id=false
    launch_model="$(extract_model_target "${launch_args[@]}" || true)"
    for arg in "${launch_args[@]}"; do
      case "$arg" in
        --resume|--resume=*|-r|--session-id|--session-id=*|--continue|-c)
          skip_session_id=true
          break
          ;;
      esac
    done

    if [[ "$skip_session_id" == true ]]; then
      resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
      write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
      run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$claude_launch_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
      exit $?
    else
      claude_external_session_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
      write_claude_session_map "${claude_external_session_id}"
      write_runtime_binding "${claude_external_session_id}" "${launch_model}" "" ""
      run_wrapped_ai_command "${claude_external_session_id}" "${launch_model}" "" env PATH="$claude_launch_path" DMUX_EXTERNAL_SESSION_ID="${claude_external_session_id}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" --session-id "${claude_external_session_id}" "${launch_args[@]}"
      exit $?
    fi
  fi
fi

if [[ "$tool_name" == "codex" ]]; then
  helper_script="${wrapper_dir}/dmux-ai-state.sh"
  if [[ "${1:-}" != "app-server" && -x "$helper_script" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_EVENT_DIR:-}" ]]; then
    local_permission_mode="$(configured_permission_mode || true)"
    launch_args=("$@")
    apply_configured_model_arg
    apply_configured_codex_effort_arg
    if [[ "${local_permission_mode}" == "fullAccess" ]] \
      && ! has_exact_arg "--dangerously-bypass-approvals-and-sandbox" "${launch_args[@]}" \
      && ! has_exact_arg "--full-auto" "${launch_args[@]}" \
      && ! has_exact_arg "--sandbox" "${launch_args[@]}" \
      && ! has_prefix_arg "--sandbox=" "${launch_args[@]}" \
      && ! has_exact_arg "--ask-for-approval" "${launch_args[@]}" \
      && ! has_prefix_arg "--ask-for-approval=" "${launch_args[@]}" \
      && ! has_exact_arg "-s" "${launch_args[@]}" \
      && ! has_exact_arg "-a" "${launch_args[@]}"; then
      launch_args=(--dangerously-bypass-approvals-and-sandbox "${launch_args[@]}")
    fi
    apply_default_codex_memory_sandbox_arg
    apply_codex_memory_workspace_args
    apply_codex_memory_developer_instructions
    launch_model="$(extract_model_target "${launch_args[@]}" || true)"
    resume_target=""
    resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
    write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
    run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" --enable hooks "${launch_args[@]}"
    exit $?
  fi
fi

if [[ "$tool_name" == "agy" ]]; then
  local_permission_mode="$(configured_permission_mode || true)"
  launch_args=("$@")
  apply_configured_model_arg
  if [[ "${local_permission_mode}" == "fullAccess" ]] \
    && ! has_exact_arg "--approval-mode" "${launch_args[@]}" \
    && ! has_prefix_arg "--approval-mode=" "${launch_args[@]}" \
    && ! has_exact_arg "--yolo" "${launch_args[@]}" \
    && ! has_exact_arg "-y" "${launch_args[@]}"; then
    launch_args=(--approval-mode yolo "${launch_args[@]}")
  fi
  launch_model="$(extract_model_target "${launch_args[@]}" || true)"
  resume_target=""
  resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
  write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
  run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit $?
fi

if [[ "$tool_name" == "kimi" || "$tool_name" == "kimi-code" ]]; then
  launch_args=("$@")
  apply_configured_model_arg
  apply_kimi_memory_agent_file
  launch_model="$(extract_model_target "${launch_args[@]}" || true)"
  resume_target=""
  resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
  write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
  run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit $?
fi

if [[ "$tool_name" == "kiro-cli" ]]; then
  launch_args=("$@")
  launch_model="$(configured_tool_model || true)"
  resume_target=""
  resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
  write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
  run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit $?
fi

if [[ "$tool_name" == "opencode" || "$tool_name" == "mimo" ]]; then
  local_permission_mode="$(configured_permission_mode || true)"
  launch_args=("$@")
  apply_configured_model_arg
  if [[ "${local_permission_mode}" == "fullAccess" ]] \
    && ! has_exact_arg "--dangerously-skip-permissions" "${launch_args[@]}"; then
    launch_args=(--dangerously-skip-permissions "${launch_args[@]}")
  fi
  launch_model="$(extract_model_target "${launch_args[@]}" || true)"
  resume_target=""
  resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
  opencode_config_dir="${wrapper_dir}/opencode-config"
  write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
  if [[ "$tool_name" == "mimo" ]]; then
    run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" XDG_CONFIG_HOME="${opencode_config_dir}/xdg" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" DMUX_ACTIVE_AI_TOOL="${tool_name}" "$real_bin" "${launch_args[@]}"
  else
    run_wrapped_ai_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" OPENCODE_CONFIG_DIR="${opencode_config_dir}" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" DMUX_ACTIVE_AI_TOOL="${tool_name}" "$real_bin" "${launch_args[@]}"
  fi
  exit $?
fi

if [[ "$tool_name" == "codewhale" ]]; then
  local_permission_mode="$(configured_permission_mode || true)"
  launch_args=("$@")
  apply_configured_model_arg
  if [[ "${local_permission_mode}" == "fullAccess" ]] \
    && ! has_exact_arg "--yolo" "${launch_args[@]}"; then
    launch_args=(--yolo "${launch_args[@]}")
  fi
  launch_model="$(extract_model_target "${launch_args[@]}" || true)"
  resume_target=""
  resume_target="$(extract_resume_target "${launch_args[@]}" || true)"
  write_runtime_binding "${resume_target}" "${launch_model}" "" "$(runtime_session_origin_for_resume "${resume_target}")"
  apply_managed_lifecycle_env
  run_wrapped_command "${resume_target}" "${launch_model}" "" env PATH="$runtime_path" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit_code=$?
  emit_wrapper_session_end
  exit "${exit_code}"
fi

exec env PATH="$runtime_path" "$real_bin" "$@"
