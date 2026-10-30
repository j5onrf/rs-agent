# ⚡ pyra-agent

**Sub-millisecond startup. Pure Rust AI Agent Engine.**

Replacing Python agent frameworks with a high-density, memory-safe binary tailored for local edge models (`LFM3-1.7B`, `Qwen`) and cloud LLMs (`xAI Grok`, `OpenRouter`).

```console
╭─ ○ ────────────────────────────────────────╮
│  LFM3-1.7B                                 │
│  chat                                      │
╰────────────────────────────────────────────╯
```

</div>

---

## ⚡ Highlights

* **< 1ms Startup Time:** Instantaneous CLI response time (`ai`) with zero interpreter overhead.
* **Sub-Millisecond Shell Teleportation:** Intercepts `command-not-found` via `matcher.rs` to route workspace shortcuts (`lfm`, `ling`, `sage`, `clover`) instantly into agent sessions.
* **Autonomous Multi-Turn Tool Loop:** 6-tool smol suite with closed-loop self-healing, automatic retry on conflict, and multi-turn refactoring.
* **Edge-Model Self-Healing (`adapter.rs`):** Auto-normalizes tool aliases (`filepath` → `path`, `cmd` → `command`) and repairs malformed/unclosed JSON arguments using a stack balancer.
* **Zero-Trust Security Boundary (`security.rs`):** Workspace path containment with canonicalization (prevents directory traversal `../`) and gates privileged/mutating commands (`sudo`, `pacman`, `pip`, `systemctl`) via `/dev/tty` prompts.
* **Streaming Thinking Gutter (`stream.rs`):** Clean dimmed vertical gutters (`│ `) for `<think>` reasoning blocks with realtime word-wrapping and token telemetry.
* **Box Presets with Wake-on-Key:** 3 terminal box styles with 350ms progressive-reveal fade animations on startup.

---

## 🎨 Terminal Box Aesthetics

Switch between presets on the fly using `/box [1-3]`, or pin them in profile frontmatter (`box: 1`):

### Box 1: Clean Rounded (Default)
```console
╭─ ○ ────────────────────────────────────────╮
│  LFM3-1.7B                                 │
│  chat                                      │
╰────────────────────────────────────────────╯
```

### Box 2: Original Dot Slab Gradient
```console
· · ○ · · · · · · · · · · · · · · · · · · · · ·
·  LFM3-1.7B  · · · · · · · · · · · · · · · · ·
·  lfm3       · · · · · · · · · · · · · · · · ·
· · · · · · · · · · · · · · · · · · · · · · · ·
```

### Box 3: 2-Line Minimal Floating Dots
```console
·  LFM3-1.7B  · · · · · · · · · · · · · · · · ·
·  lfm3       · · · · · · · · · · · · · · · · ·
```

---

## 🛠 Smol 6-Tool Suite

`pyra-agent` includes an autonomous 6-tool suite built specifically for refactoring, exploration, and execution:

| Tool | Purpose | Behavior |
| :--- | :--- | :--- |
| `read_file` | Read project files | Supports `line_start` and `line_end` pagination; auto-truncates 250+ lines. |
| `edit_file` | Surgical code replacement | 2-stage replacer: (1) exact substring replacement, (2) whitespace-tolerant line block matcher. |
| `write_file` | File creation & overwrite | Creates parent directories automatically; guards against accidental overwrites (`overwrite: true`). |
| `list_dir` | Workspace exploration | Lists relative file tree, auto-filtering hidden files and common noise (`.git`, `target`). |
| `search_code` | Fast regex & text search | Case-insensitive workspace ripgrep-style search with match cap (30 items). |
| `run_command` | Workspace command runner | Runs isolated shell calls inside workspace; traps stderr/stdout up to 15k characters. |

---

## 🛡 Zero-Trust Security Kernel

The agent operates in a sandboxed, zero-trust perimeter:

