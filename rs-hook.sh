# Rs-Agent Shell Hook (Sub-Millisecond Startup)
_pyra_bin="$HOME/.config/rs-agent/target/release/rs-agent"

# Primary 'ai' command wrapper
ai() {
    "$_pyra_bin" "$@"
}

pyra() {
    "$_pyra_bin" "$@"
}

pyra_handle_missing() {
    [[ -n "${_PYRA_CNF_ACTIVE:-}" ]] && return 127
    local _PYRA_CNF_ACTIVE=1
    local raw_cmd="$*"
    [[ -z "$raw_cmd" ]] && return 127

    # Sub-millisecond lookup in Rust
    local matched
    matched=$("$_pyra_bin" match "$raw_cmd" 2>/dev/null)
    local rc=$?

    if [[ $rc -eq 0 && -n "$matched" ]]; then
        # If the matched command starts with "ai init <dir>" or "ai init <dir>", auto-cd into it
        if [[ "$matched" =~ ^(ai|pyra)[[:space:]]+init[[:space:]]+(.*)$ ]]; then
            local target_dir="${BASH_REMATCH[2]}"
            target_dir="${target_dir/#\~/$HOME}"
            mkdir -p "$target_dir" 2>/dev/null
            cd "$target_dir" 2>/dev/null
        fi

        # Execute directly through binary
        if [[ "$matched" =~ ^(ai|pyra)[[:space:]]+(.*)$ ]]; then
            "$_pyra_bin" ${BASH_REMATCH[2]}
        else
            eval "$matched"
        fi
        return 0
    fi

    # Fall back cleanly if another handler exists
    if declare -F _orig_pyra_cnf >/dev/null 2>&1; then
        _orig_pyra_cnf "$@"
        return $?
    fi

    return 127
}

# Preserve and chain existing hook without collision
if [[ -n "$BASH_VERSION" ]]; then
    if [[ -z "${_PYRA_HOOK_SET:-}" ]]; then
        _PYRA_HOOK_SET=1
        if declare -F command_not_found_handle >/dev/null 2>&1; then
            _orig_def=$(declare -f command_not_found_handle)
            eval "_orig_pyra_cnf() ${_orig_def#*$'\n'}"
            unset _orig_def
        fi
    fi
    command_not_found_handle() { pyra_handle_missing "$@"; }
elif [[ -n "$ZSH_VERSION" ]]; then
    if [[ -z "${_PYRA_HOOK_SET:-}" ]]; then
        _PYRA_HOOK_SET=1
        if (( $+functions[command_not_found_handler] )); then
            functions[_orig_pyra_cnf]=$functions[command_not_found_handler]
        fi
    fi
    command_not_found_handler() { pyra_handle_missing "$@"; }
fi
