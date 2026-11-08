#!/usr/bin/env bash
set -uo pipefail

PLUGIN_DIR="$HOME/.config/rs-agent/plugins/t3code"
BRIDGE_BIN="$HOME/.config/rs-agent/plugins/t3code/t3code-bridge"
[[ -x "$BRIDGE_BIN" ]] || BRIDGE_BIN="$HOME/.config/rs-agent/bin/ai"

# Discover T3 Binary (macOS .app + Linux CLI)
T3_BIN="$(command -v t3code || command -v t3 || true)"

if [[ -z "$T3_BIN" && "$(uname -s)" == "Darwin" ]]; then
    for mac_app in "/Applications/T3 Code.app" "$HOME/Applications/T3 Code.app"; do
        if [[ -d "$mac_app" ]]; then
            T3_BIN="$mac_app"
            break
        fi
    done
fi

T3_CLI="$HOME/.t3/bin/t3"
[[ -x "$T3_CLI" ]] || T3_CLI="$(command -v t3 || true)"

if [[ -z "$T3_BIN" ]]; then
    echo -e "\033[1;31m[t3code]\033[0m Error: 't3code' not found in PATH or Applications." >&2
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
    echo -e "\033[1;33m[t3code]\033[0m Warning: Directory '$RAW_WORKSPACE' not found. Falling back to '$PWD'." >&2
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

if [[ "$MODE" == "web" ]]; then
    if [[ -x "$T3_CLI" ]]; then
        exec "$T3_CLI" "$TARGET_WORKSPACE"
    elif [[ -d "$T3_BIN" && "$(uname -s)" == "Darwin" ]]; then
        open -a "$T3_BIN" --args --web "$TARGET_WORKSPACE"
        exit 0
    else
        exec "$T3_BIN" --web "$TARGET_WORKSPACE"
    fi
fi

# 1. If T3 Code is already open, quietly hand over via IPC
# Filter out $$ and $PPID to prevent self-match on .../plugins/t3code/launch.sh
if [[ -x "$T3_CLI" ]]; then
    if pgrep -f "(^|/)(t3code|T3 Code)(\s|$)" 2>/dev/null | grep -vw -e "$$" -e "$PPID" &>/dev/null; then
        if "$T3_CLI" app "$TARGET_WORKSPACE" &>/dev/null; then
            echo -e "\033[1;32m[t3code]\033[0m Handed workspace '$TARGET_WORKSPACE' to active T3 Code window."
            exit 0
        fi
    fi
fi

# 2. Universal Detachment: uwsm on Omarchy/Arch, open on macOS, systemd-run on Linux
launch_detached() {
    local bin="$1"
    local ws="$2"

    if [[ "$(uname -s)" == "Darwin" ]]; then
        # macOS: native launchd via open
        if [[ -d "$bin" ]]; then
            open -a "$bin" --args "$ws" >/dev/null 2>&1 || true
        elif command -v open >/dev/null 2>&1; then
            open -a "$bin" --args "$ws" >/dev/null 2>&1 || ( "$bin" "$ws" >/dev/null 2>&1 & )
        else
            ( "$bin" "$ws" >/dev/null 2>&1 & )
        fi
    elif command -v uwsm >/dev/null 2>&1; then
        # Omarchy / CachyOS / Arch Wayland: Native session manager app daemon
        uwsm app -- "$bin" "$ws" >/dev/null 2>&1 &
    elif command -v systemd-run >/dev/null 2>&1 && [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
        # Linux systemd: Transient user service (parent is PID 1)
        systemd-run --user --quiet "$bin" "$ws" >/dev/null 2>&1 || true
    elif command -v setsid >/dev/null 2>&1; then
        setsid -f "$bin" "$ws" >/dev/null 2>&1 || true
    else
        nohup "$bin" "$ws" >/dev/null 2>&1 &
    fi
    disown -a 2>/dev/null || true
}

launch_detached "$T3_BIN" "$TARGET_WORKSPACE"

# 3. Wait briefly for IPC handshake readiness, then confirm cleanly
handshake_ok=false
if [[ -x "$T3_CLI" ]]; then
    for _ in {1..30}; do
        sleep 0.15
        if "$T3_CLI" app "$TARGET_WORKSPACE" &>/dev/null; then
            handshake_ok=true
            break
        fi
    done
fi

if [[ "$handshake_ok" == true ]]; then
    echo -e "\033[1;32m[t3code]\033[0m Handed workspace '$TARGET_WORKSPACE' to active T3 Code window."
else
    echo -e "\033[1;32m[t3code]\033[0m Launched T3 Code in background for '$TARGET_WORKSPACE'."
fi
exit 0