1. **Workspace Boundary:** Every relative and absolute path is canonicalized. Attempts to escape root via `../` or symlink traversal are caught and blocked unless explicitly approved via `/dev/tty`.
2. **Mutating Command Interception:** Dangerous system commands (`sudo`, `pacman`, `yay`, `pip`, `systemctl`, `rm -rf`) trigger security alerts that **never** auto-approve—even when `--yolo` mode is active.
3. **Dedicated TTY Prompts:** Security authorization reads directly from `/dev/tty`, leaving standard input clean for pipes and automation.

---

## 🧭 Shell Teleportation & Context Router

`pyra-agent` eliminates friction through shell hook integration (`pyra-hook.sh`). When you type a workspace name directly in your shell:

```bash
~ ❯ lfm
```

1. Shell hook catches `command-not-found`.
2. `matcher.rs` queries `pyra-context.md` in `< 0.2ms`.
3. Resolves to `ai init ~/.config/pyra-agent/projects/lfm3`.
4. Automatically `cd`s to the workspace and boots the session.

### Configuration (`pyra-context.md`)
```markdown
# --- Workspaces ---
ai init ~/.config/pyra-agent/projects/lfm3 ---> lfm3, lfm
ai init ~/.config/pyra-agent/projects/ling-tiny ---> ling, ling-tiny
ai init ~/.config/pyra-agent/projects/clover ---> clover, clov

# --- Quick Tools ---
curl -s "wttr.in/?format=3" ---> weather, wttr
date "+Current Date: %A, %B %-d, %Y" ---> get date, today
```

---

## 👤 Workspace Profiles

Workspace behaviors are declared using Markdown profiles with YAML frontmatter in `profiles/*.md`:

```markdown
---
name: lfm3
description: "Liquid AI LFM3-1.7B Ultra-Fast Edge Systems Engineer"
yolo: true
reasoning_budget: 500
box: 1
---
ROLE: Liquid AI LFM3-1.7B Autonomous Edge Systems & Software Engineer.

DIRECTIVES:
- TOOL EXECUTION: Emit tool calls immediately. Never output introductory commentary.
- CLOSED-LOOP EXECUTION: inspect -> edit -> verify.
- HALT: Stop immediately with "✓ Task complete: <10-word summary>".
```

---

## 🔌 Provider Configuration Cascade

`pyra-agent` discovers models top-down from `~/.config/pyra-agent/.env` without requiring third-party SDKs:

1. **CUSTOM2:** `CUSTOM2_API_KEY` (xAI Grok, TokenHarbor, DeepSeek)
2. **CUSTOM:** `CUSTOM_API_KEY` (Hugging Face Inference Endpoints)
3. **OPENROUTER:** `OPENROUTER_API_KEY` (OpenRouter API)
4. **Local Fallback:** Auto-queries `http://127.0.0.1:8080/v1/models` (local `llama-server`) and extracts the loaded `.gguf` model name automatically.

---

## ⌨️ In-Session Commands

| Command | Action |
| :--- | :--- |
| `/box [1-3]` | Cycle or switch terminal box preset dynamically. |
| `/yolo` (or `/g`) | Toggle autonomous execution mode (bypasses in-bounds prompts). |
| `/clear` (or `/c`) | Reset conversation history and redraw clean terminal header. |
| `q` or `exit` | Gracefully exit session (status code `0`). |
| `Ctrl+C` | Cleanly exits back to shell with `exit 0` (`~ ❯`). |

---

## 🚀 Installation & Build

```bash
# Clone and enter directory
cd ~/.config/pyra-agent

# Build optimized release binary (< 2s incremental)
cargo build --release

# Symlink to PATH
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/pyra-agent" ~/.local/bin/ai
ln -sf "$PWD/target/release/pyra-agent" ~/.local/bin/pyra

# Source the hook in ~/.bashrc or ~/.zshrc
echo 'source ~/.config/pyra-agent/pyra-hook.sh' >> ~/.zshrc
```

---

<div align="center">
Built for speed, clarity, and terminal minimalism.
</div>
