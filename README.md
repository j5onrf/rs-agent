<br>

<div align="center">
  <img alt="rs-agent" src="logo.svg?v=2" height="130" />
  <h1>Rs-Agent</h1>
 
<p>
    <a href="https://github.com/j5onrf/pyra-agent"><img src="https://shieldcn.dev/badge/version-v0.1.0.svg?variant=secondary" alt="Version"></a>
    <a href="https://github.com/j5onrf/pyra-agent"><img src="https://shieldcn.dev/badge/Rust.svg?variant=branded&brand=rust" alt="Language"></a>
    <a href="https://github.com/j5onrf/pyra-agent"><img src="https://shieldcn.dev/badge/C%2B%2B.svg?variant=branded&brand=cplusplus" alt="C++"></a>
    <a href="https://github.com/j5onrf/pyra-agent/blob/main/LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-green.svg" alt="License"></a>
    <a href="https://shieldcn.dev/badge/status-beta-blue.svg"><img src="https://shieldcn.dev/badge/status-beta-blue.svg" alt="Status"></a>
  </p>

  <p>
    <code>gguf</code> &nbsp;•&nbsp; <code>llama.cpp</code> &nbsp;•&nbsp; <code>grok</code> &nbsp;•&nbsp; <code>openrouter</code>
  </p>

  <p>
    <b>A zero-dependency Rust AI agent engine driving local <code>llama-server</code> backends and cloud endpoints.</b><br>
    <sub>Sub-millisecond startup, autonomous 6-tool refactoring suite, zero-trust security kernel, and streaming thinking telemetry.</sub>
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
        <p><sub>Deep reasoning, complex architecture, and multi-turn loops</sub></p>
        <code>Sage-A1.5-35B</code> &nbsp;•&nbsp; <code>Flint-40B-A2.5B*</code><br>
        <code>Qwen3.8-35B-D</code> &nbsp;•&nbsp; <code>KAT-Coder-V2.5</code><br>
        <code>Ornith-1.5-35B</code> &nbsp;•&nbsp; <code>Tiel-Coder-35B</code><br>
        <code>xAI Grok-Beta</code> &nbsp;•&nbsp; <code>DeepSeek-V4.1</code>
      </td>
    </tr>
  </table>
  
  <p>
    * Primary edge baselines &nbsp;•&nbsp; Auto-discovers local models loaded on <code>http://127.0.0.1:8080</code>
  </p>

  <p>
    <sub><b>Cloud Endpoints:</b> <a href="https://tokenharbor.ai">TokenHarbor</a>, <a href="https://api.x.ai">xAI Grok</a>, <a href="https://openrouter.ai">OpenRouter</a>, and <a href="https://huggingface.co/spaces">Hugging Face</a><br>
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
  <p><sub>Switch terminal styles with <code>/box [1-3]</code> or pin in profile frontmatter. Progressive 350ms wake-on-key reveal.</sub></p>
</div>

<br>

---

<h2 align="center">Execution Surfaces</h2>

<div align="center">

| Invocation | Interface | Description |
| :--- | :---: | :--- |
| `[shortcut]` | **Shell Teleport** | Sub-millisecond context router (`matcher.rs`) via `pyra-context.md` (e.g., `lfm`, `ling`) |
| `ai "<prompt>"` | **One-Shot** | Single prompt query direct from terminal |
| `ai` | **Interactive REPL** | Conversational chat session with thinking gutter and token stats |
| `ai init [path]` | **Workspace Agent** | Autonomous project workspace with full Smol 6-tool suite |

</div>

<p align="center">
  <sub><code>/box [1-3]</code> style &nbsp;•&nbsp; <code>/yolo</code> (or <code>/g</code>) autonomy gate &nbsp;•&nbsp; <code>/clear</code> (or <code>/c</code>) reset &nbsp;•&nbsp; <code>q</code> or <code>exit</code> quit</sub>
</p>

<br>

---

<h2 align="center">Runtime Architecture</h2>

`pyra-agent` discards heavy agent frameworks in favor of simple, predictable primitives:

* **Sub-Millisecond Shell Intercept (`matcher.rs`):** `pyra-hook.sh` catches missing commands and routes through `pyra-context.md` in `< 0.2ms`. Typing `lfm` jumps into the directory and launches the agent instantly.
* **Autonomous 6-Tool Suite (`tools.rs`):** Minimal tool surface built for refactoring without AST bloat: `read_file`, `edit_file` (2-stage exact + whitespace-tolerant matcher), `write_file`, `list_dir`, `search_code`, and `run_command`.
* **Edge Argument Healer (`adapter.rs`):** Small models often hallucinate parameter aliases (`filepath` vs `path`, `cmd` vs `command`) or unclosed quotes. An in-memory stack balancer normalizes keys and repairs broken JSON frames on the fly.
* **Zero-Trust Security Boundary (`security.rs`):** Canonicalizes all paths inside workspace root. Traversal attempts (`../`) and mutating system calls (`sudo`, `pacman`, `pip`, `systemctl`, destructive `rm`) fail closed and require explicit interactive approval via `/dev/tty`.
* **Live Thinking Gutter & Telemetry (`stream.rs`):** Streams model `<think>` reasoning into a dimmed `│ ` vertical gutter with word-wrapping, followed by compact live telemetry (`↑ ↓ R CH% · %/8.2k · s @ t/s`).

