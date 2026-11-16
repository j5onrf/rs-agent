# Rs-Agent shell hook (minimal: < 0.2ms, Bash & Zsh, macOS & Linux)
[[ $- == *i* ]] || return 0 2>/dev/null || exit 0

# 1. First-Draw Screen Hygiene (Zero-fork, wipes window-spawn hotkey leak before Prompt #1)
_rs_first_draw() {
    [[ -n "${_rs_drawn:-}" ]] && return 0
    _rs_drawn=1
    printf '\x1b[H\x1b[2J'
}
if [[ -z "${_RS_FIRST_DRAW:-}" && "${TERM:-}" != "linux" && -t 0 && -t 1 ]]; then
    export _RS_FIRST_DRAW=1
    printf '\x1b[H\x1b[2J'
    if [[ -n "${ZSH_VERSION:-}" ]]; then
        precmd_functions+=(_rs_first_draw)
    elif [[ -n "${BASH_VERSION:-}" ]]; then
        PROMPT_COMMAND="_rs_first_draw${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    fi
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
