# Model Context Protocol (MCP) Integration: `plugins/mcp/`

Native, zero-overhead stdio Model Context Protocol (MCP) client for `rs-agent`. Connect external desktop apps, databases, and web services dynamically without modifying core engine code.

---

## 1. Registering an MCP Server

Add server definitions to `~/.config/rs-agent/plugins/mcp/servers.json`. Supports environment variable expansion (`${VAR}`) and home expansion (`${HOME}` or `~`):

```json
{
  "servers": {
    "blender": {
      "command": "python3",
      "args": ["-m", "blender_mcp", "--addon-port", "9876"],
      "env": {}
    },
    "sqlite": {
      "command": "uvx",
      "args": ["mcp-server-sqlite", "--db-path", "${HOME}/.config/rs-agent/plugins/mcp/mcp.db"],
      "env": {}
    }
  }
}
```

---

## 2. Attaching to Profiles (Opt-In Token Hygiene)

To prevent overwhelming small models with 40+ tool definitions, MCP servers are **opt-in per profile** via Markdown frontmatter (`profiles/*.md`):

```yaml
---
description: "Blender 3D Automation Specialist"
mcp: blender                # Clean string: no brackets required!
# or multiple: mcp: blender, sqlite
---
ROLE: Blender 3D Automation Specialist. Use mcp_blender_* tools to build meshes and run Python scripts.
```

When you select this profile via `/p` in the terminal or launch `/t3`, the engine dynamically discovers the tools exported by the server (`mcp_<server>_<tool>`) and attaches them to the session.

---

## 3. CLI Debugging & Inspection

Inspect and test connected MCP servers directly from your terminal:

```bash
ai mcp list                 # List all configured servers and status
ai mcp list photocraft      # List tools exported by a specific server
ai mcp call photocraft doc_inspect '{"document_id": 1}'
ai mcp schema               # Export OpenAI-compatible tool schemas for active servers
```
