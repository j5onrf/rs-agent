# rs-agent Shell Hook (Sub-Millisecond Startup)
_ai_bin="$HOME/.local/bin/ai"
[[ ! -x "$_ai_bin" ]] && _ai_bin="$HOME/.config/rs-agent/target/release/rs-agent"

ai() {
    "$_ai_bin" "$@"
}

_ai_handle_missing() {
    [[ -n "${_AI_CNF_ACTIVE:-}" ]] && return 127
    local _AI_CNF_ACTIVE=1
    local raw_cmd="$*"
    [[ -z "$raw_cmd" ]] && return 127

    local matched
    matched=$("$_ai_bin" match "$raw_cmd" 2>/dev/null)
    local rc=$?

    if [[ $rc -eq 0 && -n "$matched" ]]; then
        if [[ "$matched" == "ai init "* ]]; then
            local target_dir="${matched#ai init }"
            target_dir="${target_dir/#\~/$HOME}"
            mkdir -p "$target_dir" 2>/dev/null
            cd "$target_dir" 2>/dev/null || return 1
            "$_ai_bin" init "$target_dir"
            return $?
        elif [[ "$matched" == "ai "* ]]; then
            local sub_cmd="${matched#ai }"
            "$_ai_bin" $sub_cmd
            return $?
        else
            eval "$matched"
            return $?
        fi
    fi

    if declare -F _orig_ai_cnf >/dev/null 2>&1; then
        _orig_ai_cnf "$@"
        return $?
    fi

    return 127
}

if [[ -n "$ZSH_VERSION" ]]; then
    if [[ -z "${_AI_HOOK_SET:-}" ]]; then
        _AI_HOOK_SET=1
        if (( $+functions[command_not_found_handler] )); then
            functions[_orig_ai_cnf]=$functions[command_not_found_handler]
        fi
    fi
    command_not_found_handler() { _ai_handle_missing "$@"; }
elif [[ -n "$BASH_VERSION" ]]; then
    if [[ -z "${_AI_HOOK_SET:-}" ]]; then
        _AI_HOOK_SET=1
        if declare -F command_not_found_handle >/dev/null 2>&1; then
            _orig_def=$(declare -f command_not_found_handle)
            eval "_orig_ai_cnf() ${_orig_def#*$'\n'}"
            unset _orig_def
        fi
    fi
    command_not_found_handle() { _ai_handle_missing "$@"; }
fi
