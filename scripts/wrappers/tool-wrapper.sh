#!/bin/zsh
set -uo pipefail

tool_name="$1"
shift
wrapper_dir="$(cd "$(dirname "$0")" && pwd)"
wrapper_bin_dir="${wrapper_dir}/bin"
current_path="${PATH:-}"
orig_path="${DMUX_ORIGINAL_PATH:-}"
search_path=""

if [[ -n "$current_path" ]]; then
  path_without_wrapper_parts=()
  path_parts=(${(s/:/)current_path})
  for dir in "${path_parts[@]}"; do
    [[ -z "$dir" ]] && continue
    [[ "$dir" == "$wrapper_bin_dir" ]] && continue
    path_without_wrapper_parts+=("$dir")
  done
  search_path="${(j/:/)path_without_wrapper_parts}"
fi

if [[ -z "$search_path" ]]; then
  search_path="$orig_path"
fi

find_real_binary() {
  local search_parts
  search_parts=(${(s/:/)search_path})
  local -a candidate_names=("$tool_name")
  for dir in "${search_parts[@]}"; do
    for binary_name in "${candidate_names[@]}"; do
      local candidate="$dir/$binary_name"
      if [[ -x "$candidate" && "$candidate" != "$wrapper_dir/bin/$tool_name" ]]; then
        print -r -- "$candidate"
        return 0
      fi
    done
  done

  case "$tool_name" in
    claude|claude-code)
      [[ -x "/Applications/cmux.app/Contents/Resources/bin/claude" ]] && print -r -- "/Applications/cmux.app/Contents/Resources/bin/claude" && return 0
      ;;
  esac

  return 1
}

real_bin="$(find_real_binary || true)"
if [[ -z "$real_bin" ]]; then
  print -u2 -- "wrapper: failed to locate real binary for $tool_name"
  exit 127
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

