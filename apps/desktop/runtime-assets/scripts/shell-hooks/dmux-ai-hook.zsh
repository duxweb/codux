if [[ -n "${DMUX_AI_HOOK_INSTALLED:-}" ]]; then
  return 0
fi
export DMUX_AI_HOOK_INSTALLED=1

zmodload zsh/datetime 2>/dev/null || true
autoload -Uz add-zsh-hook

typeset -g DMUX_ACTIVE_AI_TOOL=""
typeset -g DMUX_ACTIVE_AI_STARTED_AT=""
typeset -g DMUX_ACTIVE_AI_INVOCATION_ID=""
typeset -g DMUX_ACTIVE_AI_RESOLVED_PATH=""
export DMUX_ACTIVE_AI_TOOL
export DMUX_ACTIVE_AI_STARTED_AT
export DMUX_ACTIVE_AI_INVOCATION_ID
export DMUX_ACTIVE_AI_RESOLVED_PATH

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

_dmux_reset_terminal_input_modes() {
  [[ -t 1 ]] || return 0
  printf '\033[<u' || true
}

# Arrow keys must work whether or not a prior full-screen program (e.g. a dev
# server killed mid-run) left the terminal in DECCKM "application cursor" mode,
# in which arrows send `ESC O A` instead of `ESC [ A`. zsh only binds the form
# matching its current keypad state, so a leftover mode desync makes arrows
# self-insert as `^[OA` at the prompt until the line editor re-syncs (today only
# a Ctrl-C round-trip fixes it). Mirror each cursor key's normal-mode (CSI)
# binding onto its application-mode (SS3) form so both encodings always resolve.
# Non-destructive: existing user bindings are kept; only missing forms are added.
_dmux_bind_cursor_keys() {
  emulate -L zsh
  local -a specs=(
    '^[[A' '^[OA' up-line-or-history
    '^[[B' '^[OB' down-line-or-history
    '^[[C' '^[OC' forward-char
    '^[[D' '^[OD' backward-char
    '^[[H' '^[OH' beginning-of-line
    '^[[F' '^[OF' end-of-line
  )
  local csi ss3 fallback binding widget ss3_widget i
  for (( i = 1; i <= ${#specs}; i += 3 )); do
    csi="${specs[i]}"; ss3="${specs[i + 1]}"; fallback="${specs[i + 2]}"
    # Resolve the normal-mode widget (falling back to a sane default if unbound).
    binding="$(bindkey -- "$csi" 2>/dev/null)"
    widget="${binding##* }"
    if [[ -z "$widget" || "$widget" == undefined-key ]]; then
      widget="$fallback"
      bindkey -- "$csi" "$widget"
    fi
    # Mirror it onto the application-mode form only when that form is unbound,
    # so a user's own SS3 binding is never overridden.
    binding="$(bindkey -- "$ss3" 2>/dev/null)"
    ss3_widget="${binding##* }"
    if [[ -z "$ss3_widget" || "$ss3_widget" == undefined-key ]]; then
      bindkey -- "$ss3" "$widget"
    fi
  done
}

_dmux_is_wrapper_bin_dir() {
  local dir="$1"
  [[ -n "${dir}" ]] || return 1
  local normalized="${dir:A}"
  if [[ -n "${DMUX_WRAPPER_BIN:-}" && "${normalized}" == "${DMUX_WRAPPER_BIN:A}" ]]; then
    return 0
  fi
  [[ "${normalized}" == */Contents/Resources/runtime-root/scripts/wrappers/bin ]]
}

_dmux_filtered_tool_search_path() {
  local source_path="${1:-}"
  local -a parts filtered
  parts=(${(s/:/)source_path})
  local dir
  for dir in "${parts[@]}"; do
    [[ -n "${dir}" ]] || continue
    _dmux_is_wrapper_bin_dir "${dir}" && continue
    filtered+=("${dir}")
  done
  print -r -- "${(j/:/)filtered}"
}

_dmux_prepend_wrapper_bin() {
  [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]] || return 0
  typeset -gaU path
  path=("${DMUX_WRAPPER_BIN}" ${path:#"${DMUX_WRAPPER_BIN}"})
  export PATH
}

_dmux_exec_wrapped_tool() {
  local tool="$1"
  shift
  _dmux_prepend_wrapper_bin
  local wrapper="${DMUX_WRAPPER_BIN:-}/${tool}"
  if [[ -n "${DMUX_WRAPPER_BIN:-}" && -x "${wrapper}" ]]; then
    "${wrapper}" "$@"
    return $?
  fi
  command "${tool}" "$@"
}

_dmux_define_tool_shims() {
  [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]] || return 0
  codex() { _dmux_exec_wrapped_tool codex "$@"; }
  claude() { _dmux_exec_wrapped_tool claude "$@"; }
  claude-code() { _dmux_exec_wrapped_tool claude-code "$@"; }
  reclaude() { _dmux_exec_wrapped_tool reclaude "$@"; }
  opencode() { _dmux_exec_wrapped_tool opencode "$@"; }
  agy() { _dmux_exec_wrapped_tool agy "$@"; }
  kiro-cli() { _dmux_exec_wrapped_tool kiro-cli "$@"; }
  codewhale() { _dmux_exec_wrapped_tool codewhale "$@"; }
  kimi() { _dmux_exec_wrapped_tool kimi "$@"; }
  kimi-code() { _dmux_exec_wrapped_tool kimi-code "$@"; }
  mimo() { _dmux_exec_wrapped_tool mimo "$@"; }
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
      codex|claude|claude-code|reclaude|opencode|agy|kiro-cli|codewhale|kimi|kimi-code|mimo)
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
  DMUX_ACTIVE_AI_TOOL="${tool}"
  DMUX_ACTIVE_AI_STARTED_AT="$(_dmux_now)"
  DMUX_ACTIVE_AI_INVOCATION_ID="$(_dmux_new_invocation_id)"
  local resolved_path=""
  local resolve_search_path=""
  resolve_search_path="$(_dmux_filtered_tool_search_path "${DMUX_ORIGINAL_PATH:-$PATH}")"
  resolved_path="$(PATH="${resolve_search_path}" whence -p "${tool}" 2>/dev/null || true)"
  if [[ -n "${resolved_path}" ]] && _dmux_is_wrapper_bin_dir "${resolved_path:h}"; then
    resolved_path=""
  fi
  DMUX_ACTIVE_AI_RESOLVED_PATH="${resolved_path}"
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
  export DMUX_ACTIVE_AI_RESOLVED_PATH
  _dmux_prepend_wrapper_bin
  _dmux_define_tool_shims
}

_dmux_ai_precmd() {
  _dmux_prepend_wrapper_bin
  _dmux_define_tool_shims
  [[ -n "${DMUX_ACTIVE_AI_TOOL}" ]] || return 0
  _dmux_reset_terminal_input_modes
  DMUX_ACTIVE_AI_TOOL=""
  DMUX_ACTIVE_AI_STARTED_AT=""
  DMUX_ACTIVE_AI_INVOCATION_ID=""
  DMUX_ACTIVE_AI_RESOLVED_PATH=""
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
  export DMUX_ACTIVE_AI_RESOLVED_PATH
}

_dmux_ai_zshexit() {
  if [[ -n "${DMUX_ACTIVE_AI_TOOL}" ]]; then
    _dmux_reset_terminal_input_modes
  fi
  DMUX_ACTIVE_AI_TOOL=""
  DMUX_ACTIVE_AI_STARTED_AT=""
  DMUX_ACTIVE_AI_INVOCATION_ID=""
  DMUX_ACTIVE_AI_RESOLVED_PATH=""
  export DMUX_ACTIVE_AI_TOOL
  export DMUX_ACTIVE_AI_STARTED_AT
  export DMUX_ACTIVE_AI_INVOCATION_ID
  export DMUX_ACTIVE_AI_RESOLVED_PATH
}

# OSC 133 semantic marks (C=command start, D=command end, A=prompt); the Codux
# terminal derives per-command busy state from C/D. Registered before the AI
# precmd so $? still holds the finished command's exit code.
typeset -g _dmux_osc133_command_running=""

_dmux_osc133_preexec() {
  _dmux_osc133_command_running=1
  # AI TUIs own their status (progress OSC / runtime probe); a session-long
  # command mark would fake a permanent spinner, so skip C and keep only the
  # exit-time D, which sweeps up any leftover dot.
  if _dmux_resolve_tool_from_command "$1" >/dev/null 2>&1; then
    return 0
  fi
  printf '\033]133;C\007'
}

_dmux_osc133_precmd() {
  local exit_code=$?
  if [[ -n "${_dmux_osc133_command_running}" ]]; then
    _dmux_osc133_command_running=""
    printf '\033]133;D;%d\007' "${exit_code}"
  fi
  printf '\033]133;A\007'
}

add-zsh-hook preexec _dmux_osc133_preexec
add-zsh-hook precmd _dmux_osc133_precmd

add-zsh-hook preexec _dmux_ai_preexec
add-zsh-hook precmd _dmux_ai_precmd
add-zsh-hook zshexit _dmux_ai_zshexit

_dmux_prepend_wrapper_bin
_dmux_define_tool_shims
_dmux_bind_cursor_keys
_dmux_reset_terminal_input_modes
