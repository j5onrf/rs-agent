#!/usr/bin/env bash
set -uo pipefail

export T3CODE_TELEMETRY_ENABLED=false
export DO_NOT_TRACK=1

PLUGIN_DIR="$HOME/.config/rs-agent/plugins/t3code"
BRIDGE_BIN="$PLUGIN_DIR/t3code-bridge"
[[ -x "$BRIDGE_BIN" ]] || BRIDGE_BIN="$HOME/.config/rs-agent/bin/ai"

T3_APPIMAGE="$HOME/.local/share/t3code/T3-Code.AppImage"

# Prioritize the actual AppImage binary over the stale CLI wrapper
T3_BIN=""
if [[ -x "$T3_APPIMAGE" ]]; then
    T3_BIN="$T3_APPIMAGE"
elif [[ "$(uname -s)" == "Darwin" ]]; then
    for mac_app in "/Applications/T3 Code.app" "$HOME/Applications/T3 Code.app"; do
        if [[ -d "$mac_app" ]]; then
            T3_BIN="$mac_app"
            break
        fi
    done
else
    T3_BIN="$(command -v t3code || command -v t3 || true)"
fi

if [[ -z "$T3_BIN" ]]; then
    echo -e "\033[1;31m[t3code]\033[0m Error: T3 Code AppImage or binary not found." >&2
    exit 1
fi

MODE="desktop"
WORKSPACE_ARG=""

for arg in "$@"; do
    case "$arg" in
        web|--web|--browser)
            MODE="web"
            ;;
        *)
            if [[ -z "$WORKSPACE_ARG" ]]; then
                WORKSPACE_ARG="$arg"
            fi
            ;;
    esac
done

RAW_WORKSPACE="${WORKSPACE_ARG:-${AI_WORKSPACE_PATH:-$PWD}}"
if ! TARGET_WORKSPACE=$(cd "$RAW_WORKSPACE" 2>/dev/null && pwd -P); then
    TARGET_WORKSPACE="$PWD"
fi

if [[ -f "$BRIDGE_BIN" ]]; then
    chmod +x "$BRIDGE_BIN" 2>/dev/null || true
fi

export AI_WORKSPACE_PATH="$TARGET_WORKSPACE"
export RSAGENT_BRIDGE="$BRIDGE_BIN"
export AI_INSTANCE_ID="${AI_INSTANCE_ID:-1}"
export T3CODE_HOME="${T3CODE_HOME:-$HOME/.t3}"

cd "$TARGET_WORKSPACE"

# Web Mode: Starts headless server via AppImage --web
if [[ "$MODE" == "web" ]]; then
    if [[ -d "$T3_BIN" && "$(uname -s)" == "Darwin" ]]; then
        open -a "$T3_BIN" --args --web "$TARGET_WORKSPACE"
        exit 0
    else
        exec "$T3_BIN" --web "$TARGET_WORKSPACE"
    fi
fi

# IPC handover if window is already active
if pgrep -f "(^|/)(t3code|T3 Code|T3-Code)(\s|$)" 2>/dev/null | grep -vw -e "$$" -e "$PPID" &>/dev/null; then
    T3_CLI="$HOME/.t3/bin/t3"
    if [[ -x "$T3_CLI" ]] && "$T3_CLI" app "$TARGET_WORKSPACE" &>/dev/null; then
        echo -e "\033[1;32m[t3code]\033[0m Handed workspace '$TARGET_WORKSPACE' to active T3 Code window."
        exit 0
    fi
fi

# Detached Desktop Launch (Omarchy uwsm / systemd / background)
launch_detached() {
    local bin="$1"
    local ws="$2"

    if command -v uwsm >/dev/null 2>&1; then
        uwsm app -- "$bin" "$ws" >/dev/null 2>&1 &
    elif command -v systemd-run >/dev/null 2>&1 && [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
        systemd-run --user --quiet "$bin" "$ws" >/dev/null 2>&1 || true
    elif [[ "$(uname -s)" == "Darwin" ]]; then
        open -a "$bin" --args "$ws" >/dev/null 2>&1 || ( "$bin" "$ws" >/dev/null 2>&1 & )
    else
        nohup "$bin" "$ws" >/dev/null 2>&1 &
    fi
    disown -a 2>/dev/null || true
}

launch_detached "$T3_BIN" "$TARGET_WORKSPACE"
echo -e "\033[1;32m[t3code]\033[0m Launched T3 Code desktop window for '$TARGET_WORKSPACE'."
exit 0
