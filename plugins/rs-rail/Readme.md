# rs-rail (`plugins/rs-rail`)

Native Rust local HTTP+SSE web surface for `rs-agent` — zero changes to `rs-agent` core.

* `rail` or `rr` — opens frameless desktop window (`--app=`)
* `rail web`  or `rr web` — opens in default browser tab

---

## Micro-Bots (`rs-bot`)

Consult sibling workspaces read-only from inside `rs-rail` without switching projects or risking edits.

### Guardrails
* **Strictly read-only (depth = 1):** Agent targets get `read_file`, `list_dir`, and `search_code` scoped to the **caller's** workspace (never edits, `run_command`, or recursive `call_bot`). `chat` targets get `no tools`.
* **Reasoning isolated:** Caller `<think>` blocks are never forwarded in peer context envelopes.

### Ways to Call
* **Auto-call (`call_bot`):** When an agent workspace has linked `collaborators`, its LLM receives `call_bot` automatically (context controlled by `call_mode`, defaulting to `last_turn`).
* **Picker modal (`/call`):** Click `/call` in **Actions**, hover a project in the left sidebar and click its message icon, or type `/call`.
  * **Prompt only** *(default)* — sends only your prompt + workspace file list.
  * **Last turn** — sends the caller's last user/assistant exchange + your prompt.
  * **Full** — sends a condensed conversation digest (up to 20k chars) + your prompt.
* **Inline slash:** `/call <bot>: <prompt>` sends immediately using **Prompt only** (`none`); use the `/call` modal for **Last turn** or **Full**.

### Configuration

**Profile frontmatter (`profiles/*.md`):**
```yaml
collaborators: [gem35, nemo]   # Sibling bots available to call_bot (max 8)
call_mode: last_turn          # Context for auto-calls: last_turn | full | none
provider: GEMINI              # Optional backend slot when other bots call this one
```

**UI controls (persisted in `plugins/rs-rail/config.json`, overriding frontmatter):**
* **Top-left bot icon:** Pick a unique micro-bot glyph (`project_bots`) and toggle **Can auto-call** collaborators (`project_collabs`). Linked peers also appear as `/call <bot>` shortcuts in **Actions**.
* **Settings → Projects:** Assign each workspace's **Peer provider** (`Active (Default)`, `Local (:8080)`, or any `.env` slot — `project_providers`). Direct chat in that workspace still uses the composer's global model.
* **Settings → Appearance → Micro-Bot Mode:** Choose **Micro-Bots** (enables `rs-bot`; `mb` shows in the topbar when `collaborators` or `peer_provider` is set), **Letters**, or **None** (disables all `call_bot` injection and `/call` UI).
