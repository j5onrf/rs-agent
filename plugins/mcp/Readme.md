# Model Context Protocol (MCP) Integration: `plugins/mcp/`

Native, zero-overhead stdio Model Context Protocol (MCP) client for `rs-agent`. Connect external desktop apps, databases, and web services dynamically without modifying core engine code.

---

## 1. Registering an MCP Server

Add server definitions to `~/.config/rs-agent/plugins/mcp/servers.json`. Supports environment variable expansion (`${VAR}`) and home expansion (`${HOME}` or `~`):

```json
{
  "servers": {
    "photocraft": {
      "command": "${HOME}/.config/rs-agent/plugins/mcp/photocraft/mcp-bridge.sh",
      "args": [],
      "env": {
        "PHOTOCRAFT_AUTOSTART": "1"
      }
    },
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

To prevent overwhelming models with dozens of external tool definitions, MCP servers are **opt-in per profile** via Markdown frontmatter (`profiles/*.md`):

```yaml
---
description: "Blender 3D Automation Specialist"
mcp: blender                # Attach server (comma-separated for multiple: blender, sqlite)
# Optional tool allowlist (keeps prefill small for local SLMs):
mcp_tools: create_mesh, run_script
---
ROLE: Blender 3D Automation Specialist.
```

### Selective Tool Filtering (`mcp_tools:`)
When an MCP server exports dozens of tools (e.g. PhotoCraft with 26 tools), raw schemas can consume 7,000+ tokens of prompt space. Use `mcp_tools:` to selectively load only the tools you need:

- **Omitted (Cloud/Large Models):** Attaches all exported server tools in full detail.
- **Filtered (`mcp_tools: tool_a, tool_b`):** Loads only the specified tools and compacts verbose descriptions to the first sentence (`<= 120` chars), slashing prompt overhead down to < 2k tokens.
- **Box 7 Status Display:** Shows `mcp:<server>` when full, or `mcp:<server>(count)` when filtered (e.g., `mcp:photocraft(7)`).

---

## 3. CLI Debugging & Inspection

Inspect and test connected MCP servers directly from your terminal:

```bash
ai mcp list                 # List all configured servers and status
ai mcp list photocraft      # List tools exported by a specific server
ai mcp call photocraft doc_inspect '{"document_id": 1}'
ai mcp schema photocraft    # Export OpenAI-compatible tool schemas for inspection
```