log_line() {
  [[ -n "${DMUX_LOG_FILE:-}" ]] || return 0
  /bin/mkdir -p -- "${DMUX_LOG_FILE:h}"
  print -r -- "[$(/bin/date '+%Y-%m-%dT%H:%M:%S%z')] [wrapper] $1" >> "${DMUX_LOG_FILE}"
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

send_usage_runtime_event() {
  local phase="${1:-running}"
  local external_session_id="${2:-}"
  local model="${3:-}"

  [[ -n "${DMUX_RUNTIME_SOCKET:-}" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_PROJECT_ID:-}" ]] || return 0
  command -v /usr/bin/nc >/dev/null 2>&1 || return 0
  local now
  now="$(now)"
  local payload
  payload="$(
    {
      print -rn -- '{"kind":"usage","payload":{'
      print -rn -- "\"sessionId\":\"$(json_escape "${DMUX_SESSION_ID}")\","
      print -rn -- "\"sessionInstanceId\":\"$(json_escape "${DMUX_SESSION_INSTANCE_ID:-}")\","
      if [[ -n "${DMUX_ACTIVE_AI_INVOCATION_ID:-}" ]]; then
        print -rn -- "\"invocationId\":\"$(json_escape "${DMUX_ACTIVE_AI_INVOCATION_ID}")\","
      else
        print -rn -- "\"invocationId\":null,"
      fi
      if [[ -n "${external_session_id}" ]]; then
        print -rn -- "\"externalSessionID\":\"$(json_escape "${external_session_id}")\","
      else
        print -rn -- "\"externalSessionID\":null,"
      fi
      print -rn -- "\"projectId\":\"$(json_escape "${DMUX_PROJECT_ID}")\","
      print -rn -- "\"projectName\":\"$(json_escape "${DMUX_PROJECT_NAME:-Workspace}")\","
      print -rn -- "\"projectPath\":\"$(json_escape "${DMUX_PROJECT_PATH:-}")\","
      print -rn -- "\"sessionTitle\":\"$(json_escape "${DMUX_SESSION_TITLE:-Terminal}")\","
      print -rn -- "\"tool\":\"$(json_escape "${DMUX_ACTIVE_AI_TOOL:-$tool_name}")\","
      if [[ -n "${model}" ]]; then
        print -rn -- "\"model\":\"$(json_escape "${model}")\","
      else
        print -rn -- "\"model\":null,"
      fi
      print -rn -- "\"status\":\"$(json_escape "${phase}")\","
      if [[ "${phase}" == "running" && ( "${DMUX_ACTIVE_AI_TOOL:-$tool_name}" == "codex" || "${DMUX_ACTIVE_AI_TOOL:-$tool_name}" == "claude" || "${DMUX_ACTIVE_AI_TOOL:-$tool_name}" == "claude-code" || "${DMUX_ACTIVE_AI_TOOL:-$tool_name}" == "gemini" || "${DMUX_ACTIVE_AI_TOOL:-$tool_name}" == "opencode" ) ]]; then
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
  print -r -- "${payload}" | /usr/bin/nc -U -w 1 "${DMUX_RUNTIME_SOCKET}" >/dev/null 2>&1 || true
  log_line "usage event tool=${DMUX_ACTIVE_AI_TOOL:-$tool_name} phase=${phase} session=${DMUX_SESSION_ID} externalSession=${external_session_id:-nil}"
}

run_wrapped_command() {
  local external_session_id="${1:-}"
  local model="${2:-}"
  shift 2

  "$@"
  local exit_code=$?
  send_usage_runtime_event completed "${external_session_id}" "${model}"
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
    skip_session_id=false
    for arg in "$@"; do
      case "$arg" in
        --resume|--resume=*|-r|--session-id|--session-id=*|--continue|-c)
          skip_session_id=true
          break
          ;;
      esac
    done

    helper_script_json="$(json_escape "${helper_script}")"
    session_start_command="\\\"${helper_script_json}\\\" session-start claude"
    stop_command="\\\"${helper_script_json}\\\" stop claude"
    stop_failure_command="\\\"${helper_script_json}\\\" stop-failure claude"
    session_end_command="\\\"${helper_script_json}\\\" session-end claude"
    prompt_submit_command="\\\"${helper_script_json}\\\" prompt-submit claude"
    pre_tool_use_command="\\\"${helper_script_json}\\\" pre-tool-use claude"
    hooks_json='{"hooks":{"SessionStart":[{"matcher":"","hooks":[{"type":"command","command":"'"${session_start_command}"'","timeout":5}]}],"Stop":[{"matcher":"","hooks":[{"type":"command","command":"'"${stop_command}"'","timeout":5}]}],"StopFailure":[{"matcher":"","hooks":[{"type":"command","command":"'"${stop_failure_command}"'","timeout":5}]}],"SessionEnd":[{"matcher":"","hooks":[{"type":"command","command":"'"${session_end_command}"'","timeout":5}]}],"UserPromptSubmit":[{"matcher":"","hooks":[{"type":"command","command":"'"${prompt_submit_command}"'","timeout":5}]}],"PreToolUse":[{"matcher":"","hooks":[{"type":"command","command":"'"${pre_tool_use_command}"'","timeout":5,"async":true}]}]}}'

    if [[ "$skip_session_id" == true ]]; then
      resume_target="$(extract_resume_target "$@" || true)"
      if [[ -n "${resume_target}" ]]; then
        send_usage_runtime_event running "${resume_target}"
      fi
      run_wrapped_command "${resume_target}" "" env PATH="$search_path" "$real_bin" --settings "$hooks_json" "$@"
      exit $?
    else
      claude_external_session_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
      write_claude_session_map "${claude_external_session_id}"
      send_usage_runtime_event running "${claude_external_session_id}"
      log_line "launch claude session=${DMUX_SESSION_ID} externalSession=${claude_external_session_id}"
      run_wrapped_command "${claude_external_session_id}" "" env PATH="$search_path" DMUX_EXTERNAL_SESSION_ID="${claude_external_session_id}" "$real_bin" --session-id "${claude_external_session_id}" --settings "$hooks_json" "$@"
      exit $?
    fi
  fi
fi

if [[ "$tool_name" == "codex" ]]; then
  helper_script="${wrapper_dir}/dmux-ai-state.sh"
  if [[ "${1:-}" != "app-server" && -x "$helper_script" && -n "${DMUX_SESSION_ID:-}" && -n "${DMUX_RUNTIME_SOCKET:-}" ]]; then
    log_line "launch codex managed session=${DMUX_SESSION_ID} project=${DMUX_PROJECT_ID:-} binary=${real_bin} hooks=enabled"
    run_wrapped_command "" "" env PATH="$search_path" "$real_bin" --enable codex_hooks "$@"
    exit $?
  fi
fi

if [[ "$tool_name" == "gemini" ]]; then
  resume_target=""
  resume_target="$(extract_resume_target "$@" || true)"
  if [[ -n "${resume_target}" ]]; then
    send_usage_runtime_event running "${resume_target}"
  fi
  log_line "launch managed tool=${tool_name} session=${DMUX_SESSION_ID:-nil} project=${DMUX_PROJECT_ID:-nil} binary=${real_bin} invocation=${DMUX_ACTIVE_AI_INVOCATION_ID:-nil} resume=${resume_target:-nil}"
  run_wrapped_command "${resume_target}" "" env PATH="$search_path" "$real_bin" "$@"
  exit $?
fi

if [[ "$tool_name" == "opencode" ]]; then
  resume_target=""
  resume_target="$(extract_resume_target "$@" || true)"
  opencode_config_dir="${wrapper_dir}/opencode-config"
  if [[ -n "${resume_target}" ]]; then
    send_usage_runtime_event running "${resume_target}"
  fi
  log_line "launch managed tool=${tool_name} session=${DMUX_SESSION_ID:-nil} project=${DMUX_PROJECT_ID:-nil} binary=${real_bin} invocation=${DMUX_ACTIVE_AI_INVOCATION_ID:-nil} resume=${resume_target:-nil} configDir=${opencode_config_dir}"
  run_wrapped_command "${resume_target}" "" env PATH="$search_path" OPENCODE_CONFIG_DIR="${opencode_config_dir}" DMUX_EXTERNAL_SESSION_ID="${resume_target}" "$real_bin" "$@"
  exit $?
fi

exec env PATH="$search_path" "$real_bin" "$@"
