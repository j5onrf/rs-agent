<div align="center">
  <img alt="rs-agent" src="logo.svg?v=2" height="230" />
  <h1>rs-agent</h1>
 
  <p>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/version-v0.4.9.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=branded&brand=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/rs-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-green.svg" alt="License"></a>
    <a href="https://shieldcn.dev/badge/status-beta-blue.svg"><img src="https://shieldcn.dev/badge/status-beta-blue.svg" alt="Status"></a>
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>t3code</code> &nbsp;•&nbsp; <code>omarchy</code> &nbsp;•&nbsp; <code>local-first</code>
  </p>
  </p>

  <p>
    <b>A Rust AI harness built for low-latency local <code>llama-server</code> C++ backends, slm/llm gguf, and cloud endpoints.</b><br>
    <sub>Sub-millisecond boot, autonomous 6-tool execution loop, zero-trust security kernel, and edge-model JSON repair.</sub>
  </p>

  <br>

  <table>
    <tr>
      <td align="center" width="28%" valign="top">
        <h3>👤 Sub-27B (SLM)</h3>
        <p><sub>Tool calling, fast refactoring, and single-turn edits</sub></p>
        <code>LFM3-1.7B*</code> &nbsp;•&nbsp; <code>Ling-3.0-tiny</code><br>
        <code>MiniCPM5-2B</code> &nbsp;•&nbsp; <code>Qwen3.5-2B+</code><br>
        <code>MiniCPM-V-4.7-1B</code> &nbsp;•&nbsp; <code>LFM2.5-24B-A2B</code>
      </td>
      <td align="center" width="28%" valign="top">
        <h3>👤 27B+ (LLM)</h3>
        <p><sub>Deep reasoning, complex logic, and multi-turn loops</sub></p>
        <code>Sage-A1.5-35B</code> &nbsp;•&nbsp; <code>Flint-40B-A2.5B*</code><br>
        <code>Qwen3.8-35B-D</code> &nbsp;•&nbsp; <code>KAT-Coder-V2.5</code><br>
        <code>Ornith-1.5-35B</code> &nbsp;•&nbsp; <code>Tiel-Coder-35B</code><br>
        <code>Qwen3.8-27B</code> &nbsp;•&nbsp; <code>Qwen4.0-27B</code><br>
        <code>Qwen3.8-Flash</code> &nbsp;•&nbsp; <code>DeepSeek-V4.1</code><br>
        <br>
      </td>
    </tr>
  </table>
  
  <p>
    * Primary edge baselines &nbsp;•&nbsp; Auto-discovers local models loaded on <code>http://127.0.0.1:8080</code>
  </p>

  <p>
    <sub><b>Cloud Endpoints:</b> <a href="https://ai.google.dev">Google Gemini</a>, <a href="https://openrouter.ai">OpenRouter</a>, and <a href="https://tokenharbor.ai">TokenHarbor</a><br>
    <b>Free Endpoints:</b> <a href="https://ai.google.dev"><code>gemini-3.5-flash-lite</code></a> (Google AI Studio) &nbsp;•&nbsp; <a href="https://openrouter.ai/models?variant=free"><code>openrouter/free</code></a> (OpenRouter) &nbsp;•&nbsp; <a href="https://tokenharbor.ai"><code>deepseek-v4.1-flash:free</code></a> (TokenHarbor)</sub>
  </p>
</div><br>

---

```console
~ ❯ ai
╭─ ○ ────────────────────────────────────────╮
│  LFM3-1.7B                                 │
│  chat                                      │
╰────────────────────────────────────────────╯

· · ○ · · · · · · · · · · · · · · · · · · · · ·
·  LFM3-1.7B  · · · · · · · · · · · · · · · · ·
·  chat       · · · · · · · · · · · · · · · · ·
· · · · · · · · · · · · · · · · · · · · · · · ·

·  LFM3-1.7B  · · · · · · · · · · · · · · · · ·
·  chat       · · · · · · · · · · · · · · · · ·

❯ █
```

