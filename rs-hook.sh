# Production rs-agent Shell Hook (Sub-Millisecond Startup)
_rs_bin="$HOME/.config/rs-agent/target/release/rs-agent"
[[ ! -x "$_rs_bin" ]] && _rs_bin="$HOME/.config/rs-agent/target/debug/rs-agent"

# Commands
ai() { "$_rs_bin" "$@"; }
rs() { "$_rs_bin" "$@"; }
pyra() { "$_rs_bin" "$@"; }

rs_handle_missing() {
    [[ -n "${_RS_CNF_ACTIVE:-}" ]] && return 127
    local _RS_CNF_ACTIVE=1
    local raw_cmd="$*"
    [[ -z "$raw_cmd" ]] && return 127

    local matched
    matched=$("$_rs_bin" match "$raw_cmd" 2>/dev/null)
    local rc=$?

    if [[ $rc -eq 0 && -n "$matched" ]]; then
        if [[ "$matched" =~ ^(ai|rs|pyra)[[:space:]]+init[[:space:]]+(.*)$ ]]; then
            local target_dir="${BASH_REMATCH[2]}"
            target_dir="${target_dir/#\~/$HOME}"
            mkdir -p "$target_dir" 2>/dev/null
            cd "$target_dir" 2>/dev/null
        fi

        if [[ "$matched" =~ ^(ai|rs|pyra)[[:space:]]+(.*)$ ]]; then
            "$_rs_bin" ${BASH_REMATCH[2]}
        else
            eval "$matched"
        fi
        return 0
    fi

    if declare -F _orig_rs_cnf >/dev/null 2>&1; then
        _orig_rs_cnf "$@"
        return $?
    fi
    return 127
}

if [[ -n "$BASH_VERSION" ]]; then
    if [[ -z "${_RS_HOOK_SET:-}" ]]; then
        _RS_HOOK_SET=1
        if declare -F command_not_found_handle >/dev/null 2>&1; then
            _orig_def=$(declare -f command_not_found_handle)
            eval "_orig_rs_cnf() ${_orig_def#*$'\n'}"
            unset _orig_def
        fi
    fi
    command_not_found_handle() { rs_handle_missing "$@"; }
elif [[ -n "$ZSH_VERSION" ]]; then
    if [[ -z "${_RS_HOOK_SET:-}" ]]; then
        _RS_HOOK_SET=1
        if (( $+functions[command_not_found_handler] )); then
            functions[_orig_rs_cnf]=$functions[command_not_found_handler]
        fi
    fi
    command_not_found_handler() { rs_handle_missing "$@"; }
fi
