<h1><img alt="" src="assets/logo.svg" width="36" height="36" /> Rs-Agent</h1>

  <p>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/version-v0.7.4.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=secondary&logo=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/rs-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT.svg?variant=secondary" alt="License"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/status-beta.svg?variant=secondary" alt="Status"></a>
  </p>

  <p>
    <strong>Sub-millisecond native Rust AI harness for local <code>llama-server</code> GGUF backends and frontier cloud endpoints.</strong><br />
    Parallel 6-tool kernel &nbsp;•&nbsp; 3-stage resilient edits &nbsp;•&nbsp; Zero-trust path sandbox &nbsp;•&nbsp; Native T3 Code ACP bridge
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>t3code</code> &nbsp;•&nbsp; <code>omarchy</code> &nbsp;•&nbsp; <code>local-first</code>
  </p>

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Tested Tiers & Endpoints

| Tier | Tested Models | Workload |
| :--- | :--- | :--- |
| **Sub-27B**<br />*(SLM)* | `LFM3-1.7B` · `Ling-3.0-tiny*` · `MiniCPM5-2B` · `Qwen3.5-2B+` · `MiniCPM-V-4.7-1B` · `LFM2.5-24B-A2B` | Tool calling, fast refactoring, single-turn edits |
| **27B+**<br />*(LLM)* | `Groot-19B-A3B` · `Clover-35B-A3B` · `Sage-A1.5-35B` · `Flint-40B-A2.5B*` · `Qwen3.8-35B-D` · `KAT-Coder-V2.5` · `Ornith-1.5-35B` · `Tiel-Coder-35B` · `Qwen3.8-27B` · `Qwen4.0-27B` · `Qwen3.8-Flash` · `DeepSeek-V4.1` | Deep reasoning, complex logic, multi-turn loops |

*\* Primary edge baselines • Auto-discovers local models on `http://127.0.0.1:8080`*

