# T3 Code Plugin (`rs-agent`)

A native Rust Agent Client Protocol (ACP) stdio bridge connecting the `rs-agent` execution kernel directly into the [T3 Code](https://t3.codes) desktop and web IDE.

Supports **Omarchy** (self-updating AppImage), **Arch Linux / AUR** (`t3code-bin`), **macOS** (`T3 Code.app`), and standalone AppImages.

---

## 1. Automated 1-Step Setup

Run this one-liner to register `Rs-Agent` as the default engine in your T3 Code settings. It automatically configures the ACP bridge, sets `Rs-Agent` as the default model for new threads, disables telemetry, and preserves your active theme (including Omarchy's theme):

```bash
python3 - << 'PY_SETUP'
import os
import json

path = os.path.expanduser("~/.t3/userdata/settings.json")
os.makedirs(os.path.dirname(path), exist_ok=True)

data = {}
if os.path.isfile(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception:
        pass

bridge_bin = os.path.expanduser("~/.config/rs-agent/plugins/t3code/t3code-bridge")

# 1. Register Rs-Agent in providers list
providers = data.setdefault("providers", [])
existing = next((p for p in providers if isinstance(p, dict) and "rs-agent" in str(p.get("name", "")).lower()), None)
if existing:
    existing["command"] = bridge_bin
    existing["args"] = []
else:
    providers.append({
        "id": "rs_agent",
        "name": "Rs-Agent",
        "type": "acp",
        "command": bridge_bin,
        "args": []
    })

# 2. Mitigate upstream T3 bootstrap crash by keeping codex driver instance registered
inst = data.setdefault("providerInstances", {})
inst["codex"] = {"driver": "codex", "enabled": True}

# 3. Set Rs-Agent as the permanent default model for new threads
data["defaultModel"] = {"provider": "rs_agent", "model": "rs-agent"}
data["defaultModelSelection"] = {"provider": "rs_agent", "instanceId": "rs_agent", "model": "rs-agent"}

threads = data.setdefault("threads", {})
threads["defaultModel"] = "rs-agent"
threads["defaultProvider"] = "rs_agent"
threads["defaultModelSelection"] = {"provider": "rs_agent", "instanceId": "rs_agent", "model": "rs-agent"}

# 4. Enforce telemetry off
data["telemetry"] = False
data["telemetryEnabled"] = False

with open(path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)

print("\033[1;32m✓ Rs-Agent successfully registered as default engine in ~/.t3/userdata/settings.json\033[0m")
PY_SETUP
```

### Manual Configuration (Optional)
If configuring via the T3 Code GUI:
1. Open T3 Code $\rightarrow$ **Settings** (`Ctrl + ,`) $\rightarrow$ **Providers** $\rightarrow$ **Add Provider**.
2. **ACP Source:** `Local command`
3. **Display Name:** `Rs-Agent`
4. **Executable:** `~/.config/rs-agent/plugins/t3code/t3code-bridge`
5. **Arguments:** *(Leave blank)*

---

## 2. In-Session Usage

You can switch to T3 Code directly from an interactive `rs-agent` session or launch it independently:

| Invocation | Surface | Behavior |
| :--- | :---: | :--- |
| `❯ /t3` | **Desktop App** | Launches the desktop window via `uwsm` and hands over the active workspace. |
| `❯ /t3 web` | **Browser IDE** | Launches the headless web server on `http://127.0.0.1:3773` with zero telemetry. |
| **Omarchy Menu** (`Super + Space`) | **Desktop App** | Launches T3 Code directly from the app launcher; automatically uses `rs-agent`. |

The launcher exports `AI_WORKSPACE_PATH` and `AI_ACTIVE_PROFILE` automatically on CLI handover.

---

## 3. Workspace Isolation & Tool Behavior

`rs-agent` enforces strict profile and tool boundaries based on the workspace folder opened in T3 Code:

| Workspace Opened | Adopted Profile | Execution Mode | Tool Access |
| :--- | :---: | :---: | :--- |
| **`projects/chat`** | `chat` | Conversational Partner | **0 Tools (Disabled)**. Responds conversationally without tool execution. |
| **`projects/ling-tiny`** | `lingtiny` | Autonomous Workspace Agent | **Native 6-Tools Active** (`read_file`, `edit_file`, `write_file`, `list_dir`, `search_code`, `run_command`). |
| **`projects/groot`** | `groot` | Autonomous Systems Architect | **Native 6-Tools Active** with MTP deep reasoning. |

---

## 4. Architecture & Wire Protocol

```text
  T3 Code (Monaco Editor / Diff Viewer / Settings)
       │
       │  ACP (Agent Client Protocol) JSON-RPC 2.0 (stdio)
       ▼
  ~/.config/rs-agent/plugins/t3code/t3code-bridge
       │
       │  exec ai --bridge "$@" (Zero binary drift)
       ▼
  rs-agent Engine
  • LlmClient (raw SSE streaming with model reasoning normalization)
  • Dynamic Reasoning Effort Dial (Off, Low, Medium, High, Unrestricted)
  • Native 6-Tool suite (read, edit, write, list, search, run)
  • Zero-trust SecurityBoundary (symlink-aware path confinement)
  • Edge SLM argument repair & stack-based JSON healer
```

### Protocol Capabilities
* **`initialize`**: ACP handshake declaring protocol version, image capabilities, and agent info.
* **`session/new` / `createSession`**: Resolves workspace path, loads `.rs/config.json`, binds profile frontmatter (`adapters`, `tier`, `delegate`), and injects `.rs/memory.md`.
* **`session/prompt`**: Streams thinking blocks (`agent_thought_chunk`) and response body tokens (`agent_message_chunk`).
* **`session/set_config_option`**: Intercepts T3's reasoning effort slider (`thought_level`) and clamps or expands token budget mid-turn.
* **`session/request_permission`**: Dispatches live split/unified diff previews for human review before executing mutating actions (`write_file`, `edit_file`, `run_command`).
* **`session/cancel`**: Atomic cancellation terminates in-flight LLM streams without deadlocking subprocess pipes.

---

## 5. Binary Discovery & Compatibility

`launch.sh` auto-detects T3 Code across multiple installation types:

1. **Omarchy Self-Updating App**:
   - Desktop Binary: `~/.local/share/t3code/T3-Code.AppImage`
   - CLI Helper: `~/.t3/bin/t3`
2. **Arch Linux / AUR / Package Manager**:
   - CLI: `/usr/bin/t3code` or `/usr/bin/t3`
3. **macOS**:
   - `/Applications/T3 Code.app` or `~/Applications/T3 Code.app`
4. **Wayland Compositor Detachment**:
   - Uses `uwsm app --` on Omarchy / Arch Wayland, `systemd-run` on systemd Linux, and `open -a` on macOS so closing the terminal never kills the IDE window.

---

## 6. Troubleshooting

### "T3 Code binary not found"
Ensure your T3 Code AppImage or binary is executable:
```bash
chmod +x ~/.local/share/t3code/T3-Code.AppImage 2>/dev/null || true
```

### "Codex is unauthenticated" / Startup Crash Loop
T3 Code's initial project bootstrap routine has an upstream bug where it invokes Codex if no default model is defined. Run the **Automated Setup** script above to set `Rs-Agent` as the default model and enable the background provider stub.

### Telemetry & Privacy
Telemetry is disabled by default in `launch.sh` by exporting:
```bash
export T3CODE_TELEMETRY_ENABLED=false
export DO_NOT_TRACK=1
```
