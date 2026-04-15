if [[ -z "${DMUX_ORIGINAL_ZSHRC_SOURCED:-}" ]]; then
  export DMUX_ORIGINAL_ZSHRC_SOURCED=1
  if [[ -f "${HOME}/.zshrc" ]]; then
    export DMUX_HOOK_ORIGINAL_ZDOTDIR="${ZDOTDIR:-}"
    export ZDOTDIR="${HOME}"
    source "${HOME}/.zshrc"
    if [[ -n "${DMUX_HOOK_ORIGINAL_ZDOTDIR:-}" ]]; then
      export ZDOTDIR="${DMUX_HOOK_ORIGINAL_ZDOTDIR}"
    else
      unset ZDOTDIR
    fi
  fi
fi

if [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]]; then
  typeset -gaU path
  path=("${DMUX_WRAPPER_BIN}" ${path:#"${DMUX_WRAPPER_BIN}"})
  export PATH
fi

if [[ -n "${DMUX_ZSH_HOOK_SCRIPT:-}" && -f "${DMUX_ZSH_HOOK_SCRIPT}" ]]; then
  source "${DMUX_ZSH_HOOK_SCRIPT}"
fi