<div align="center">
  <p><sub>Switch terminal styles with <code>/box [1-3]</code> · Progressive 350ms wake-on-key reveal</sub></p>
</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Client Surfaces</h2>

<p align="center">
  <b><a href="https://t3.codes">T3 Code</a> is the official graphical surface for rs-agent.</b><br>
  <sub>A zero-overhead pairing connecting rs-agent's native Rust execution kernel directly to T3 Code via stdio ACP (Agent Client Protocol).</sub>
</p>

<div align="center">
  <table>
    <tr>
      <td align="center" width="220" valign="top">
        <br>
        <h3>Seamless Handover</h3>
        <p>👤 <code>/t3</code> · <code>/t3 web</code></p>
        <sub>Instant desktop or browser launch. Conversational chat isolates to <code>projects/chat</code>; agent sessions inherit workspace root.</sub>
        <br><br>
      </td>
      <td align="center" width="220" valign="top">
        <br>
        <h3>Interactive Diff Review</h3>
        <p><code>session/request_permission</code></p>
        <sub>Full support for T3's 4-tier security controls (<i>Supervised</i>, <i>Auto-accept edits</i>, <i>Auto</i>, <i>Full access</i>) with live split/unified diff previews.</sub>
        <br><br>
      </td>
      <td align="center" width="220" valign="top">
        <br>
        <h3>Dynamic Controls</h3>
        <p><code>$skills</code> · <code>/commands</code> · <code>thought_level</code></p>
        <sub>Autocomplete <code>$</code> skill pills with side-panel instruction viewer, <code>/task</code> goal loop, reasoning budget dials, and multimodal vision.</sub>
        <br><br>
      </td>
    </tr>
  </table>

  <p>
    <sub><b>Unified Binary Bridge:</b> Embedded JSON-RPC 2.0 ACP server runs natively from <code>ai --bridge</code> without duplicate compilation units or binary drift.</sub>
  </p>
</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Execution Modes</h2>

<div align="center">

| Invocation | Interface | Description |
| :--- | :---: | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-millisecond router via `rs-context.md` (e.g. `lfm`, `ling`) |
| 👤 `ai` | **Interactive Chat** | Conversational REPL with thinking gutters and live token stats |
| 👤 `ai init [path]` | **Workspace Agent** | Autonomous project workspace with tool execution & security kernel |
| `/task [goal]` | **Task Loop** | Autonomous goal loop; auto-discovers `TASK.md` when prompt is omitted |
| `/t3` / `/t3 web` | **T3 Code IDE** | Handover active workspace to official T3 Code GUI with diffs & security gates |
| `ai --bridge` | **Headless ACP** | Embedded stdio JSON-RPC 2.0 interface for IDEs and desktop surfaces |

</div>

<p align="center">
  <sub><code>/p</code> profile select &nbsp;•&nbsp; <code>/task</code> goal loop &nbsp;•&nbsp; <code>/t3</code> launch IDE &nbsp;•&nbsp; <code>/s</code> toggle skills &nbsp;•&nbsp; <code>/box</code> themes &nbsp;•&nbsp; <code>/r</code> reset</sub>
</p>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Developer Tool Suite</h2>

Standalone native Rust binaries deployed in `tools/` for zero-overhead project maintenance:

<div align="center">

| Binary | Tool | Purpose |
| :--- | :---: | :--- |
| 👤 `ms` | **Model Selector** | Dynamic TUI for toggling universal providers, context budget, and loop limit |
| `newp` | **Project Creator** | Scaffolds workspace, `.rs/config.json`, and auto-links router aliases |
| `idx` | **Codebase Indexer** | Generates token-dense Rust AST architecture maps (`index-map.txt`) |
| `ta` | **System Diagnostics** | 10-pass native verification suite testing binary integrity, safety, and UI |
| `eva` | **Agentic Benchmark** | Full-stack `eval-stack` measuring par efficiency and token economy |

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Runtime Architecture</h2>

`rs-agent` discards heavy agent frameworks in favor of simple, predictable primitives:

