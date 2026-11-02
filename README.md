<br>

<div align="center">
  <img alt="rs-agent" src="logo.svg?v=2" height="100" />
  <h1>rs-agent</h1>
 
  <p>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/version-v0.4.3.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=branded&brand=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/rs-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-green.svg" alt="License"></a>
    <a href="https://shieldcn.dev/badge/status-beta-blue.svg"><img src="https://shieldcn.dev/badge/status-beta-blue.svg" alt="Status"></a>
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>router</code> &nbsp;•&nbsp; <code>local-first</code>
  </p>

  <p>
    <b>A Rust AI harness built for low-latency local <code>llama-server</code> C++ backends, slm/llm gguf, and cloud endpoints.</b><br>
    <sub>Sub-millisecond boot, autonomous 6-tool execution loop, zero-trust security kernel, edge-model JSON repair.</sub>
  </p>

  <br>

  <table>
    <tr>
      <td align="center" width="50%" valign="top">
        <h3>Sub-27B (SLM)</h3>
        <p><sub>Tool calling, fast refactoring, and single-turn edits</sub></p>
        <code>LFM3-1.7B*</code> &nbsp;•&nbsp; <code>Ling-3.0-tiny</code><br>
        <code>MiniCPM5-2B</code> &nbsp;•&nbsp; <code>Qwen3.5-2B+</code><br>
        <code>MiniCPM-V-4.7-1B</code> &nbsp;•&nbsp; <code>LFM2.5-24B-A2B</code>
      </td>
      <td align="center" width="50%" valign="top">
        <h3>27B+ (LLM)</h3>
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
    <sub><b>Cloud Endpoints:</b> <a href="https://tokenharbor.ai">TokenHarbor</a>, <a href="https://openrouter.ai">OpenRouter</a>, and <a href="https://huggingface.co/spaces">Hugging Face</a><br>
    <b>Free Endpoints:</b> <a href="https://tokenharbor.ai"><code>deepseek-v4.1-flash:free</code></a> (TokenHarbor) &nbsp;•&nbsp; <a href="https://openrouter.ai/models?variant=free"><code>openrouter/free</code></a> (OpenRouter)</sub>
  </p>
</div>

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

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Execution Modes</h2>

<div align="center">

| Invocation | Interface | Description |
| :--- | :---: | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-millisecond router via `rs-context.md` (e.g. `lfm`, `ling`) |
| `ai` | **Interactive Chat** | Conversational REPL with thinking gutters and live token stats |
| `ai init [path]` | **Workspace Agent** | Autonomous project workspace with tool execution & security kernel |
| `/task [goal]` | **Task Loop** | Autonomous goal loop; auto-discovers `TASK.md` when prompt is omitted |
| `/t3` / `/t3 web` | **T3 Code IDE** | Handover active workspace and profile to T3 Code desktop or browser |
| `ai --bridge` | **Headless ACP** | Stdio JSON-RPC 2.0 interface for IDEs and desktop surfaces |

</div>

<p align="center">
  <sub><code>/p</code> profile select &nbsp;•&nbsp; <code>/task</code> goal loop &nbsp;•&nbsp; <code>/t3</code> launch IDE &nbsp;•&nbsp; <code>/s</code> toggle skills</sub>
</p>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Developer Tool Suite</h2>

Standalone native Rust binaries deployed in `tools/` for zero-overhead project maintenance:

<div align="center">

| Binary | Tool | Purpose |
| :--- | :---: | :--- |
| `ms` | **Model Selector** | Native TUI for switching models, endpoints, and loop budgets |
| `newp` | **Project Creator** | Scaffolds workspace, `.rs/config.json`, and auto-links router aliases |
| `idx` | **Codebase Indexer** | Generates token-dense Rust AST architecture maps (`index-map.txt`) via `syn` |
| `ta` | **System Diagnostics** | 10-pass native verification suite testing binary integrity, safety, and UI |

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Runtime Architecture</h2>

`rs-agent` discards heavy agent frameworks in favor of simple, predictable primitives:

* **Sub-Millisecond Shell Intercept (`matcher.rs`):** `rs-hook.sh` intercepts terminal input and matches against `rs-context.md` in `< 0.2ms`. Typing `lfm` jumps into the target directory and launches the workspace instantly.
* **Native 6-Tool Suite (`tools.rs`):** Minimal tool surface built for refactoring without AST bloat: `read_file`, `edit_file` (2-stage exact + whitespace-tolerant replacer), `write_file`, `list_dir`, `search_code`, and `run_command`.
* **Autonomous Task Loop (`/task`):** Multi-step goal loop with failure decomposition (reminds models to inspect lines after tool errors) and anti-hallucination verification.
* **Edge Argument Healer (`adapter.rs`):** Small models often hallucinate parameter aliases (`filepath` vs `path`, `cmd` vs `command`) or unclosed quotes. An in-memory stack balancer normalizes keys and heals broken JSON frames on the fly.
* **Zero-Trust Security Boundary (`security.rs`):** Canonicalizes all paths inside workspace root. Traversal attempts (`../`) and mutating system calls (`sudo`, `pacman`, `pip`, `systemctl`, destructive `rm`) fail closed and require explicit approval via `/dev/tty`.

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Native 6-Tool Suite</h2>

