use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Deserialize, Debug, Clone)]
struct ServersFile {
    #[serde(default)]
    servers: HashMap<String, McpServerConfig>,
}

fn get_mcp_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/rs-agent/plugins/mcp/servers.json"))
        .unwrap_or_else(|| PathBuf::from("plugins/mcp/servers.json"))
}

pub fn load_servers_config() -> HashMap<String, McpServerConfig> {
    let p = get_mcp_config_path();
    if !p.is_file() {
        return HashMap::new();
    }
    fs::read_to_string(&p)
        .ok()
        .and_then(|c| serde_json::from_str::<ServersFile>(&c).ok())
        .map(|f| f.servers)
        .unwrap_or_default()
}

pub fn expand_env_str(val: &str) -> String {
    let mut out = val.to_string();
    if out == "~" {
        return dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or(out);
    }
    if let Some(stripped) = out.strip_prefix("~/") {
        if let Some(h) = dirs::home_dir() {
            return h.join(stripped).to_string_lossy().to_string();
        }
    }

    while let Some(start) = out.find("${") {
        if let Some(end) = out[start..].find('}') {
            let var_name = &out[start + 2..start + end];
            let val = std::env::var(var_name).unwrap_or_default();
            out.replace_range(start..start + end + 1, &val);
            continue;
        }
        break;
    }
    out
}

async fn ensure_photocraft_bridge(clean_cmd: &str, clean_args: &[String]) {
    if !clean_cmd.contains("photocraft") && !clean_args.iter().any(|a| a.contains("photocraft")) {
        return;
    }
    if let Some(pos) = clean_args.iter().position(|a| a == "--bridge") {
        if let Some(addr) = clean_args.get(pos + 1) {
            // Check if socket is already accepting connections
            if tokio::net::TcpStream::connect(addr).await.is_err() {
                let token_file = expand_env_str("${HOME}/.config/photocraft/control.token");
                let home = expand_env_str("${HOME}");
                let port = addr.split(':').next_back().unwrap_or("7878");

                // Auto-spawn PhotoCraft desktop GUI in background with control socket enabled
                let _ = std::process::Command::new("photocraft")
                    .arg("--control")
                    .arg(port)
                    .arg("--control-token-file")
                    .arg(&token_file)
                    .arg("--automation-read-root")
                    .arg(&home)
                    .arg("--automation-write-root")
                    .arg(&home)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                // Wait up to 2 seconds for PhotoCraft to initialize socket
                for _ in 0..20 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if tokio::net::TcpStream::connect(addr).await.is_ok() {
                        break;
                    }
                }
            }
        }
    }
}

pub struct McpSession {
    child: Child,
    reader: BufReader<tokio::process::ChildStdout>,
    req_id: u64,
}

impl McpSession {
    pub async fn spawn(cfg: &McpServerConfig) -> Result<Self> {
        let clean_cmd = expand_env_str(&cfg.command);
        let clean_args: Vec<String> = cfg.args.iter().map(|a| expand_env_str(a)).collect();

        ensure_photocraft_bridge(&clean_cmd, &clean_args).await;

        let mut cmd = Command::new(&clean_cmd);
        cmd.args(&clean_args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);

        for (k, v) in &cfg.env {
            cmd.env(k, expand_env_str(v));
        }

        let mut child = cmd.spawn().with_context(|| format!("Failed to spawn MCP server '{}'", clean_cmd))?;
        let stdout = child.stdout.take().context("No stdout on MCP child")?;
        let reader = BufReader::new(stdout);

        let mut session = Self { child, reader, req_id: 0 };

        session.initialize().await?;
        Ok(session)
    }

