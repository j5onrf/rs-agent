use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub fn strip_inline_comment(s: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' if !in_single => in_double = !in_double,
            b'\'' if !in_double => in_single = !in_single,
            b'#' if !in_single && !in_double && (i == 0 || bytes[i - 1].is_ascii_whitespace()) => {
                return s[..i].trim();
            }
            _ => {}
        }
    }
    s.trim()
}

fn get_env_val(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(val_part) = rest.strip_prefix('=') {
                let val = strip_inline_comment(val_part.trim());
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
    #[serde(default)]
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
        let final_path = dir.join("config.json");
        let tmp_path = dir.join(format!("config.json.tmp.{}", std::process::id()));
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &final_path)?;
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

pub fn get_env_path() -> PathBuf {
    dirs::home_dir().map(|h| h.join(".config/rs-agent/.env")).unwrap_or_else(|| PathBuf::from(".env"))
}

static PROFILE_MAX_TOKENS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_profile_max_tokens(val: Option<usize>) {
    PROFILE_MAX_TOKENS.store(val.unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
}

pub fn get_max_tokens() -> usize {
    let profile_ctx = PROFILE_MAX_TOKENS.load(std::sync::atomic::Ordering::Relaxed);
    if profile_ctx > 0 {
        return profile_ctx;
    }
    if let Ok(content) = fs::read_to_string(get_env_path()) {
        if let Some(n) = get_env_val(&content, "AI_MAX_TOKENS").and_then(|v| v.parse::<usize>().ok()) {
            return n;
        }
    }
    std::env::var("AI_MAX_TOKENS").ok().and_then(|v| v.parse().ok()).unwrap_or(8192)
}

fn parse_max_rounds(raw: &str) -> Option<usize> {
    let t = raw.trim();
    if t == "0" || t.eq_ignore_ascii_case("unlimited") || t.eq_ignore_ascii_case("inf") {
        Some(usize::MAX)
    } else {
        t.parse::<usize>().ok().filter(|&n| n > 0)
    }
}

pub fn get_max_rounds() -> usize {
    if let Ok(content) = fs::read_to_string(get_env_path()) {
        if let Some(n) = get_env_val(&content, "AI_MAX_AGENT_ROUNDS").and_then(|v| parse_max_rounds(&v)) {
            return n;
        }
    }
    std::env::var("AI_MAX_AGENT_ROUNDS").ok().and_then(|v| parse_max_rounds(&v)).unwrap_or(15)
}

pub fn get_context_ceilings() -> ContextCeilings {
    let max_tokens = get_max_tokens();
    if max_tokens <= 16_384 {
        ContextCeilings { max_tokens, line_ceiling: 250, char_cap: 20_000, preview_lines: 100, search_cap: 30 }
    } else if max_tokens <= 32_768 {
        ContextCeilings { max_tokens, line_ceiling: 1_000, char_cap: 45_000, preview_lines: 300, search_cap: 50 }
    } else if max_tokens <= 65_536 {
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
    pub fallback_models: Vec<String>,
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

fn parse_model_list(raw: &str) -> (String, Vec<String>) {
    let mut parts = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ToString::to_string);
    let primary = parts.next().unwrap_or_else(|| "default".to_string());
    (primary, parts.collect())
}

fn normalize_chat_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") || trimmed.ends_with("/openai") || trimmed.ends_with("/v1beta") {
        format!("{}/chat/completions", trimmed)
    } else {
        url.trim().to_string()
    }
}

impl ProviderConfig {
    pub async fn load() -> Self {
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
            ("OPENAI", "https://api.openai.com/v1/chat/completions", "gpt-4o"),
            ("ANTHROPIC", "https://api.anthropic.com/v1/chat/completions", "claude-3-5-sonnet-20241022"),
            ("DEEPSEEK", "https://api.deepseek.com/chat/completions", "deepseek-chat"),
            ("GROQ", "https://api.groq.com/openai/v1/chat/completions", "llama-3.3-70b-versatile"),
        ];

        for &(pfx, def_url, def_model) in DIRECT_PROVIDERS {
            let key_var = format!("{}_API_KEY", pfx);
            if let Some(api_key) = get_env_val(&env_content, &key_var) {
                if is_valid_key(&api_key) {
                    let raw_model =
                        get_env_val(&env_content, &format!("{}_MODEL", pfx)).unwrap_or_else(|| def_model.to_string());
                    let (model, fallback_models) = parse_model_list(&raw_model);
                    let base_url = normalize_chat_url(
                        &get_env_val(&env_content, &format!("{}_URL", pfx)).unwrap_or_else(|| def_url.to_string()),
                    );

                    return ProviderConfig { base_url, api_key: api_key.trim().to_string(), model, fallback_models, max_tokens };
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
                    let base_url = normalize_chat_url(
                        &get_env_val(&env_content, &url_var)
                            .or_else(|| legacy_url.and_then(|k| get_env_val(&env_content, k)))
                            .unwrap_or_else(|| "https://api.deepseek.com/chat/completions".to_string()),
                    );

                    let model_var = format!("CUSTOM{}_MODEL", i);
                    let legacy_model = if i == 1 { Some("CUSTOM_MODEL") } else { None };
                    let raw_model = get_env_val(&env_content, &model_var)
                        .or_else(|| legacy_model.and_then(|k| get_env_val(&env_content, k)))
                        .unwrap_or_else(|| "default".to_string());
                    let (model, fallback_models) = parse_model_list(&raw_model);

                    return ProviderConfig { base_url, api_key: api_key.trim().to_string(), model, fallback_models, max_tokens };
                }
            }
        }

        // 3. Fallback: Local llama-server on :8080
        let base_url = "http://127.0.0.1:8080/v1/chat/completions".to_string();
        let model = probe_local_model().await;

        ProviderConfig { base_url, api_key: "not-needed".to_string(), model, fallback_models: Vec::new(), max_tokens }
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_env_val_preserves_quoted_hash_and_strips_comments() {
        let env = "API_KEY=\"sk#123\" # trailing note\nMODEL='gemini-4-hydrogen' # free\n";
        assert_eq!(get_env_val(env, "API_KEY").as_deref(), Some("sk#123"));
        assert_eq!(get_env_val(env, "MODEL").as_deref(), Some("gemini-4-hydrogen"));
    }

    #[test]
    fn test_normalize_chat_url_appends_chat_completions_for_base_paths() {
        assert_eq!(
            normalize_chat_url("https://api.tokenlab.amd.com/v1/"),
            "https://api.tokenlab.amd.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_url("https://api.tokenlab.amd.com/v1/chat/completions"),
            "https://api.tokenlab.amd.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_parse_model_list_handles_fallbacks_and_trailing_comma() {
        let (primary, fallbacks) = parse_model_list("DeepSeek-V4.1-Flash, Qwen3.8-27B, Gemma-4-31B-IT,");
        assert_eq!(primary, "DeepSeek-V4.1-Flash");
        assert_eq!(fallbacks, vec!["Qwen3.8-27B", "Gemma-4-31B-IT"]);
    }

    #[test]
    fn test_parse_max_rounds_supports_unlimited() {
        assert_eq!(parse_max_rounds("0"), Some(usize::MAX));
        assert_eq!(parse_max_rounds("unlimited"), Some(usize::MAX));
        assert_eq!(parse_max_rounds("20"), Some(20));
        assert_eq!(parse_max_rounds(""), None);
    }
}
