# Rs-Agent shell hook (ponytail minimalism: < 0.2ms, Bash & Zsh, macOS & Linux)
[[ $- == *i* ]] || return 0 2>/dev/null || exit 0

# 1. First-Draw Screen Hygiene (Zero-fork builtin: < 0.01ms, wipes window-spawn hotkey leak)
# Only clear on top-level terminal window spawn (SHLVL <= 1) to avoid wiping nested subshells.
if [[ -z "${_RS_FIRST_DRAW:-}" && ${SHLVL:-1} -le 1 ]]; then
    _RS_FIRST_DRAW=1
    printf '\x1b[H\x1b[2J'
fi

# 2. Zero-Symlink PATH Injection
if [[ ":$PATH:" != *":$HOME/.config/rs-agent/target/release:"* ]]; then
    export PATH="$HOME/.config/rs-agent/target/release:$PATH"
fi

# 3. Context Router (< 0.2ms match with dynamically-scoped stack guard)
_rs_route() {
    [[ -n "${_RS_ROUTING:-}" ]] && return 127
    local _RS_ROUTING=1

    local cmd status
    cmd=$(command ai match "$@" 2>/dev/null)
    status=$?

    if [[ $status -eq 0 && -n "$cmd" ]]; then
        eval "$cmd"
        return $?
    fi

    printf '%s: command not found: %s\n' "${ZSH_VERSION:+zsh}${BASH_VERSION:+bash}" "$1" >&2
    return 127
}

# 4. Missing Command Hooks (Bash + Zsh)
if [[ -n "${BASH_VERSION:-}" ]]; then
    command_not_found_handle() {
        _rs_route "$@"
        return $?
    }
elif [[ -n "${ZSH_VERSION:-}" ]]; then
    command_not_found_handler() {
        _rs_route "$@"
        return $?
    }
fi