* **Cloud Endpoints:** [Google Gemini](https://ai.google.dev) (`gemini-4-hydrogen`) • [OpenRouter](https://openrouter.ai) • [TokenHarbor](https://tokenharbor.ai) • [AMD Token Lab](https://notebooks.amd.com)
* **Free Endpoints:** [`gemini-3.5-flash-lite`](https://ai.google.dev) • [`baidu/ernie-5.5-flash-preview:free`](https://openrouter.ai/baidu/ernie-5.5-flash-preview:free) • [`deepseek-v4.1-flash:free`](https://tokenharbor.ai) • [`DeepSeek-V4.1-Flash-1017`](https://notebooks.amd.com)

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Execution Modes & Controls

| Invocation | Mode | Description |
| :--- | :--- | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-`0.2ms` router via `rs-context.md` (`lfm`, `ling`, etc.) |
| <img alt="" src="assets/prompt2.svg" width="14" height="14" /> `ai` | **Interactive Chat** | Pure conversational REPL (`~180t` profile, thinking gutters, `/pad`) |
| <img alt="" src="assets/prompt2.svg" width="14" height="14" /> `ai init [path]` | **Workspace Agent** | Autonomous project workspace with tool execution & security sandbox |

**In-Session Slash Commands:**
`/p` profile select • `/ms` model select • `/task` goal loop • `/t3` launch ADE • `/s` toggle skills • `/b [1-7]` box themes • `/pad` spacing • `/tk` thinking view • `/r` reset

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Client Surface: T3 Code ADE

**[T3 Code](https://t3.codes)** connects to `ai --bridge` over stdio JSON-RPC 2.0 (ACP).

| Feature | Mechanism | Behavior |
| :--- | :--- | :--- |
| **Handover** | `/t3` · `/t3 web` | Launches desktop or browser (`projects/chat` in chat mode, workspace root for agents) |
| **Diff Review** | `session/request_permission` | Split/unified diff gates (*Supervised*, *Auto-accept*, *Auto*, *Full access*) |
| **Live Controls** | `$skills` · `/commands` · `thought_level` | `$` skill pills, reasoning tier/budget selector, vision input |

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> CLI Box Styles (`/box [1-7]`)

* **`/box 1–6`:** Compact header, single-line `❯ ` prompt, `│ ` thinking gutter, inline stats, dense layout (`pad: false`).
* **`/box 7` *(default)*:** 2-column profile HUD, ruled `────` input bar, `/` command menu, `∴` expanded thinking, spaced layout (`pad: true`).
* **`/think`:** Cycle reasoning display between expanded `∴` italic (Box 7 default), `│ ` gutter (Boxes 1–6 default), or collapsed `∴ Thought for Xs` (persisted via `"showThinkingSummaries"` in `state.json`).

<details>
<summary><b>Preview all 7 box themes (halftone skeleton dither, dot slabs, 2-column header)</b></summary>

```yaml
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

╭─── rs-agent v0.7.4 ──────────────────────────────────────────────────────────────────────╮
│   ╭────╮  Ling-3.0-tiny                    │ 6-tool · t350 · adp                         │
│   │ Li │  lingtiny                         │ Type / for commands · Tab completes         │
│   ╰────╯  git: main · 11 modified          │ ─────────────────────────────────────────── │
│   ~/.config/rs-agent/projects/ling-tiny    │ ctx 8.2k · max 10 steps · ↑/↓ history       │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
╭─── rs-agent v0.7.4 ──────────────────────────────────────────────────────────────────────╮
│   ╭────╮  Ling-3.0-tiny                    │ purebash · t256 · adp · mcp:photocraft(7)   │
│   │ Li │  artcraft/photocraft-l            │ Type / for commands · Tab completes         │
│   ╰────╯  git: main · clean                │ ─────────────────────────────────────────── │
│   ~/.config/rs-agent/projects/ling-tiny    │ ctx 8.2k · max 15 steps · ↑/↓ history       │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
╭─── rs-agent v0.7.4 ──────────────────────────────────────────────────────────────────────╮
│   ╭────╮  DeepSeek-V4.1-Flash              │ core4 · medium                              │
│   │ De │  tokenlab                         │ Type / for commands · Tab completes         │
│   ╰────╯  git: main · 7 modified           │ ─────────────────────────────────────────── │
│   ~/.config/rs-agent/projects/tokenlab     │ ctx 65.5k · max 15 steps · ↑/↓ history      │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
```

</details>

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Runtime Architecture

```yaml
Terminal Input ─▶ rs-hook.sh (<0.2ms) ─▶ matcher.rs (rs-context.md) ─▶ Workspace Teleport
                                     └─▶ ai / ai --bridge
                                            ├── client.rs   (streaming, retries, model fallback, reasoning)
                                            ├── adapter.rs  (line-number strip, path & JSON cleanup)
                                            ├── tools.rs    (parallel reads, 3-stage edits, 30s timeout)
                                            ├── security.rs (symlink-safe sandbox, command gates)
                                            └── mcp.rs      (MCP plugins + per-profile mcp_tools filter)
```

* **Streaming & Retries (`client.rs`):** Talks straight to provider APIs without an external SDK, so local `llama-server` prompt caching and reasoning work natively. Retries brief rate limits or server errors up to 2 times (`Retry-After`), or switches to the next model in comma-separated `*_MODEL` lists after `15s` (`↳ <model>`).
* **Tool Runner (`tools.rs`):** Runs read-only lookups (`read_file`, `list_dir`, `search_code`) in parallel, applies file edits with whitespace and fuzzy matching plus inline diffs, and stops hung shell commands after `30s`.
* **Input Cleanup (`adapter.rs`):** Strips copied line numbers (`12: `), fixes `/workspace/` paths, and repairs broken JSON tool arguments from small local models.
* **Sandbox (`security.rs`):** Resolves symlinks to keep file access inside the workspace, and prompts before running risky commands (`rm`, `pip`, `sudo`).

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Efficiency & Context Scaling

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

File reads and shell output scale with `AI_MAX_TOKENS`, spilling overflows to `.rs/scratchpad.log`:

| `AI_MAX_TOKENS` | `read_file` Ceiling | `run_command` Cap | `search_code` Cap |
| :--- | :---: | :---: | :---: |
| **`<= 16k`** | 250 lines | 20,000 chars | 30 matches |
| **`32k`** | 1,000 lines | 45,000 chars | 50 matches |
| **`64k`** | 2,000 lines | 90,000 chars | 75 matches |
| **`>= 128k`** | 4,000 lines | 180,000 chars | 100 matches |

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Workspace Profiles & Skills

Configure tool tiers, reasoning, and MCP servers in `profiles/*.md`:

```yaml
---
description: "High-Speed Coding & Systems Engineer"
adapters: true          # Strip line gutters, normalize aliases, heal malformed JSON
core4: true             # Minimal 4-tool schema (~480t) for 27B+ / cloud (omit for 6-tool)
ctx: 65536              # Optional session context ceiling override (defaults to AI_MAX_TOKENS)
reasoning_budget: 500   # Local: 0-10000 tokens (0 = off) | Cloud: low, medium, high, xhigh
memory: true            # Inject project-isolated rules from .rs/memory.md
purebash: true          # Single run_command tool (~95t). Omit for Core 4 / Native 6
delegate: true          # Attach isolated delegate_task sub-agent (+~100t)
mcp: photocraft         # Attach stdio MCP server(s) (e.g. photocraft, blender, mailcraft)
mcp_tools: doc_new, doc_save  # Optional allowlist to keep local prefill low
---
```

* **Skills (`skills/`):** Modular runtime directives toggled via `/s` or T3 Code's `$` picker (`ponytail.md`, `humanizer.md`, `ub-mode.md`).
* **Global Rules (`system_instructions.md`):** Universal rules injected across every workspace and profile.

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Agent Evaluation (`eval-stack`)

5-stage benchmark via `eva`:

| Rank | Model | Par Eff | Agent Index |
| :---: | :--- | :---: | :---: |
| **1** | **Flint-40B-A2.5B** | **185.7%** | **100.0 (A+)** |
| **2** | **Groot-19B-A3B** *(MTP)* | **162.5%** | **100.0 (A+)** |
| **3** | **Sage-A1.5-35B** | **111.1%** | **100.0 (A+)** |
| **—** | **Ling-3.0-tiny** *(SLM)* | **162.5%** | **100.0 (A+)** |
| **—** | **LFM3-1.7B** *(SLM)* | **173.3%** | **100.0 (A+)** |

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Developer Tools (`tools/`)

| Binary | Tool | Purpose |
| :--- | :--- | :--- |
| <img alt="" src="assets/prompt2.svg" width="14" height="14" /> `ms` | **Model Select** | Interactive TUI for switching active providers, `AI_MAX_TOKENS` (`4k`–`1M`), and loop limits |
| `newp` | **New Project** | Scaffolds workspace, `.rs/config.json`, profile, and `rs-context.md` aliases |
| `idx` | **Index Map** | Generates token-dense Rust AST codebase maps (`index-map.txt`) |
| `ta` | **Test Agent** | 10-pass live verification suite checking tools, security boundary, UI, and `--bridge` ACP |
| `eva` | **Eval Benchmark** | Full-stack `eval-stack` model evaluation runner |

---

## <img alt="rs-agent" src="assets/prompt.svg" width="26" height="26" /> Setup & Configuration

### <img alt="" src="assets/prompt2.svg" width="18" height="18" /> Quick Start (Linux x86_64)

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

Priority goes to the first uncommented active key; when all cloud keys are commented out, `rs-agent` falls back to local `llama-server` on `:8080`. Model select with: /ms or ms.

```env
# ── 1. Google Gemini ──────────────────────────────────────────────────────────
#GEMINI_API_KEY="AQ.your-key-here"
GEMINI_MODEL="gemini-3.5-flash-lite, gemini-4-hydrogen, gemini-3.8-flash, gemini-decision-1"

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
#OPENROUTER_API_KEY="sk-or-v1-your-key-here"
OPENROUTER_MODEL="nvidia/nemotron-3.5-lightning:free, nvidia/nemotron-3.5-super-vl:free, nvidia/nemotron-3-ultra-550b-a55b:free, baidu/ernie-5.5-flash-preview:free, openrouter/free"

# ── 3. Custom Direct (CUSTOM1..CUSTOM9) ───────────────────────────────────────
#CUSTOM1_API_KEY="thk_live_your-key-here"
CUSTOM1_NAME="TokenHarbor"
CUSTOM1_URL="https://tokenharbor.ai/v1/chat/completions"
CUSTOM1_MODEL="deepseek-v4.1-flash:free, mimo-v2.6-flash:free"

# AMD Token Lab (Comma fallbacks)
# CUSTOM2_NAME="AMD Token Lab"
# CUSTOM2_API_KEY="tl-your-key-here"
# CUSTOM2_URL="https://api.tokenlab.amd.com/v1/chat/completions"
# CUSTOM2_MODEL="DeepSeek-V4.1-Flash, DeepSeek-V4-Flash, Qwen3.8-27B, Gemma-4-31B-IT, MiniCPM-V-4.7-35B-A3B"

# ── 4. MCP & Runtime Governors ────────────────────────────────────────────────
#FIRECRAWL_API_KEY="fc-your-actual-api-key"
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="10"
```

</details>

---

<div align="center">
  <a href="LICENSE">MIT License</a>
</div>
