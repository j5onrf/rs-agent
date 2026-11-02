# T3 Code Plugin (`rs-agent`)

A native Rust Agent Client Protocol (ACP) stdio bridge connecting the `rs-agent` engine directly into the [T3 Code](https://t3.codes) desktop and web IDE.

---

## 1. Automated Setup

Run this command to automatically add `Rs-Agent` alongside `Py-Agent` in your T3 Code settings without modifying existing providers:

```bash
python3 -c '
import json, os, copy

path = os.path.expanduser("~/.t3/userdata/settings.json")
if not os.path.exists(path):
    print("Settings file not found.")
    exit(0)

with open(path, "r") as f:
    data = json.load(f)

rs_bin = os.path.expanduser("~/.config/rs-agent/plugins/t3code/t3code-bridge")

def duplicate_provider(obj):
    if isinstance(obj, list):
        py_item = next((item for item in obj if isinstance(item, dict) and any("Py-Agent" in str(v) for v in item.values())), None)
        if py_item:
            has_rs = any(isinstance(x, dict) and any("Rs-Agent" in str(v) for v in x.values()) for x in obj)
            if not has_rs:
                rs_item = copy.deepcopy(py_item)
                def update_node(d):
                    for k, v in list(d.items()):
                        if isinstance(v, str):
                            if "py-agent" in v and "t3code" in v: d[k] = rs_bin
                            elif v == "Py-Agent": d[k] = "Rs-Agent"
                            elif v == "py_agent": d[k] = "rs_agent"
                        elif isinstance(v, dict): update_node(v)
                        elif isinstance(v, list):
                            for elem in v:
                                if isinstance(elem, dict): update_node(elem)
                update_node(rs_item)
                obj.append(rs_item)
                return True
        for elem in obj:
            if duplicate_provider(elem): return True
    elif isinstance(obj, dict):
        for v in obj.values():
            if duplicate_provider(v): return True
    return False

duplicate_provider(data)

with open(path, "w") as f:
    json.dump(data, f, indent=2)

print("\033[1;32m✓ Rs-Agent registered alongside Py-Agent in ~/.t3/userdata/settings.json\033[0m")
'
```

### Manual Configuration
In T3 Code $\rightarrow$ **Settings** $\rightarrow$ **Providers** $\rightarrow$ **Add Provider**:
* **ACP source:** `Local command`
* **Display name:** `Rs-Agent`
* **Executable:** `/home/j5/.config/rs-agent/plugins/t3code/t3code-bridge` (or simply `t3code-bridge`)
* **Arguments:** *(Leave empty)*

---

## 2. In-Session Usage

Switch to T3 Code directly from your interactive `rs-agent` session:

| Command | Surface | Behavior |
| :--- | :---: | :--- |
| `❯ /t3` | **Desktop** | Hands active workspace and profile over to the desktop app. |
| `❯ /t3 web` | **Browser** | Launches the headless web server and opens the browser IDE. |

The launcher exports `AI_WORKSPACE_PATH` and `AI_ACTIVE_PROFILE` automatically.

---

## 3. Architecture & Wire Protocol

```text
  T3 Code (Monaco Editor / Diff Viewer)
       │
       │  ACP (Agent Client Protocol) JSON-RPC 2.0 (stdio)
       ▼
  ~/.config/rs-agent/plugins/t3code/t3code-bridge
       │
       │  Direct Rust library linkage (rs_agent)
       ▼
  rs-agent Engine
  • LlmClient (raw SSE event stream)
  • Smol 6-Tool suite (read, edit, write, list, search, run)
  • Zero-trust SecurityBoundary
  • SLM JSON Repair Adapters
```

### Protocol Handling
* **`initialize`:** ACP handshake exposing protocol version and session capabilities.
* **`session/new` / `createSession`:** Registers active workspace and loads `.rs/memory.md` (if enabled in profile).
* **`session/prompt`:** Asynchronously streams response chunks:
  * Reasoning tokens emitted as `agent_thought_chunk`.
  * Response body tokens emitted as `agent_message_chunk`.
  * Tool calls executed via `rs_agent::tools::execute_tool` inside a non-blocking task.
* **`session/cancel`:** Atomic cancellation flag terminates in-flight streams without IPC deadlocks.

---

## 4. Rebuilding the Bridge

```bash
cd ~/.config/rs-agent
cargo build --release
cp target/release/t3code-bridge plugins/t3code/t3code-bridge
strip plugins/t3code/t3code-bridge