    async fn send_raw(&mut self, payload: &Value) -> Result<()> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let mut line = serde_json::to_string(payload)?;
            line.push('\n');
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    async fn send_rpc(&mut self, method: &str, params: Value) -> Result<Option<Value>> {
        self.req_id += 1;
        let id = self.req_id;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        self.send_raw(&req).await?;

        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.reader.read_line(&mut line),
            )
            .await
            .context("MCP server RPC timed out after 30s")??;
            if bytes_read == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(data) = serde_json::from_str::<Value>(trimmed) {
                if data.get("id").and_then(|v| v.as_u64()) == Some(id) {
                    if let Some(err) = data.get("error") {
                        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown MCP error");
                        anyhow::bail!("MCP JSON-RPC error: {}", msg);
                    }
                    return Ok(data.get("result").cloned());
                }
            }
        }
        Ok(None)
    }

    async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notif = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.send_raw(&notif).await
    }

    async fn initialize(&mut self) -> Result<()> {
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "rs-agent-mcp", "version": "0.4.6" }
        });
        self.send_rpc("initialize", init_params).await?;
        self.send_notification("notifications/initialized", json!({})).await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<Value>> {
        let res = self.send_rpc("tools/list", json!({})).await?;
        let tools = res.and_then(|v| v.get("tools").and_then(|t| t.as_array().cloned())).unwrap_or_default();
        Ok(tools)
    }

    pub async fn call_tool(&mut self, tool_name: &str, arguments: &Value) -> Result<String> {
        let res = self
            .send_rpc(
                "tools/call",
                json!({
                    "name": tool_name,
                    "arguments": arguments
                }),
            )
            .await?;

        let res_val = match res {
            Some(v) => v,
            None => return Ok("[mcp error: No response from server]".to_string()),
        };

        if res_val.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) {
            let err_text = res_val
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("Execution failed");
            anyhow::bail!("{}", err_text);
        }

        let content_arr = res_val.get("content").and_then(|c| c.as_array());
        if let Some(arr) = content_arr {
            let mut texts = Vec::new();
            for item in arr {
                if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        texts.push(t);
                    }
                }
            }
            if !texts.is_empty() {
                return Ok(texts.join("\n").trim().to_string());
            }
        }

        Ok(serde_json::to_string_pretty(&res_val).unwrap_or_default())
    }
}

type McpSchemaCache = Arc<Mutex<HashMap<String, Vec<Value>>>>;
static SCHEMA_CACHE: OnceLock<McpSchemaCache> = OnceLock::new();

fn get_cache() -> &'static Arc<Mutex<HashMap<String, Vec<Value>>>> {
    SCHEMA_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

pub async fn get_server_tools_schema(server_name: &str) -> Result<Vec<Value>, String> {
    {
        let cache = get_cache().lock().await;
        if let Some(cached) = cache.get(server_name) {
            return Ok(cached.clone());
        }
    }

    let servers = load_servers_config();
    let cfg = servers.get(server_name).ok_or_else(|| format!("Unknown MCP server '{}'", server_name))?;

    let mut session = McpSession::spawn(cfg).await.map_err(|e| e.to_string())?;
    let raw_tools = session.list_tools().await.map_err(|e| e.to_string())?;

    let mut schemas = Vec::new();
    for t in raw_tools {
        let orig_name = t.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
        let desc = t.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let params = t.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} }));

        schemas.push(json!({
            "type": "function",
            "function": {
                "name": format!("mcp_{}_{}", server_name, orig_name),
                "description": format!("[{}] {}", server_name, desc),
                "parameters": params
            }
        }));
    }

    {
        let mut cache = get_cache().lock().await;
        cache.insert(server_name.to_string(), schemas.clone());
    }

    Ok(schemas)
}

pub async fn call_tool(server_name: &str, tool_name: &str, arguments: &Value) -> Result<String, String> {
    let servers = load_servers_config();
    let cfg = servers.get(server_name).ok_or_else(|| format!("Unknown MCP server '{}'", server_name))?;

    let mut session = McpSession::spawn(cfg).await.map_err(|e| e.to_string())?;
    session.call_tool(tool_name, arguments).await.map_err(|e| e.to_string())
}