<br>

---

<h2 align="center">Engine Benchmarks</h2>

<p align="center">
  Performance comparison between native Rust <code>pyra-agent</code> and the Python predecessor (<code>py-agent</code>):
</p>

<div align="center">

| Metric | pyra-agent (Rust) | py-agent (Python) | Difference |
| :--- | :---: | :---: | :--- |
| **Cold Startup Latency** | **< 1 ms** | ~450 ms | **450x faster** (zero interpreter overhead) |
| **Idle Memory Footprint** | **~3.2 MB** | ~42 MB | **92% reduction** |
| **Shell Teleport Lookup** | **0.18 ms** | ~12 ms | **Instant shell interception** |
| **Runtime Dependencies** | **0** (single static binary) | Python, rich, requests | **Zero package drift** |
| **Tool Turn Speed (AG-03)** | **4.0s** (3 turns) | ~6.5s (3 turns) | **Fast local streaming** |
| **KV Cache Hit Rate** | **85%+** | ~74% | **Consistent prompt caching** |

</div>

<br>

---

<h2 align="center">Smol 6-Tool Suite</h2>

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

<h2 align="center">Workspace Profiles & Skills</h2>

Define agent personas, tool tiers, and box styles in Markdown frontmatter (`profiles/*.md`):

```markdown
---
name: lfm3
description: "Liquid AI LFM3-1.7B Edge Systems Engineer"
yolo: true
reasoning_budget: 500
box: 1
---
ROLE: Liquid AI LFM3-1.7B Autonomous Edge Systems & Software Engineer.

DIRECTIVES:
- TOOL EXECUTION: Run tools immediately. Never output introductory commentary or planning summaries.
- CLOSED-LOOP: Inspect (read_file) -> Edit (edit_file) -> Verify (run_command).
- HALT: Stop immediately with: "✓ Task complete: <summary>".
```

Drop skill guides like **`ponytail.md`** (anti-overengineering / YAGNI ladder) or **`humanizer.md`** (AI tell stripper) directly into `profiles/` or reference them inside system prompts.

<br>

---

<h2 align="center">Setup & Installation</h2>

### 1. Build and Link

```bash
# Clone into config directory
git clone https://github.com/j5onrf/pyra-agent.git ~/.config/pyra-agent
cd ~/.config/pyra-agent

# Build optimized release binary
cargo build --release

# Symlink to PATH
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/pyra-agent" ~/.local/bin/ai
ln -sf "$PWD/target/release/pyra-agent" ~/.local/bin/pyra

# Register shell hook (bash or zsh)
echo 'source ~/.config/pyra-agent/pyra-hook.sh' >> ~/.zshrc
source ~/.zshrc
```

### 2. Configure Providers (`.env`)

Configure credentials in `~/.config/pyra-agent/.env`. Pyra evaluates keys from the top down:

1. **`CUSTOM2_API_KEY`**: Generic OpenAI endpoint (Grok, DeepSeek, TokenHarbor)
2. **`CUSTOM_API_KEY`**: Hugging Face Inference Router
3. **`OPENROUTER_API_KEY`**: OpenRouter Gateway
4. **Local Fallback**: Automatically queries `http://127.0.0.1:8080/v1/models` (`llama-server`) with zero configuration.

<details>
<summary><b>View Example (<code>~/.config/pyra-agent/.env</code>)</b></summary>

```env
# ==============================================================================
# Pyra-Agent Provider Cascade (.env)
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

<h2 align="center">Context Router (`pyra-context.md`)</h2>

Declare instant shortcut aliases for fast navigation:

```markdown
# --- Workspaces ---
ai init ~/.config/pyra-agent/projects/lfm3 ---> lfm3, lfm
ai init ~/.config/pyra-agent/projects/ling-tiny ---> ling, ling-tiny
ai init ~/.config/pyra-agent/projects/clover ---> clover, clov
ai init ~/.config/pyra-agent/projects/sage ---> sage

# --- Quick Tools ---
curl -s "wttr.in/?format=3" ---> weather, wttr
date "+Current Date: %A, %B %-d, %Y" ---> get date, today
```

<br>

---

<div align="center">
  <sub>Built for speed, clarity, and terminal minimalism. Licensed under the <b><a href="LICENSE">MIT License</a></b>.</sub>
</div>

