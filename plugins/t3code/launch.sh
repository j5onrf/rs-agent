#!/usr/bin/env bash
set -eo pipefail

PLUGIN_DIR="$HOME/.config/rs-agent/plugins/t3code"
BRIDGE_BIN="$PLUGIN_DIR/t3code-bridge"

T3_BIN="$(command -v t3code || command -v t3 || true)"
T3_CLI="$HOME/.t3/bin/t3"
[[ -x "$T3_CLI" ]] || T3_CLI="$(command -v t3 || true)"

if [[ -z "$T3_BIN" ]]; then
    echo -e "\033[1;31m[t3code]\033[0m Error: 't3code' binary not found in PATH." >&2
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
export PYAGENT_BRIDGE="$BRIDGE_BIN"
export T3CODE_HOME="${T3CODE_HOME:-$HOME/.t3}"

cd "$TARGET_WORKSPACE"

if [[ "$MODE" == "web" ]]; then
    if [[ -x "$T3_CLI" ]]; then
        exec "$T3_CLI" "$TARGET_WORKSPACE"
    else
        exec "$T3_BIN" --web "$TARGET_WORKSPACE"
    fi
fi

T3_NAME="$(basename "$T3_BIN")"
if [[ -x "$T3_CLI" ]] && pgrep -x "$T3_NAME" &>/dev/null; then
    if "$T3_CLI" app "$TARGET_WORKSPACE" &>/dev/null; then
        echo -e "\033[1;32m[t3code]\033[0m Handed workspace '$TARGET_WORKSPACE' to active T3 Code window."
        exit 0
    fi
fi

T3_PID=""
HANDOVER_PID=""
_CLEANED=0

cleanup() {
    [[ "$_CLEANED" -eq 1 ]] && return 0
    _CLEANED=1

    [[ -n "$HANDOVER_PID" ]] && kill -TERM "$HANDOVER_PID" 2>/dev/null || true
    if [[ -n "$T3_PID" ]] && kill -0 "$T3_PID" 2>/dev/null; then
        kill -TERM "$T3_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

"$T3_BIN" &
T3_PID=$!

if [[ -x "$T3_CLI" ]]; then
    (
        for _ in {1..30}; do
            sleep 0.2
            if "$T3_CLI" app "$TARGET_WORKSPACE" &>/dev/null; then
                break
            fi
        done
    ) &
    HANDOVER_PID=$!
fi

wait "$T3_PID"
