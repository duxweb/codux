#!/bin/zsh
set -uo pipefail

tool_name="$1"
shift
wrapper_dir="$(cd "$(dirname "$0")" && pwd)"
wrapper_bin_dir="${wrapper_dir}/bin"
current_path="${PATH:-}"
orig_path="${DMUX_ORIGINAL_PATH:-}"
search_path=""

is_wrapper_bin_dir() {
  local dir="$1"
  [[ -n "$dir" ]] || return 1
  local normalized="${dir:A}"
  [[ "$normalized" == "${wrapper_bin_dir:A}" ]] && return 0
  [[ "$normalized" == */Contents/Resources/runtime-root/scripts/wrappers/bin ]]
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

system_bin_prefix="/usr/bin:/bin:/usr/sbin:/sbin"
managed_system_first_path="${system_bin_prefix}${search_path:+:${search_path}}"

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
  if [[ -n "${DMUX_ACTIVE_AI_RESOLVED_PATH:-}" \
    && -x "${DMUX_ACTIVE_AI_RESOLVED_PATH}" ]] \
    && ! is_wrapper_bin_dir "${DMUX_ACTIVE_AI_RESOLVED_PATH:h}"; then
    print -r -- "${DMUX_ACTIVE_AI_RESOLVED_PATH}"
    return 0
  fi

  local -a candidate_names=()
  case "$tool_name" in
    claude)
      candidate_names=("claude" "claude-code")
      ;;
    claude-code)
      candidate_names=("claude-code" "claude")
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

  if [[ "$tool_name" == "claude" || "$tool_name" == "claude-code" ]]; then
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
    "${wrapper_dir}/dmux-ai-state.sh" stop codux-tauri "$tool_name" </dev/null >/dev/null 2>&1 || true
  fi
  exit 127
fi

codex_hooks_feature_flag() {
  local features_output=""
  features_output="$(env PATH="$search_path" "$real_bin" features list 2>/dev/null || true)"
  if print -r -- "$features_output" | /usr/bin/grep -Eq '^hooks[[:space:]]'; then
    print -r -- "hooks"
    return 0
  fi
  if print -r -- "$features_output" | /usr/bin/grep -Eq '^codex_hooks[[:space:]]'; then
    print -r -- "codex_hooks"
    return 0
  fi
  print -r -- "hooks"
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  print -rn -- "$value"
}

log_line() {
  [[ -n "${DMUX_LOG_FILE:-}" ]] || return 0
  /bin/mkdir -p -- "${DMUX_LOG_FILE:h}"
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [wrapper] $1" >> "${DMUX_LOG_FILE}"
}

memory_prompt_file() {
  [[ -n "${DMUX_AI_MEMORY_PROMPT_FILE:-}" && -f "${DMUX_AI_MEMORY_PROMPT_FILE}" ]] || return 1
  print -r -- "${DMUX_AI_MEMORY_PROMPT_FILE}"
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
    claude|claude-code)
      config_key="claudeCode"
      ;;
    gemini|agy)
      config_key="gemini"
      ;;
    opencode)
      config_key="opencode"
      ;;
    *)
      return 0
      ;;
  esac

  CONFIG_PATH="${config_path}" CONFIG_KEY="${config_key}" /usr/bin/python3 - <<'PY'
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


configured_tool_model() {
  local config_path
  config_path="$(tool_permission_settings_path)"
  [[ -f "${config_path}" ]] || return 0

  local config_key=""
  case "${tool_name}" in
    codex)
      config_key="codexModel"
      ;;
    claude|claude-code)
      config_key="claudeCodeModel"
      ;;
    gemini|agy)
      config_key="geminiModel"
      ;;
    opencode)
      config_key="opencodeModel"
      ;;
    *)
      return 0
      ;;
  esac

  CONFIG_PATH="${config_path}" CONFIG_KEY="${config_key}" /usr/bin/python3 - <<'PY'
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
if isinstance(value, str):
    value = value.strip()
    if value:
        print(value)
PY
}

configured_codex_effort() {
  local config_path
  config_path="$(tool_permission_settings_path)"
  [[ -f "${config_path}" ]] || return 0

  CONFIG_PATH="${config_path}" /usr/bin/python3 - <<'PY'
import json
import os

path = os.environ.get("CONFIG_PATH", "")
if not path:
    raise SystemExit(0)

try:
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except Exception:
    raise SystemExit(0)

value = payload.get("codexEffort")
if isinstance(value, str):
    value = value.strip()
    if value in {"none", "minimal", "low", "medium", "high", "xhigh"}:
        print(value)
PY
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
    claude|claude-code|gemini|agy|opencode)
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
  [[ "${tool_name}" == "codex" ]] || return 0
  [[ -n "${DMUX_AI_MEMORY_WORKSPACE_ROOT:-}" && -d "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" ]] || return 0
  [[ -n "${DMUX_PROJECT_PATH:-}" && -d "${DMUX_PROJECT_PATH}" ]] || return 0
  codex_has_sandbox_mode_arg "${launch_args[@]}" && return 0
  launch_args=(--sandbox workspace-write "${launch_args[@]}")
}

apply_codex_memory_workspace_args() {
  [[ "${tool_name}" == "codex" ]] || return 0
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
  VALUE="${value}" /usr/bin/python3 - <<'PY'
import json
import os

print(json.dumps(os.environ.get("VALUE", ""), ensure_ascii=True))
PY
}