<div align="center">

| Tool | Action | Rules |
| :--- | :--- | :--- |
| `read_file` | Read source file | Relative workspace path. Supports `line_start` and `line_end`. Truncates files over 250 lines to conserve context. |
| `edit_file` | Surgical patch | Two-stage replacer: (1) exact unique string match, (2) whitespace-tolerant line block match. Fails if target is ambiguous. |
| `write_file` | Write / Create file | Creates missing parent directories automatically. Requires `overwrite: true` when overwriting existing files. |
| `list_dir` | Workspace exploration | Recursively lists clean file tree. Auto-ignores hidden files, `.git/`, and build artifacts (`target/`). |
| `search_code` | Grep codebase | Fast regex and pattern search across project files. Capped at first 30 matches with line numbers. |
| `run_command` | Execute shell command | Runs commands directly in workspace root. Captures up to 15,000 characters of combined stdout/stderr. |

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Efficiency & Token Footprint</h2>

<div align="center">

| Operational Tier | Token Budget | Runtime Footprint | Target Workload |
| :--- | :---: | :---: | :--- |
| **Pure Chat** (`ai`) | **~180 tokens** | < 1ms startup · 0 MB idle | Conversational queries and fast reasoning |
| **Pure Bash** (`purebash: true`) | **~95 tokens** | Single-tool dispatch loop | Low-parameter SLMs (1.7B–3B) via shell commands |
| **Native 6-Tool** *(Default)* | **~700 tokens** | Full AST-free surgical suite | Code refactoring, test execution, and file search |
| **Context Router** (`matcher.rs`) | **0 tokens** | < 0.2ms shell lookup | Instant shortcut teleportation via `rs-context.md` |

</div>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Workspace Profiles & Skills</h2>

Define agent profiles and runtime behavior in Markdown frontmatter (`profiles/*.md`):

```markdown
---
description: "Edge Systems Engineer (Native 6-Tool)"
adapters: true     # Tool argument repair (strips markdown fences, heals newlines)
memory: false      # Opt-in: injects workspace rules from .rs/memory.md if present
purebash: false    # Optional: single bash tool (~95t schema). Omit for Native 6-tool suite (~700t)
---
```

* **Profiles (`profiles/`):** Model configurations and system personas (e.g. `lfm3.md`, `lingtiny.md`, `chat.md`). Run `newp` to scaffold new workspaces with interactive profile selection.
* **Workspace Memory (`.rs/memory.md`):** Optional, project-isolated rules loaded into context when `memory: true` is set in the profile frontmatter.
* **Skills (`skills/`):** Modular runtime directives toggled via `/s` (e.g. `ponytail.md` for YAGNI minimalism, `humanizer.md` for clean output).
* **Global Rules (`system_instructions.md`):** Universal directives loaded across every profile and workspace.

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Setup & Installation</h2>

### 1. 👤 Build and Link

```bash
# Clone into config directory
git clone https://github.com/j5onrf/rs-agent.git ~/.config/rs-agent
cd ~/.config/rs-agent

# Build optimized release binary
cargo build --release

# Symlink to PATH
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/rs-agent" ~/.local/bin/ai
ln -sf "$PWD/plugins/t3code/t3code-bridge" ~/.local/bin/t3code-bridge

# Register shell hook (bash or zsh)
echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.bashrc
source ~/.bashrc
```

### 2. 👤 Configure Providers (`.env`)

<details>
<summary><b>View Example (<code>~/.config/rs-agent/.env</code>)</b></summary>

```env
# ==============================================================================
# Rs-Agent Provider Cascade (.env)
# Priority: Top-down first active key. Comment with '#' to disable.
# Fallback: When all keys are commented, routes to local server on :8080.
# Configure interactively with: models
# ==============================================================================

# ── 1. Custom 2: xAI Grok / DeepSeek / TokenHarbor ────────────────────────────
# CUSTOM2_API_KEY="sk-your-key-here"
CUSTOM2_URL="https://api.x.ai/v1/chat/completions"
CUSTOM2_MODEL="grok-beta"

# ── 2. Custom 1: Hugging Face Router ──────────────────────────────────────────
# CUSTOM_API_KEY="hf_your_key_here"
CUSTOM_URL="https://router.huggingface.co/v1/chat/completions"
CUSTOM_MODEL="space"

# ── 3. OpenRouter Universal Gateway ───────────────────────────────────────────
# OPENROUTER_API_KEY="sk-or-v1-your-key-here"
OPENROUTER_MODEL="openrouter/free"

# ── 4. Local llama-server Fallback (Port 8080) ────────────────────────────────
# Automatically used when cloud keys are commented. Auto-detects loaded .gguf model.

# ── 5. Runtime Governors ──────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="15"
```

</details>

---

<div align="center">
  <sub>Built for speed, clarity, and terminal minimalism. Licensed under the <b><a href="LICENSE">MIT License</a></b>.</sub>
</div>
