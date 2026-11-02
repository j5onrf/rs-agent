use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolTier {
    PureBash,
    Smol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub profile_name: String,
    pub is_agent: bool,
    pub tier: ToolTier,
    #[serde(default)]
    pub adapters: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            profile_name: "chat".into(),
            is_agent: false,
            tier: ToolTier::Smol,
            adapters: false,
        }
    }
}

impl WorkspaceConfig {
    pub fn load_or_default<P: AsRef<Path>>(workspace: P) -> Self {
        let candidates = [
            workspace.as_ref().join(".rs").join("config.json"),
            workspace.as_ref().join(".rs-agent").join("config.json"),
        ];
        for cfg_path in &candidates {
            if cfg_path.exists() {
                if let Ok(content) = fs::read_to_string(cfg_path) {
                    if let Ok(cfg) = serde_json::from_str::<WorkspaceConfig>(&content) {
                        return cfg;
                    }
                }
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

pub const DEFAULT_ENV: &str = r#"# ==============================================================================
# Rs-Agent Provider Cascade (.env)
# Priority: Top-down first active (uncommented) key.
# Fallback: When all keys are commented with '#', connects to local llama-server (:8080).
# Configure interactively with: ms (or models)
# ==============================================================================

#CUSTOM_API_KEY="hf_your_key_here"
CUSTOM_MODEL="space"

# ── 2. Custom 2 / TokenHarbor (DeepSeek / xAI) ────────────────────────────────
#CUSTOM2_API_KEY="thk_live_your_key_here"
CUSTOM2_URL="https://tokenharbor.ai/v1/chat/completions"
CUSTOM2_MODEL="deepseek-v4.1-flash:free"

# ── 3. Custom 3 / Google Gemini 3.8 Flash (OpenAI Endpoint) ───────────────────
#CUSTOM3_API_KEY="AQ_your_key_here"
CUSTOM3_URL="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
CUSTOM3_MODEL="gemini-3.8-flash"

# ── 4. Custom 4 / Google Gemini 3.5 Flash Lite ─────────────────────────────────
#CUSTOM4_API_KEY="AQ_your_key_here"
CUSTOM4_URL="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
CUSTOM4_MODEL="gemini-3.5-flash-lite"

# ── 5. Custom 5 / Local PyRouter / Custom Endpoint ────────────────────────────
#CUSTOM5_API_KEY="not-needed"
#CUSTOM5_URL="http://127.0.0.1:20128/v1/chat/completions"
#CUSTOM5_MODEL="big-pickle"

# ── Google Gemini Direct ──────────────────────────────────────────────────────
#GEMINI_API_KEY="AQ_your_key_here"
GEMINI_MODEL="gemini-3.5-flash-lite"

# ── OpenRouter Universal Gateway ──────────────────────────────────────────────
#OPENROUTER_API_KEY="sk-or-v1-your_key_here"
OPENROUTER_MODEL="openrouter/free"

# ── Search Grounding (/gnd) ───────────────────────────────────────────────────
#GND_KEY="AQ_your_key_here"
GND_MODEL="gemini-2.5-flash"

# ── Context Ceilings & Execution Budget ───────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="15"
"#;

pub fn get_env_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/rs-agent/.env"))
        .unwrap_or_else(|| PathBuf::from(".env"))
}

pub fn ensure_env_file() {
    let path = get_env_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let default_content = r#"# Rs-Agent Provider Cascade (.env) — Priority: First active key | Switch with: ms
# Fallback: Local llama-server (:8080) when all cloud keys are commented with '#'.

# ── 1. Google Gemini ──────────────────────────────────────────────────────────
# CUSTOM1_NAME="Google Gemini"
# CUSTOM1_API_KEY="sk-your-key-here"
CUSTOM1_URL="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
CUSTOM1_MODEL="gemini-2.5-flash-lite"

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
# CUSTOM2_NAME="OpenRouter"
# CUSTOM2_API_KEY="sk-or-v1-your-key-here"
CUSTOM2_URL="https://openrouter.ai/api/v1/chat/completions"
CUSTOM2_MODEL="openrouter/free"

# ── 3. Custom Direct Endpoint ─────────────────────────────────────────────────
# Quick Reference URLs & Models:
#   • DeepSeek: https://api.deepseek.com/chat/completions        (deepseek-chat / deepseek-reasoner)
#   • xAI:      https://api.x.ai/v1/chat/completions             (grok-2-latest)
#   • OpenAI:   https://api.openai.com/v1/chat/completions       (gpt-4o / o3-mini)
#   • Groq:     https://api.groq.com/openai/v1/chat/completions  (llama-3.3-70b-versatile)
# CUSTOM3_NAME="Custom Provider"
# CUSTOM3_API_KEY="sk-your-key-here"
CUSTOM3_URL="https://api.deepseek.com/chat/completions"
CUSTOM3_MODEL="deepseek-chat"

# ── Local llama-server Fallback (Port 8080) ───────────────────────────────────
# Automatically used when all keys above are commented. Auto-detects loaded .gguf model.

# ── Runtime Governors ─────────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="10"
"#;
        let _ = std::fs::write(&path, default_content);
    }
}

pub fn get_max_tokens() -> usize {
    ensure_env_file();
    let env_path = get_env_path();

    if let Ok(content) = fs::read_to_string(env_path) {
        for l in content.lines() {
            let s = l.trim();
            if !s.starts_with('#') && s.contains("AI_MAX_TOKENS=") {
                let parts: Vec<&str> = s.splitn(2, '=').collect();
                if let Some(v) = parts.get(1) {
                    let clean = v.split('#').next().unwrap_or("").trim().trim_matches('"').trim_matches('\'');
                    if let Ok(n) = clean.parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }
    std::env::var("AI_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8192)
}

pub fn get_context_ceilings() -> ContextCeilings {
    let max_tokens = get_max_tokens();
    if max_tokens <= 16_384 {
        ContextCeilings {
            max_tokens,
            line_ceiling: 250,
            char_cap: 20_000,
            preview_lines: 100,
            search_cap: 30,
        }
    } else if max_tokens <= 32_768 {
        ContextCeilings {
            max_tokens,
            line_ceiling: 1_000,
            char_cap: 45_000,
            preview_lines: 300,
            search_cap: 50,
        }
    } else if max_tokens <= 64_000 {
        ContextCeilings {
            max_tokens,
            line_ceiling: 2_000,
            char_cap: 90_000,
            preview_lines: 600,
            search_cap: 75,
        }
    } else {
        ContextCeilings {
            max_tokens,
            line_ceiling: 4_000,
            char_cap: 180_000,
            preview_lines: 1_200,
            search_cap: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: usize,
}

async fn query_local_model_name() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(250))
        .build()
        .ok()?;
    let res = client.get("http://127.0.0.1:8080/v1/models").send().await.ok()?;
    if res.status().is_success() {
        let json: serde_json::Value = res.json().await.ok()?;
        if let Some(id) = json["data"].get(0).and_then(|m| m.get("id")).and_then(|s| s.as_str()) {
            let filename = Path::new(id).file_name()?.to_str()?;
            let clean = filename
                .trim_end_matches(".gguf")
                .split("-Q")
                .next()
                .unwrap_or(filename);
            return Some(clean.to_string());
        }
    }
    None
}

impl ProviderConfig {
    pub async fn load() -> Self {
        ensure_env_file();
        let env_path = get_env_path();

        let mut lines = Vec::new();
        if let Ok(content) = fs::read_to_string(env_path) {
            lines = content.lines().map(|s| s.trim().to_string()).collect();
        }

        let max_tokens = get_max_tokens();

        let get_active_key = |key_name: &str| -> Option<String> {
            for l in &lines {
                if !l.starts_with('#') && l.contains('=') {
                    let parts: Vec<&str> = l.splitn(2, '=').collect();
                    let k = parts[0].trim().trim_start_matches("export ");
                    if k == key_name {
                        let v = parts[1].split('#').next().unwrap_or("").trim();
                        let clean = v.trim_matches('"').trim_matches('\'');
                        if clean == "not-needed" || (clean.len() >= 8 && !clean.contains("your_key_here")) {
                            return Some(clean.to_string());
                        }
                    }
                }
            }
            None
        };

        let get_var = |var_name: &str| -> Option<String> {
            for l in &lines {
                if !l.starts_with('#') && l.contains('=') {
                    let parts: Vec<&str> = l.splitn(2, '=').collect();
                    let k = parts[0].trim().trim_start_matches("export ");
                    if k == var_name {
                        let v = parts[1].split('#').next().unwrap_or("").trim();
                        return Some(v.trim_matches('"').trim_matches('\'').to_string());
                    }
                }
            }
            std::env::var(var_name).ok()
        };

        let custom_prefixes = [
            "CUSTOM", "CUSTOM2", "CUSTOM3", "CUSTOM4", "CUSTOM5",
            "CUSTOM6", "CUSTOM7", "CUSTOM8", "CUSTOM9"
        ];
        for prefix in custom_prefixes {
            let key_name = format!("{}_API_KEY", prefix);
            if let Some(key) = get_active_key(&key_name) {
                let url_name = format!("{}_URL", prefix);
                let mod_name = format!("{}_MODEL", prefix);
                return Self {
                    base_url: get_var(&url_name).unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into()),
                    api_key: key,
                    model: get_var(&mod_name).unwrap_or_else(|| "default".into()),
                    max_tokens,
                };
            }
        }

        if let Some(key) = get_active_key("OPENROUTER_API_KEY") {
            return Self {
                base_url: get_var("OPENROUTER_URL").unwrap_or_else(|| "https://openrouter.ai/api/v1/chat/completions".into()),
                api_key: key,
                model: get_var("OPENROUTER_MODEL").unwrap_or_else(|| "openrouter/free".into()),
                max_tokens,
            };
        }

        if let Some(key) = get_active_key("GEMINI_API_KEY") {
            return Self {
                base_url: get_var("GEMINI_URL").unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".into()),
                api_key: key,
                model: get_var("GEMINI_MODEL").unwrap_or_else(|| "gemini-3.5-flash-lite".into()),
                max_tokens,
            };
        }

        let local_model = query_local_model_name().await.unwrap_or_else(|| "LFM3-1.7B".into());
        Self {
            base_url: "http://127.0.0.1:8080/v1/chat/completions".into(),
            api_key: "not-needed".into(),
            model: local_model,
            max_tokens,
        }
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
                let active_lines: Vec<&str> = content
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect();

                if !active_lines.is_empty() {
                    return Some(active_lines.join("\n"));
                }
            }
        }
    }
    None
}
