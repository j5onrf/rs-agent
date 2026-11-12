use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn get_env_val(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(val_part) = rest.strip_prefix('=') {
                let val = val_part.trim().split('#').next().unwrap_or(val_part).trim();
                let clean = val.trim_matches('"').trim_matches('\'').trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

#[inline]
fn is_valid_key(key: &str) -> bool {
    let k = key.trim();
    !k.is_empty() && !k.contains("your_key_here") && !k.contains("your-key-here")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolTier {
    PureBash,
    Core4,
    Smol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub profile_name: String,
    pub is_agent: bool,
    pub tier: ToolTier,
    #[serde(default)]
    pub adapters: bool,
    pub delegate: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self { profile_name: "chat".into(), is_agent: false, tier: ToolTier::Smol, adapters: false, delegate: false }
    }
}

impl WorkspaceConfig {
    pub fn load_or_default<P: AsRef<Path>>(workspace: P) -> Self {
        let cfg_path = workspace.as_ref().join(".rs/config.json");
        if let Ok(content) = fs::read_to_string(&cfg_path) {
            if let Ok(cfg) = serde_json::from_str::<WorkspaceConfig>(&content) {
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save<P: AsRef<Path>>(&self, workspace: P) -> Result<()> {
        let dir = workspace.as_ref().join(".rs");
        fs::create_dir_all(&dir)?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(dir.join("config.json"), content)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContextCeilings {
    pub max_tokens: usize,
    pub line_ceiling: usize,
    pub char_cap: usize,
    pub preview_lines: usize,
    pub search_cap: usize,
}

pub const DEFAULT_ENV: &str = r#"# Rs-Agent Provider Cascade (.env) — Priority: First active key | Switch with: ms
# Fallback: Local llama-server (:8080) when all cloud keys are commented with '#'.

# ── 1. Google Gemini ──────────────────────────────────────────────────────────
#GEMINI_API_KEY="AQ.your-key-here"
GEMINI_MODEL="gemini-3.5-flash-lite"

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
#OPENROUTER_API_KEY="sk-or-v1-your-key-here"
OPENROUTER_MODEL="openrouter/free"

# ── 3. TokenHarbor ────────────────────────────────────────────────────────────
#TOKENHARBOR_API_KEY="thk_live_your-key-here"
TOKENHARBOR_MODEL="deepseek-v4.1-flash:free"

# ── More Direct Providers e.g. ────────────────────────────────────────────────
# OPENAI_API_KEY="sk-your-key-here"
# ANTHROPIC_API_KEY="sk-ant-your-key-here"

# ── 4. Custom Direct Endpoint ─────────────────────────────────────────────────
# Add: Define CUSTOM1_*, CUSTOM2_*, up to CUSTOM9_*
# CUSTOM1_NAME="Custom Provider"
# CUSTOM1_API_KEY="sk-your-key-here"
CUSTOM1_URL="https://api.deepseek.com/chat/completions"
CUSTOM1_MODEL="deepseek-flash"

# ── MCP Server Keys (Optional) ────────────────────────────────────────────────
# API keys for tools configured in plugins/mcp/servers.json
# FIRECRAWL_API_KEY="fc-your-actual-api-key"

# ── Runtime Governors ─────────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="10"
"#;

pub fn get_env_path() -> PathBuf {
    dirs::home_dir().map(|h| h.join(".config/rs-agent/.env")).unwrap_or_else(|| PathBuf::from(".env"))
}

pub fn ensure_env_file() {
    let env_path = get_env_path();
    if !env_path.exists() {
        if let Some(parent) = env_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&env_path, DEFAULT_ENV);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&env_path, fs::Permissions::from_mode(0o600));
        }
    }
}

pub fn get_max_tokens() -> usize {
    ensure_env_file();
    if let Ok(content) = fs::read_to_string(get_env_path()) {
        if let Some(n) = get_env_val(&content, "AI_MAX_TOKENS").and_then(|v| v.parse::<usize>().ok()) {
            return n;
        }
    }
    std::env::var("AI_MAX_TOKENS").ok().and_then(|v| v.parse().ok()).unwrap_or(8192)
}

pub fn get_max_rounds() -> usize {
    ensure_env_file();
    if let Ok(content) = fs::read_to_string(get_env_path()) {
        if let Some(n) = get_env_val(&content, "AI_MAX_AGENT_ROUNDS").and_then(|v| v.parse::<usize>().ok()) {
            return n;
        }
    }
    std::env::var("AI_MAX_AGENT_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(15)
}

pub fn get_context_ceilings() -> ContextCeilings {
    let max_tokens = get_max_tokens();
    if max_tokens <= 16_384 {
        ContextCeilings { max_tokens, line_ceiling: 250, char_cap: 20_000, preview_lines: 100, search_cap: 30 }
    } else if max_tokens <= 32_768 {
        ContextCeilings { max_tokens, line_ceiling: 1_000, char_cap: 45_000, preview_lines: 300, search_cap: 50 }
    } else if max_tokens <= 64_000 {
        ContextCeilings { max_tokens, line_ceiling: 2_000, char_cap: 90_000, preview_lines: 600, search_cap: 75 }
    } else {
        ContextCeilings { max_tokens, line_ceiling: 4_000, char_cap: 180_000, preview_lines: 1_200, search_cap: 100 }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: usize,
}

async fn probe_local_model() -> String {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_millis(500)).build().unwrap_or_default();

    if let Ok(resp) = client.get("http://127.0.0.1:8080/v1/models").send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(id) = json.pointer("/data/0/id").and_then(|v| v.as_str()) {
                return id.to_string();
            }
        }
    }
    "local-model".to_string()
}

impl ProviderConfig {
    pub async fn load() -> Self {
        ensure_env_file();
        let env_path = get_env_path();
        let env_content = fs::read_to_string(&env_path).unwrap_or_default();
        let max_tokens = get_max_tokens();

        // 1. Direct Providers (Top Priority)
        const DIRECT_PROVIDERS: &[(&str, &str, &str)] = &[
            (
                "GEMINI",
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
                "gemini-3.5-flash-lite",
            ),
            (
                "GOOGLE",
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
                "gemini-3.5-flash-lite",
            ),
            ("OPENROUTER", "https://openrouter.ai/api/v1/chat/completions", "openrouter/free"),
            ("TOKENHARBOR", "https://tokenharbor.ai/v1/chat/completions", "deepseek-v4.1-flash:free"),
            ("OPENAI", "https://api.openai.com/v1/chat/completions", "gpt-4o"),
            ("ANTHROPIC", "https://api.anthropic.com/v1/messages", "claude-3-5-sonnet-20241022"),
            ("DEEPSEEK", "https://api.deepseek.com/chat/completions", "deepseek-chat"),
            ("GROQ", "https://api.groq.com/openai/v1/chat/completions", "llama-3.3-70b-versatile"),
        ];

        for &(pfx, def_url, def_model) in DIRECT_PROVIDERS {
            let key_var = format!("{}_API_KEY", pfx);
            if let Some(api_key) = get_env_val(&env_content, &key_var) {
                if is_valid_key(&api_key) {
                    let model =
                        get_env_val(&env_content, &format!("{}_MODEL", pfx)).unwrap_or_else(|| def_model.to_string());
                    let base_url =
                        get_env_val(&env_content, &format!("{}_URL", pfx)).unwrap_or_else(|| def_url.to_string());

                    return ProviderConfig { base_url, api_key: api_key.trim().to_string(), model, max_tokens };
                }
            }
        }

        // 2. Custom Slots: CUSTOM1..CUSTOM9
        for i in 1..=9 {
            let key_var = format!("CUSTOM{}_API_KEY", i);
            let legacy_key = if i == 1 { Some("CUSTOM_API_KEY") } else { None };

            let maybe_key =
                get_env_val(&env_content, &key_var).or_else(|| legacy_key.and_then(|k| get_env_val(&env_content, k)));

            if let Some(api_key) = maybe_key {
                if is_valid_key(&api_key) {
                    let url_var = format!("CUSTOM{}_URL", i);
                    let legacy_url = if i == 1 { Some("CUSTOM_URL") } else { None };
                    let base_url = get_env_val(&env_content, &url_var)
                        .or_else(|| legacy_url.and_then(|k| get_env_val(&env_content, k)))
                        .unwrap_or_else(|| "https://api.deepseek.com/chat/completions".to_string());

                    let model_var = format!("CUSTOM{}_MODEL", i);
                    let legacy_model = if i == 1 { Some("CUSTOM_MODEL") } else { None };
                    let model = get_env_val(&env_content, &model_var)
                        .or_else(|| legacy_model.and_then(|k| get_env_val(&env_content, k)))
                        .unwrap_or_else(|| "default".to_string());

                    return ProviderConfig { base_url, api_key: api_key.trim().to_string(), model, max_tokens };
                }
            }
        }

        // 3. Fallback: Local llama-server on :8080
        let base_url = "http://127.0.0.1:8080/v1/chat/completions".to_string();
        let model = probe_local_model().await;

        ProviderConfig { base_url, api_key: "not-needed".to_string(), model, max_tokens }
    }
}

pub fn load_global_instructions() -> Option<String> {
    let candidates = [
        dirs::home_dir().map(|h| h.join(".config/rs-agent/system_instructions.md")),
        dirs::home_dir().map(|h| h.join(".config/rs-agent/skills/system_instructions.md")),
    ];

    for path in candidates.into_iter().flatten() {
        if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                let active_lines: Vec<&str> =
                    content.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();

                if !active_lines.is_empty() {
                    return Some(active_lines.join("\n"));
                }
            }
        }
    }
    None
}
