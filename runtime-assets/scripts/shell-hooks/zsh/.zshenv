if [[ "${(t)DMUX_ORIGINAL_ZSHENV_SOURCED:-}" == *export* ]]; then
  unset DMUX_ORIGINAL_ZSHENV_SOURCED
fi

if [[ -z "${DMUX_ORIGINAL_ZSHENV_SOURCED:-}" ]]; then
  typeset -g DMUX_ORIGINAL_ZSHENV_SOURCED=1
  dmux_user_zdotdir="${DMUX_USER_ZDOTDIR:-${HOME}}"
  dmux_runtime_zdotdir="${ZDOTDIR:-}"
  if [[ -f "${dmux_user_zdotdir}/.zshenv" ]]; then
    export ZDOTDIR="${dmux_user_zdotdir}"
    source "${dmux_user_zdotdir}/.zshenv"
    export ZDOTDIR="${dmux_runtime_zdotdir}"
  fi
  unset dmux_user_zdotdir dmux_runtime_zdotdir
fi
