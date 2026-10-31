<br>

<div align="center">
  <img alt="rs-agent" src="logo.svg?v=2" height="130" />
  <h1>Rs-Agent</h1>
 
<p>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/version-v0.2.1.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/Cargo.svg?variant=outline&logo=rust" alt="Cargo"></a>
    <a href="https://github.com/j5onrf/rs-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=branded&brand=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/rs-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-green.svg" alt="License"></a>
    <a href="https://shieldcn.dev/badge/status-beta-blue.svg"><img src="https://shieldcn.dev/badge/status-beta-blue.svg" alt="Status"></a>
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>plugins</code> &nbsp;•&nbsp; <code>router</code>
  </p>

<p>
    <b>A Rust AI agent built for low-latency local <code>llama-server</code> C++ backends, slm/llm gguf, and cloud endpoints.</b><br>
    <sub>Sub-millisecond boot, autonomous 6-tool execution loop, zero-trust security kernel, edge-model JSON repair, and live token stats.</sub>
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
        <p><sub>Deep reasoning, long complex, and multi-turn loops</sub></p>
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

<br>

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
  <p><sub>Switch terminal styles with <code>/box [1-3]</code> Progressive 350ms wake-on-key reveal.</sub></p>
</div>

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Execution Surfaces</h2>

<div align="center">

| Invocation | Interface | Description |
| :--- | :---: | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-millisecond context router (`matcher.rs`) via `rs-context.md` (e.g., `lfm`, `ling`) |
| `ai` | **Interactive Chat** | Conversational chat session with thinking and token stats |
| `ai init [path]` | **Workspace Agent** | Autonomous project workspace with full Smol 6-tools |

</div>

<p align="center">
  <sub><code>/p</code> (or <code>/profile</code>) profile select &nbsp;•&nbsp; <code>/r</code> reset session &nbsp;•&nbsp; <code>/s</code> or <code>/skill</code> skills</sub>
</p>

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Runtime Architecture</h2>

`rs-agent` discards heavy agent frameworks in favor of simple, predictable primitives:

* **Sub-Millisecond Shell Intercept (`matcher.rs`):** `rs-hook.sh` catches missing commands and routes through `rs-context.md` in `< 0.2ms`. Typing `lfm` jumps into the directory and launches the agent instantly.
* **Autonomous 6-Tool Suite (`tools.rs`):** Minimal tool surface built for refactoring without AST bloat: `read_file`, `edit_file` (2-stage exact + whitespace-tolerant matcher), `write_file`, `list_dir`, `search_code`, and `run_command`.
* **Edge Argument Healer (`adapter.rs`):** Small models often hallucinate parameter aliases (`filepath` vs `path`, `cmd` vs `command`) or unclosed quotes. An in-memory stack balancer normalizes keys and repairs broken JSON frames on the fly.
* **Zero-Trust Security Boundary (`security.rs`):** Canonicalizes all paths inside workspace root. Traversal attempts (`../`) and mutating system calls (`sudo`, `pacman`, `pip`, `systemctl`, destructive `rm`) fail closed and require explicit interactive approval via `/dev/tty`.

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Smol 6-Tool Suite</h2>

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

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Workspace Profiles & Skills</h2>

Define agent profiles, tool tiers in Markdown frontmatter (`profiles/*.md`):

```markdown
---
name: lfm3
description: "Liquid AI LFM3-1.7B Ultra-Fast Edge Systems Engineer (Native 6-Tool)"
yolo: true
reasoning_budget: 500
adapters: true     # SLM tool argument repair (strips markdown fences, heals newlines)
purebash: false    # false = 6 tools (~700t schema), true = single bash tool (~95t schema)
---
```

Drop profiles for models like **`lfm3.md`** (Liquid AI LFM3-1.7B) or **`lingtiny.md`** (Ling-3.0-tiny) directly into `profiles/` or type `newp` in terminal to create them.

Drop skill guides like **`ponytail.md`** (anti-overengineering / YAGNI ladder) or **`humanizer.md`** (AI tell stripper) directly into `skills/`.

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Setup & Installation</h2>

### 1. 🚀 Build and Link

```bash
# Clone into config directory
git clone https://github.com/j5onrf/rs-agent.git ~/.config/rs-agent
cd ~/.config/rs-agent

# Build optimized release binary
cargo build --release

# Symlink to PATH
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/rs-agent" ~/.local/bin/ai

# Register shell hook (bash or zsh)
echo 'source ~/.config/rs-agent/rs-hook.sh' >> ~/.bashrc
source ~/.bashrc
```

### 2. 🔌 Configure Providers (`.env`)

<details>
<summary><b>View Example (<code>~/.config/rs-agent/.env</code>)</b></summary>

```env
# ==============================================================================
# Rs-Agent Provider Cascade (.env)
# Priority: Top-down first active key. Comment with '#' to disable.
# Fallback: When all keys are commented, routes to local server on :8080.
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
```

</details>

<br>

---

<h2 align="center"><img alt="rs-agent" src="logo.svg?v=2" height="50" /> Context Router (`rs-context.md`)</h2>

Declare instant shortcut aliases for fast navigation:

```markdown
# --- Workspaces ---
ai init ~/.config/rs-agent/projects/lfm3 ---> lfm3, lfm
ai init ~/.config/rs-agent/projects/ling-tiny ---> ling, ling-tiny
ai init ~/.config/rs-agent/projects/clover ---> clover, clov
ai init ~/.config/rs-agent/projects/sage ---> sage

# --- Quick Tools ---
curl -s "wttr.in/?format=3" ---> weather, wttr
date "+Current Date: %A, %B %-d, %Y" ---> get date, today
```

<br>

---

<div align="center">
  <sub>Built for speed, clarity, and terminal minimalism. Licensed under the <b><a href="LICENSE">MIT License</a></b>.</sub>
</div>

