<div align="center">
  <img alt="rs-agent" src="logo.svg?v=2" height="460" />
  <h1>Rs-Agent</h1>

  <p>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/version-v0.6.5.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=branded&brand=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/rs-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-green.svg" alt="License"></a>
    <a href="https://shieldcn.dev/badge/status-beta-blue.svg"><img src="https://shieldcn.dev/badge/status-beta-blue.svg" alt="Status"></a>
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>t3code</code> &nbsp;•&nbsp; <code>omarchy</code> &nbsp;•&nbsp; <code>local-first</code>
  </p>

  <p>
    <b>Sub-millisecond native Rust AI harness for local <code>llama-server</code> GGUF backends and cloud reasoning models.</b><br>
    <sub>Parallel 6-tool execution kernel • Parallel read-only tool kernel • 3-stage resilient edits • Zero-trust path sandbox • Native T3 Code ACP bridge</sub>
  </p>

  <table>
    <tr>
      <td align="center" width="28%" valign="top">
        <h3>👤 Sub-27B (SLM)</h3>
        <p><sub>Tool calling, fast refactoring, and single-turn edits</sub></p>
        <code>LFM3-1.7B</code> &nbsp;•&nbsp; <code>Ling-3.0-tiny*</code><br>
        <code>MiniCPM5-2B</code> &nbsp;•&nbsp; <code>Qwen3.5-2B+</code><br>
        <code>MiniCPM-V-4.7-1B</code> &nbsp;•&nbsp; <code>LFM2.5-24B-A2B</code>
      </td>
      <td align="center" width="28%" valign="top">
        <h3>👤 27B+ (LLM)</h3>
        <p><sub>Deep reasoning, complex logic, and multi-turn loops</sub></p>
        <code>Groot-19B-A3B</code> &nbsp;•&nbsp; <code>Clover-35B-A3B</code><br>
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
    <sub><b>Cloud Endpoints:</b> <a href="https://ai.google.dev">Google Gemini</a> (<code>gemini-4-hydrogen</code>), <a href="https://openrouter.ai">OpenRouter</a>, and <a href="https://tokenharbor.ai">TokenHarbor</a><br>
    <b>Free Endpoints:</b> <a href="https://ai.google.dev"><code>gemini-3.5-flash-lite</code></a> (Google AI Studio) &nbsp;•&nbsp; <a href="https://openrouter.ai/models?variant=free"><code>openrouter/free</code></a> (OpenRouter) &nbsp;•&nbsp; <a href="https://tokenharbor.ai"><code>deepseek-v4.1-flash:free</code></a> (TokenHarbor)</sub>
  </p>
</div>

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Execution Modes & Controls

| Invocation | Mode | Description |
| :--- | :--- | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-`0.2ms` router via `rs-context.md` (`lfm`, `ling`, etc.) |
| 👤 `ai` | **Interactive Chat** | Pure conversational REPL (`~180t` profile, thinking gutters, `/pad`) |
| 👤 `ai init [path]` | **Workspace Agent** | Autonomous project workspace with tool execution & security sandbox |
| `/task [goal]` | **Task Loop** | Multi-step autonomous loop; auto-loads `TASK.md` if goal is omitted |
| `/t3` / `/t3 web` | **T3 Code ADE** | Compositor handover to official T3 Code GUI with live diffs & permission gates |
| `ai --bridge` | **Headless ACP** | Embedded stdio JSON-RPC 2.0 server for ADEs and desktop surfaces |

**In-Session Slash Commands:**
`/p` profile select • `/task` goal loop • `/t3` launch ADE • `/s` toggle skills • `/box [1-6]` box themes • `/pad` spacing • `/r` reset

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Client Surface: T3 Code ADE