* **Sub-Millisecond Shell Intercept (`matcher.rs`):** `rs-hook.sh` intercepts terminal input and matches against `rs-context.md` in `< 0.2ms`. Typing `lfm` jumps into the target directory and launches the workspace instantly.
* **Native 6-Tool Suite (`tools.rs`):** Minimal tool surface ready for SLM and LLM: `read_file`, `edit_file` (2-stage exact + whitespace-tolerant replacer), `write_file`, `list_dir`, `search_code`, and `run_command`.
* **Embedded ACP Bridge (`bridge.rs`):** Stdio JSON-RPC 2.0 server inside the core binary. Dispatches live file diffs for T3 Code review, handles multimodal image attachments, and streams token cancellation mid-turn.
* **Autonomous Task Loop (`/task`):** Multi-step goal loop with failure decomposition (reminds models to inspect lines after tool errors) and anti-hallucination verification.
* **Edge Argument Healer (`adapter.rs`):** Small models often hallucinate parameter aliases (`filepath` vs `path`, `cmd` vs `command`) or unclosed quotes. An in-memory stack balancer normalizes keys and heals broken JSON frames on the fly.
* **Zero-Trust Security Boundary (`security.rs`):** Canonicalizes all paths inside workspace root. Traversal attempts (`../`) and mutating system calls (`sudo`, `pacman`, `pip`, `systemctl`, destructive `rm`) fail closed and require explicit approval.

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Efficiency & Token Footprint</h2>

<div align="center">

| Operational Tier | Token Budget | Runtime Footprint | Target Workload |
| :--- | :---: | :---: | :--- |
| **Pure Chat** (`ai`) | **~180 tokens** | < 1ms startup · 0 MB idle | Conversational queries and fast reasoning |
| **Pure Bash** (`purebash: true`) | **~95 tokens** | Single-tool dispatch loop | Low-parameter shell commands |
| **Native 6-Tool** *(Default)* | **~680 tokens** | Full surgical suite | Code refactoring, test execution, and file search |
| **Native 7-Tool** (`delegate: true`) | **~780 tokens** | Sub-agent recursion barrier | Complex code research & isolated sub-tasks |
| **Context Router** (`matcher.rs`) | **0 tokens** | < 0.2ms shell lookup | Instant shortcut teleportation via `rs-context.md` |

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Agent Evaluation (<code>eval-stack</code>)</h2>

<p align="center">
  Decision-grade evaluation across 5-stage agentic challenges (<code>eval-stack</code>).<br>
  <sub>Results scale with tuning: select a model foundation and optimize for your workload via profile directives, sampling flags, and <code>adapters</code>.</sub>
</p>

<div align="center">

| Rank | Model | Par Eff | Agent Index |
| :---: | :--- | :---: | :---: |
| **1** | **Flint-40B-A2.5B** | **120.0%** | **100.0 (A+)** |
| **2** | **Sage-A1.5-35B** | **111.1%** | **100.0 (A+)** |
| **3** | **Qwen3.8-35B-Distill** *(MTP)* | **136.4%** | **92.2 (A)** |
| **—** | **LFM3-1.7B** *(SLM)* | **162.5%** | **100.0 (A+)** |
| **—** | **Ling-3.0-tiny** *(SLM)* | **130.0%** | **97.3 (A+)** |

<p><sub>Decision-grade evaluation via <code>eva</code>: 5-task autonomous suite measuring par efficiency, tool resilience, and token economy.</sub></p>

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Workspace Profiles & Skills</h2>

Define agent profiles and runtime behavior in Markdown frontmatter (`profiles/*.md`):

```markdown
---
description: "Edge Systems Engineer (Model Profile)"
adapters: true          # Tool argument repair (strips markdown fences, heals newlines)
reasoning_budget: 500   # Thinking tokens clamp: 0 (off), 500 (medium), or omit for unrestricted
memory: true            # Opt-in: injects workspace rules from .rs/memory.md if present
purebash: true          # Optional: single bash tool (~95t schema). Omit for Native 6-tool (~680t)
delegate: true          # Opt-in: adds delegate_task tool (~100t) for sub-agents. Omit for Native 6-tool
mcp: photocraft         # Connect MCP apps (e.g. photocraft, blender). Omit for pure 6-tool
---
```

