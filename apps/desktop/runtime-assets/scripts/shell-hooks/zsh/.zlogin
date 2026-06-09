if [[ "${(t)DMUX_ORIGINAL_ZLOGIN_SOURCED:-}" == *export* ]]; then
  unset DMUX_ORIGINAL_ZLOGIN_SOURCED
fi

if [[ -z "${DMUX_ORIGINAL_ZLOGIN_SOURCED:-}" ]]; then
  typeset -g DMUX_ORIGINAL_ZLOGIN_SOURCED=1
  dmux_user_zdotdir="${DMUX_USER_ZDOTDIR:-${HOME}}"
  dmux_runtime_zdotdir="${ZDOTDIR:-}"
  if [[ -f "${dmux_user_zdotdir}/.zlogin" ]]; then
    export ZDOTDIR="${dmux_user_zdotdir}"
    source "${dmux_user_zdotdir}/.zlogin"
    export ZDOTDIR="${dmux_runtime_zdotdir}"
  fi
  unset dmux_user_zdotdir dmux_runtime_zdotdir
fi

if [[ -n "${DMUX_WRAPPER_BIN:-}" && -d "${DMUX_WRAPPER_BIN}" ]]; then
  typeset -gaU path
  path=("${DMUX_WRAPPER_BIN}" ${path:#"${DMUX_WRAPPER_BIN}"})
  export PATH
fi
