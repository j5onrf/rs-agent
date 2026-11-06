# rs-agent shell hook (ponytail minimalism: < 0.2ms, Bash & Zsh, macOS & Linux)
[[ $- == *i* ]] || return 0 2>/dev/null || exit 0

# 1. First-Draw Screen Hygiene (Zero-fork builtin: < 0.01ms, wipes window-spawn hotkey leak)
if [[ -z "${_RS_FIRST_DRAW:-}" ]]; then
    _RS_FIRST_DRAW=1
    printf '\x1b[H\x1b[2J'
fi

# 2. Zero-Symlink PATH Injection
if [[ ":$PATH:" != *":$HOME/.config/rs-agent/target/release:"* ]]; then
    export PATH="$HOME/.config/rs-agent/target/release:$PATH"
fi

# 3. Context Router (< 0.2ms sub-millisecond match)
_rs_route() {
    local cmd
    cmd=$(command ai match "$@" 2>/dev/null) || return 127
    [[ -n "$cmd" ]] || return 127
    eval "$cmd"
}

# 4. Missing Command Hooks (Bash + Zsh)
if [[ -n "$BASH_VERSION" ]]; then
    command_not_found_handle() {
        _rs_route "$@" || return 127
    }
elif [[ -n "$ZSH_VERSION" ]]; then
    command_not_found_handler() {
        _rs_route "$@" || return 127
    }
fi