* **Profiles (`profiles/`):** Model configurations and system personas (e.g. `lfm3.md`, `lingtiny.md`, `chat.md`). Run `newp` to scaffold new workspaces with interactive profile selection.
* **Skills (`skills/`):** Modular runtime directives toggled via `/s` or T3 Code's `$` picker (e.g. `ponytail.md` for YAGNI minimalism, `humanizer.md` for clean output, `ub-mode.md` for clean craftsmanship).
* **Workspace Memory (`.rs/memory.md`):** Optional, project-isolated rules loaded into context when `memory: true` is set in the profile frontmatter.
* **Global Rules (`system_instructions.md`):** Universal directives loaded across every profile and workspace.

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="46" /> Setup & Installation</h2>

### 1. 👤 Quick Start (Linux x86_64)

Pre-built native binaries are included. No Rust installation or build tools required:

```bash
# 1. Clone into config directory
git clone https://github.com/j5onrf/rs-agent.git ~/.config/rs-agent

# 2. Register shell hook
echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.bashrc
source ~/.config/rs-agent/rs-hook.sh
```

<details>
<summary><b>macOS / Build from Source</b></summary>

For macOS (Apple Silicon / Intel), ARM64, or custom modifications:

```bash
cd ~/.config/rs-agent
cargo build --release
mkdir -p bin ~/.local/bin
cp target/release/ai bin/ai
cp target/release/ai bin/rs-agent

# Build model selector (ms) and create global symlink
cargo build --release --package model-select
cp target/release/model-select bin/ms
ln -sf ~/.config/rs-agent/bin/ms ~/.local/bin/ms

echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.zshrc
source ~/.config/rs-agent/rs-hook.sh
```

</details>

### 2. 👤 Configure Providers (`.env`)

<details>
<summary><b>View Example (<code>~/.config/rs-agent/.env</code>)</b></summary>

```env
# Rs-Agent Provider Cascade (.env) — Priority: First active key | Switch with: ms
# Fallback: Local llama-server (:8080) when all cloud keys are commented with '#'.

# ── 1. Google Gemini ──────────────────────────────────────────────────────────
CUSTOM1_NAME="Google Gemini"
# CUSTOM1_API_KEY="AQ-your-key-here"
CUSTOM1_URL="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
CUSTOM1_MODEL="gemini-3.5-flash-lite"

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
# CUSTOM2_NAME="OpenRouter"
# CUSTOM2_API_KEY="sk-or-v1-your-key-here"
CUSTOM2_URL="https://openrouter.ai/api/v1/chat/completions"
CUSTOM2_MODEL="openrouter/free"

# ── 3. TokenHarbor ────────────────────────────────────────────────────────────
# CUSTOM3_NAME="TokenHarbor"
# CUSTOM3_API_KEY="thk_live_your-key-here"
CUSTOM3_URL="https://tokenharbor.ai/v1/chat/completions"
CUSTOM3_MODEL="deepseek-v4.1-flash:free"

# ── 4. Custom Direct Endpoint ─────────────────────────────────────────────────
# Add More: Define CUSTOM5_*, CUSTOM6_*, up to CUSTOM9_*
# CUSTOM4_NAME="Custom Provider"
# CUSTOM4_API_KEY="sk-your-key-here"
CUSTOM4_URL="https://api.deepseek.com/chat/completions"
CUSTOM4_MODEL="deepseek-chat"

# ── MCP Server Keys (Optional) ────────────────────────────────────────────────
# FIRECRAWL_API_KEY="fc-your-actual-api-key"

# ── Runtime Governors ─────────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="10"
```

</details>

---

<div align="center">
  <sub>Built for speed, clarity, and terminal minimalism. Licensed under the <b><a href="LICENSE">MIT License</a></b>.</sub>
</div>