**[T3 Code](https://t3.codes)** is the official agent developer environment for `rs-agent`, connected directly to `ai --bridge` over stdio JSON-RPC 2.0 (ACP).

| Capability | Mechanism | What It Gives You |
| :--- | :--- | :--- |
| **Seamless Handover** | `/t3` · `/t3 web` | Instant desktop or browser launch (`projects/chat` for chat, workspace root for agents) |
| **Interactive Diff Review** | `session/request_permission` | Live split/unified previews with T3's 4-tier controls (*Supervised*, *Auto-accept*, *Auto*, *Full access*) |
| **Dynamic Controls** | `$skills` · `/commands` · `thought_level` | Autocomplete `$` skill pills, reasoning selector (`low`–`xhigh` cloud / token budgets local), multimodal vision |

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> CLI Box Styles (`/box [1-6]`)

<details>
<summary><b>Preview all 6 box themes (halftone skeleton dither, dot slabs, wake-on-key animations)</b></summary>

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

╭─ ○ ────────────────────────────────────────╮
│  ╭────╮  LFM3-1.7B                         │
│  │ LF │  chat                              │
│  ╰────╯  git: master · clean               │
╰────────────────────────────────────────────╯

  ╭────╮  LFM3-1.7B
  │ LF │  chat · 2
  ╰────╯  git: master · clean

· · ○ · · · · · · · · · · · · · · · · · · · · ·
·  ╭────╮  LFM3-1.7B                · · · · · ·
·  │ LF │  lfm3                     · · · · · ·
·  ╰────╯  git: main · 6 modified   · · · · · ·
· · · · · · · · · · · · · · · · · · · · · · · ·

❯ █
```

</details>

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Runtime Architecture

```text
Terminal Input ─▶ rs-hook.sh (<0.2ms) ─▶ matcher.rs (rs-context.md) ─▶ Workspace Teleport
                                     └─▶ ai / ai --bridge
                                            ├── client.rs   (SSE stream, thought_signature, cloud effort / local budget)
                                            ├── adapter.rs  (JSON bracket balancer, gutter strip, alias normalization)
                                            ├── tools.rs    (concurrent read-only batching, 3-stage edit, 30s timeout)
                                            ├── security.rs (symlink-safe canonical jail, dangerous command gates)
                                            └── mcp.rs      (on-demand stdio MCP plugins e.g. photocraft)
```

* **Parallel Read-Only Tool Kernel (`tools.rs`):** Executes contiguous `read_file`, `list_dir`, and `search_code` calls concurrently while keeping mutating actions (`edit_file`, `write_file`, `run_command`) strictly ordered.
* **3-Stage Resilient Edits (`tools.rs`):** Patches files through *Exact match → Indent-delta whitespace normalization → Levenshtein fuzzy match (≥ 0.85)*, with automatic process-group reaping (`SIGTERM` → `SIGKILL`) after 30s on `run_command`.
* **Edge Argument Healer (`adapter.rs`):** Strips copied line gutters (`12: `), fixes container paths (`/workspace/...`), balances truncated JSON quotes/brackets, and normalizes parameter aliases in memory.
* **Zero-Trust Security Boundary (`security.rs`):** Walks ancestors and inspects symlinks before canonicalization. Fails closed on `../` traversal, system directories (`/etc`, `/usr`, `/root`, ...), `python -m pip`, destructive `rm`, and privileged binaries.

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Efficiency & Context Scaling

### Tool Schema Footprint

| Operational Tier | Frontmatter Flag | Schema Cost | Intended Workload |
| :--- | :--- | :---: | :--- |
| **Pure Chat** (`ai`) | *(none)* | **~180t** | Pure conversation and fast reasoning (0 tools) |
| **Pure Bash** | `purebash: true` | **~95t** | Single-tool shell dispatch (`run_command`) |
| **Core 4-Tool** | `core4: true` | **~480t** | 27B+ LLMs & cloud models (`read`, `edit`, `write`, `run_command`) |
| **Native 6-Tool** | *(default)* | **~680t** | Full local code exploration (`+ list_dir`, `search_code`) |
| **+ Sub-Agent** | `delegate: true` | **+ ~100t** | Attaches isolated `delegate_task` sub-agent to any tier |
| **Context Router** | *(shell hook)* | **0t** | Sub-millisecond directory/command routing via `rs-context.md` |

### Context Ceilings (`AI_MAX_TOKENS`)

File reads and shell outputs adapt dynamically to `AI_MAX_TOKENS`, offloading overflows to `.rs/scratchpad.log` with clean head/tail previews:

| `AI_MAX_TOKENS` | `read_file` Ceiling | `run_command` Cap | `search_code` Cap |
| :--- | :---: | :---: | :---: |
| **`<= 16k`** | 250 lines | 20,000 chars | 30 matches |
| **`32k`** | 1,000 lines | 45,000 chars | 50 matches |
| **`64k`** | 2,000 lines | 90,000 chars | 75 matches |
| **`>= 128k`** | 4,000 lines | 180,000 chars | 100 matches |

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Workspace Profiles & Skills

Configure agent personas, tool tiers, MCP servers, and reasoning behavior in `profiles/*.md` frontmatter:

```markdown
---
description: "High-Speed Coding & Systems Engineer"
adapters: true          # Strip line gutters, normalize aliases, heal malformed JSON
core4: true             # Minimal 4-tool schema (~480t) for 27B+ / cloud (omit for 6-tool)
reasoning_budget: 500   # Local: 0-10000 tokens (0 = off) | Cloud: low, medium, high, xhigh
reasoning_effort: high  # Explicit cloud tier: low, medium, high, xhigh (e.g. gemini-4-hydrogen)
memory: true            # Inject project-isolated rules from .rs/memory.md
purebash: true          # Single run_command tool (~95t). Omit for Core 4 / Native 6
delegate: true          # Attach isolated delegate_task sub-agent (+~100t)
mcp: photocraft         # Attach stdio MCP server(s) (e.g. photocraft, blender, mailcraft).
---
```

* **Skills (`skills/`):** Modular runtime directives toggled via `/s` or T3 Code's `$` picker (`ponytail.md`, `humanizer.md`, `ub-mode.md`).
* **Global Rules (`system_instructions.md`):** Universal rules injected across every workspace and profile.

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Agent Evaluation (`eval-stack`)

5-stage autonomous benchmark measuring par efficiency, tool resilience, and token economy via `eva`:

| Rank | Model | Par Eff | Agent Index |
| :---: | :--- | :---: | :---: |
| **1** | **Flint-40B-A2.5B** | **185.7%** | **100.0 (A+)** |
| **2** | **Groot-19B-A3B** *(MTP)* | **162.5%** | **100.0 (A+)** |
| **3** | **Sage-A1.5-35B** | **111.1%** | **100.0 (A+)** |
| **4** | **Qwen3.8-35B-Distill** *(MTP)* | **136.4%** | **92.2 (A)** |
| **—** | **Ling-3.0-tiny** *(SLM)* | **162.5%** | **100.0 (A+)** |
| **—** | **LFM3-1.7B** *(SLM)* | **173.3%** | **100.0 (A+)** |

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Developer Tools (`tools/`)

| Binary | Tool | Purpose |
| :--- | :--- | :--- |
| 👤 `ms` | **Model Selector** | Interactive TUI for switching active providers, `AI_MAX_TOKENS`, and loop limits |
| `newp` | **Project Creator** | Scaffolds workspace, `.rs/config.json`, profile, and `rs-context.md` aliases |
| `idx` | **Codebase Indexer** | Generates token-dense Rust AST architecture maps (`index-map.txt`) |
| `ta` | **System Diagnostics** | 10-pass native test harness checking kernel, security boundary, and UI |
| `eva` | **Agentic Benchmark** | Full-stack `eval-stack` evaluation runner |

---

## <img alt="rs-agent" src="logo.svg?v=2" height="50" /> Setup & Configuration

### 👤 Quick Start (Linux x86_64)

```bash
git clone https://github.com/j5onrf/rs-agent.git ~/.config/rs-agent
echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.bashrc
source ~/.config/rs-agent/rs-hook.sh
```

<details>
<summary><b>macOS / Build from Source</b></summary>

```bash
cd ~/.config/rs-agent
cargo build --release --workspace
mkdir -p bin ~/.local/bin
cp target/release/ai bin/ai
cp target/release/ai bin/rs-agent
cp target/release/model-select bin/ms
ln -sf ~/.config/rs-agent/bin/ms ~/.local/bin/ms

echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.zshrc
source ~/.config/rs-agent/rs-hook.sh
```

</details>

<details>
<summary><b>Provider Cascade &amp; Governors (<code>~/.config/rs-agent/.env</code>)</b></summary>

Priority goes to the first uncommented active key; when all cloud keys are commented out, `rs-agent` falls back to local `llama-server` on `:8080`.

```env
# ── 1. Google Gemini ──────────────────────────────────────────────────────────
#GEMINI_API_KEY="AQ.your-key-here"
GEMINI_MODEL="gemini-4-hydrogen"       # or gemini-3.5-flash-lite (free tier)

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
#OPENROUTER_API_KEY="sk-or-v1-your-key-here"
OPENROUTER_MODEL="openrouter/free"

# ── 3. TokenHarbor ────────────────────────────────────────────────────────────
#TOKENHARBOR_API_KEY="thk_live_your-key-here"
TOKENHARBOR_MODEL="deepseek-v4.1-flash:free"

# ── More Direct Providers ─────────────────────────────────────────────────────
# OPENAI_API_KEY="sk-your-key-here"
# ANTHROPIC_API_KEY="sk-ant-your-key-here"

# ── 4. Custom Direct Slots (CUSTOM1..CUSTOM9) ─────────────────────────────────
# CUSTOM1_NAME="Custom Provider"
# CUSTOM1_API_KEY="sk-your-key-here"
CUSTOM1_URL="https://api.deepseek.com/chat/completions"
CUSTOM1_MODEL="deepseek-flash"

# ── MCP Server Keys (Optional) ────────────────────────────────────────────────
# FIRECRAWL_API_KEY="fc-your-actual-api-key"

# ── Runtime Governors ─────────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"                   # Set 65536+ for cloud models like gemini-4-hydrogen
AI_MAX_AGENT_ROUNDS="10"
```

</details>

---

<div align="center">
  <sub>Built for speed, clarity, and terminal minimalism. Licensed under the <b><a href="LICENSE">MIT License</a></b>.</sub>
</div>
