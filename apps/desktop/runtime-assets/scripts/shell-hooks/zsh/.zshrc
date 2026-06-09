if [[ "${(t)DMUX_ORIGINAL_ZSHRC_SOURCED:-}" == *export* ]]; then
  unset DMUX_ORIGINAL_ZSHRC_SOURCED
fi

if [[ -z "${DMUX_ORIGINAL_ZSHRC_SOURCED:-}" ]]; then
  typeset -g DMUX_ORIGINAL_ZSHRC_SOURCED=1
  dmux_user_zdotdir="${DMUX_USER_ZDOTDIR:-${HOME}}"
  dmux_runtime_zdotdir="${ZDOTDIR:-}"
  if [[ -f "${dmux_user_zdotdir}/.zshrc" ]]; then
    export ZDOTDIR="${dmux_user_zdotdir}"
    source "${dmux_user_zdotdir}/.zshrc"
    export ZDOTDIR="${dmux_runtime_zdotdir}"
  fi
  unset dmux_user_zdotdir dmux_runtime_zdotdir
fi

dmux_user_zdotdir="${DMUX_USER_ZDOTDIR:-${HOME}}"
dmux_runtime_zdotdir="${ZDOTDIR:-}"
if [[ -z "${HISTFILE:-}" || "${HISTFILE}" == "${dmux_runtime_zdotdir}/.zsh_history" ]]; then
  export HISTFILE="${dmux_user_zdotdir}/.zsh_history"
fi
unset dmux_user_zdotdir dmux_runtime_zdotdir

if [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]]; then
  typeset -gaU path
  path=("${DMUX_WRAPPER_BIN}" ${path:#"${DMUX_WRAPPER_BIN}"})
  export PATH
fi

if [[ -n "${DMUX_ZSH_HOOK_SCRIPT:-}" && -f "${DMUX_ZSH_HOOK_SCRIPT}" ]]; then
  source "${DMUX_ZSH_HOOK_SCRIPT}"
fi
