if [[ -z "${DMUX_ORIGINAL_ZSHENV_SOURCED:-}" ]]; then
  export DMUX_ORIGINAL_ZSHENV_SOURCED=1
  if [[ -f "${HOME}/.zshenv" ]]; then
    export DMUX_HOOK_ORIGINAL_ZDOTDIR="${ZDOTDIR:-}"
    export ZDOTDIR="${HOME}"
    source "${HOME}/.zshenv"
    if [[ -n "${DMUX_HOOK_ORIGINAL_ZDOTDIR:-}" ]]; then
      export ZDOTDIR="${DMUX_HOOK_ORIGINAL_ZDOTDIR}"
    else
      unset ZDOTDIR
    fi
  fi
fi
