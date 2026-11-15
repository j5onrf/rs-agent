# PhotoCraft MCP Integration

Headless and live-bridge image editing engine for `rs-agent` via the Model Context Protocol (MCP).

---

## 1. Installation

### Arch Linux / CachyOS (AUR)
```bash
paru -S photocraft
# or
yay -S photocraft
```

### Build from Source
```bash
git clone https://github.com/storytold/photocraft.git /tmp/photocraft
cd /tmp/photocraft
cargo build --release -p photocraft -p photocraft-cli
install -m 755 target/release/photocraft target/release/photocraft-cli ~/.local/bin/
cd ~ && rm -rf /tmp/photocraft
```

---

## 2. Authentication Token Setup

PhotoCraft requires a 64-character hex control token for socket authentication:

```bash
mkdir -p ~/.config/photocraft
openssl rand -hex 32 > ~/.config/photocraft/control.token
chmod 600 ~/.config/photocraft/control.token
```

---

## 3. Launcher & Desktop Bridge (`mcp-bridge.sh`)

The included launcher script (`plugins/mcp/photocraft/mcp-bridge.sh`) automatically coordinates between live GUI mode and headless CLI execution:

- **Live Bridge (GUI):** If PhotoCraft is running with `--control 7878`, the launcher bridges directly to the open GUI window, enabling real-time canvas manipulation, live layer inspection, and `ui_set` / `ui_screenshot`.
- **Autostart:** In `plugins/mcp/servers.json`, set `"PHOTOCRAFT_AUTOSTART": "1"` under `photocraft.env`. The launcher will automatically spawn PhotoCraft in a detached background session (`nohup setsid`) so your canvas stays open even after exiting `ai`.
- **Headless Fallback:** If PhotoCraft is not running and autostart is `"0"`, the launcher runs `photocraft-cli mcp` headless with file system boundaries scoped to `${HOME}`.

---

## 4. Profile Configuration & Token Scaling

PhotoCraft exports 26 tools (~7.2k tokens of raw schema). Configure profiles based on model context capacity:

### A. Cloud Models (Gemini, Claude, GPT-4o) — `profiles/artcraft/photocraft-c.md`
Full tool suite with unconstrained context:
```yaml
---
description: "PhotoCraft Visual Engine (Cloud)"
mcp: photocraft             # Loads all 26 tools
reasoning_budget: 256
---
```

### B. Local SLMs (Ling-3.0-tiny, LFM3-1.7B on 8k) — `profiles/artcraft/photocraft-l.md`
Minimal schema using `purebash: true` and an `mcp_tools:` allowlist to keep prefill under ~3.9k tokens:
```yaml
---
description: "PhotoCraft Automation Engineer (Local SLM)"
purebash: true              # Minimal 1-tool base schema (~95t)
mcp: photocraft
mcp_tools: doc_new, doc_open, doc_inspect, doc_save, command_run, command_list, ui_set
reasoning_budget: 256
---
```

---

## 5. Verification

Test the connection and verify loaded tools:

```bash
ai mcp list photocraft
ai mcp call photocraft doc_new '{"name":"test.psd","width":800,"height":600}'
```