apply_codex_memory_developer_instructions() {
  [[ "${tool_name}" == "codex" ]] || return 0
  [[ -n "${DMUX_AI_MEMORY_WORKSPACE_ROOT:-}" && -d "${DMUX_AI_MEMORY_WORKSPACE_ROOT}" ]] || return 0
  local memory_agents="${DMUX_AI_MEMORY_WORKSPACE_ROOT}/AGENTS.md"
  [[ -f "${memory_agents}" ]] || return 0
  has_config_key_arg "developer_instructions" "${launch_args[@]}" && return 0
  local memory_instructions
  memory_instructions="$(<"${memory_agents}")"
  [[ -n "${memory_instructions}" ]] || return 0
  launch_args=(-c "developer_instructions=$(codex_toml_string "${memory_instructions}")" "${launch_args[@]}")
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
  if [[ "${tool_name}" == "opencode" && -z "${external_session_id}" && -n "${DMUX_OPENCODE_SESSION_MAP_DIR:-}" && -n "${DMUX_SESSION_ID:-}" ]]; then
    local opencode_state_path="${DMUX_OPENCODE_SESSION_MAP_DIR}/opencode-session-${DMUX_SESSION_ID}.json"
    if [[ -f "${opencode_state_path}" ]]; then
      local resolved_state
      resolved_state="$(
        OPENCODE_STATE_PATH="${opencode_state_path}" /usr/bin/python3 - <<'PY'
import json
import os

path = os.environ.get("OPENCODE_STATE_PATH", "")
if not path:
    raise SystemExit(0)

try:
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except Exception:
    raise SystemExit(0)

external = payload.get("externalSessionID")
model = payload.get("model")
if isinstance(external, str) and external:
    print(external)
if isinstance(model, str) and model:
    print(model)
PY
)"
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
  log_line "process exit tool=${DMUX_ACTIVE_AI_TOOL:-$tool_name} session=${DMUX_SESSION_ID:-nil} code=${exit_code} externalSession=${external_session_id:-nil}"
  return "${exit_code}"
}

extract_resume_target() {
  local previous=""
  for arg in "$@"; do
    case "${previous}" in
      --resume)
        [[ -n "$arg" && "$arg" != -* ]] && print -r -- "$arg"
        return 0
        ;;
      --session)
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

if [[ "$tool_name" == "claude" || "$tool_name" == "claude-code" ]]; then
  helper_script="${wrapper_dir}/dmux-ai-state.sh"
  if [[ -x "$helper_script" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_SOCKET:-}" ]]; then
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
    if [[ -n "${claude_memory_prompt_file}" ]] \
      && ! has_exact_arg "--append-system-prompt" "${launch_args[@]}" \
      && ! has_prefix_arg "--append-system-prompt=" "${launch_args[@]}"; then
      claude_memory_prompt="$(<"${claude_memory_prompt_file}")"
      if [[ -n "${claude_memory_prompt}" ]]; then
        launch_args=(--append-system-prompt "${claude_memory_prompt}" "${launch_args[@]}")
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
      run_wrapped_command "${resume_target}" "${launch_model}" "" env PATH="$claude_launch_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
      exit $?
    else
      claude_external_session_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
      write_claude_session_map "${claude_external_session_id}"
      log_line "launch claude session=${DMUX_SESSION_ID} externalSession=${claude_external_session_id}"
      run_wrapped_command "${claude_external_session_id}" "${launch_model}" "" env PATH="$claude_launch_path" DMUX_EXTERNAL_SESSION_ID="${claude_external_session_id}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" --session-id "${claude_external_session_id}" "${launch_args[@]}"
      exit $?
    fi
  fi
fi

if [[ "$tool_name" == "codex" ]]; then
  helper_script="${wrapper_dir}/dmux-ai-state.sh"
  if [[ "${1:-}" != "app-server" && -x "$helper_script" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_SOCKET:-}" ]]; then
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
    hooks_feature="$(codex_hooks_feature_flag)"
    log_line "launch codex managed session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-} binary=${real_bin} hooks=${hooks_feature}"
    run_wrapped_command "" "${launch_model}" "" env PATH="$search_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" --enable "${hooks_feature}" "${launch_args[@]}"
    exit $?
  fi
fi

if [[ "$tool_name" == "gemini" || "$tool_name" == "agy" ]]; then
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
  log_line "launch managed tool=${tool_name} session=${DMUX_SESSION_ID:-nil} project=${DMUX_PROJECT_ID:-nil} binary=${real_bin} invocation=${DMUX_ACTIVE_AI_INVOCATION_ID:-nil} resume=${resume_target:-nil}"
  run_wrapped_command "${resume_target}" "${launch_model}" "" env PATH="$search_path" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit $?
fi

if [[ "$tool_name" == "opencode" ]]; then
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
  log_line "launch managed tool=${tool_name} session=${DMUX_SESSION_ID:-nil} project=${DMUX_PROJECT_ID:-nil} binary=${real_bin} invocation=${DMUX_ACTIVE_AI_INVOCATION_ID:-nil} resume=${resume_target:-nil} configDir=${opencode_config_dir}"
  run_wrapped_command "${resume_target}" "${launch_model}" "" env PATH="$search_path" OPENCODE_CONFIG_DIR="${opencode_config_dir}" DMUX_EXTERNAL_SESSION_ID="${resume_target}" DMUX_ACTIVE_AI_MODEL="${launch_model}" "$real_bin" "${launch_args[@]}"
  exit $?
fi

exec env PATH="$search_path" "$real_bin" "$@"
