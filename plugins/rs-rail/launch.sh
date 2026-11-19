#!/usr/bin/env bash
set -euo pipefail

PLUGIN_DIR="$HOME/.config/rs-agent/plugins/rs-rail"

# Auto-register XDG desktop entry & icon for current user on launch
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$DESKTOP_DIR" "$ICON_DIR"

if [ ! -f "$DESKTOP_DIR/rs-rail.desktop" ] || ! grep -q "$PLUGIN_DIR" "$DESKTOP_DIR/rs-rail.desktop" 2>/dev/null; then
    cat <<DESKTOP > "$DESKTOP_DIR/rs-rail.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=rs-rail
GenericName=AI Agent Surface
Comment=Native Rust web and desktop surface for rs-agent
Exec=$PLUGIN_DIR/launch.sh
Icon=$PLUGIN_DIR/../../assets/rs-rail.svg
Terminal=false
StartupNotify=true
Categories=Development;IDE;Utility;
Keywords=ai;agent;llm;rust;rs-agent;rail;
DESKTOP
    ln -sf "$PLUGIN_DIR/../../assets/rs-rail.svg" "$ICON_DIR/rs-rail.svg"
    update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
fi

BIN="$PLUGIN_DIR/target/release/rs-rail"
PORT="${RS_RAIL_PORT:-24243}"
MODE="app"
TARGET_PATH="$PWD"

for arg in "$@"; do
    case "$arg" in
        web|--web) MODE="web" ;;
        *) [ -d "$arg" ] && TARGET_PATH="$arg" ;;
    esac
done

if [ ! -f "$BIN" ]; then
    cargo build --release --manifest-path "$PLUGIN_DIR/Cargo.toml"
fi

if ! curl -sf "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1; then
    nohup "$BIN" >"${XDG_RUNTIME_DIR:-/tmp}/rs-rail-$(id -u).log" 2>&1 &
    for _ in $(seq 1 20); do
        curl -sf "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && break
        sleep 0.1
    done
fi

PROJ=""
case "$TARGET_PATH" in
    "$HOME/.config/rs-agent/projects/"*)
        PROJ="${TARGET_PATH#$HOME/.config/rs-agent/projects/}"
        PROJ="${PROJ%%/*}"
        ;;
esac

URL="http://127.0.0.1:$PORT/${PROJ:+?project=$PROJ}"

if [ "$MODE" = "app" ]; then
    if command -v omarchy-launch-webapp >/dev/null 2>&1; then
        nohup setsid omarchy-launch-webapp "$URL" >/dev/null 2>&1 </dev/null &
        exit 0
    fi
    if command -v xdg-settings >/dev/null 2>&1; then
        DESKTOP=$(xdg-settings get default-web-browser 2>/dev/null || true)
        if [ -n "$DESKTOP" ]; then
            BIN_CMD=$(grep -hm1 '^Exec=' "$HOME/.local/share/applications/$DESKTOP" "/usr/share/applications/$DESKTOP" 2>/dev/null | head -n1 | sed 's/^Exec=//; s/ *%[a-zA-Z].*//; s/"//g' | awk '{print $1}' || true)
            case "${BIN_CMD##*/}" in
                *chrom*|*brave*|*vivaldi*|*edge*|*thorium*|*helium*|*opera*)
                    if command -v "$BIN_CMD" >/dev/null 2>&1; then
                        nohup setsid "$BIN_CMD" --app="$URL" --new-window >/dev/null 2>&1 </dev/null &
                        exit 0
                    fi
                    ;;
            esac
        fi
    fi
    for browser in chromium chromium-browser google-chrome google-chrome-stable brave brave-bin brave-browser brave-origin brave-origin-bin vivaldi vivaldi-stable microsoft-edge thorium-browser ungoogled-chromium; do
        if command -v "$browser" >/dev/null 2>&1; then
            nohup setsid "$browser" --app="$URL" --new-window >/dev/null 2>&1 </dev/null &
            exit 0
        fi
    done
fi

if command -v xdg-open >/dev/null 2>&1; then
    nohup xdg-open "$URL" >/dev/null 2>&1 </dev/null &
elif command -v open >/dev/null 2>&1; then
    open "$URL"
else
    echo "Open: $URL"
fi
