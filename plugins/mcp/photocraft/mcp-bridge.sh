#!/usr/bin/env bash
set -euo pipefail

HOST="127.0.0.1"
PORT="7878"
TOKEN_FILE="${HOME}/.config/photocraft/control.token"

port_open() {
    (exec 3<>"/dev/tcp/${HOST}/${PORT}") 2>/dev/null
}

if ! port_open && [[ "${PHOTOCRAFT_AUTOSTART:-0}" == "1" ]]; then
    nohup setsid photocraft \
        --control "$PORT" \
        --control-token-file "$TOKEN_FILE" \
        --automation-read-root "$HOME" \
        --automation-write-root "$HOME" \
        </dev/null >/dev/null 2>&1 &

    for _ in {1..20}; do
        port_open && break
        sleep 0.1
    done
fi

if port_open; then
    exec photocraft-cli mcp \
        --bridge "${HOST}:${PORT}" \
        --control-token-file "$TOKEN_FILE"
else
    exec photocraft-cli mcp \
        --automation-read-root "$HOME" \
        --automation-write-root "$HOME"
fi
