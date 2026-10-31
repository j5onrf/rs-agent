#![allow(dead_code)]

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
    pub yolo: bool,
    #[serde(default)]
    pub adapters: bool,
    pub thinking_budget: u32,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            profile_name: "chat".into(),
            is_agent: false,
            tier: ToolTier::Smol,
            yolo: false,
            adapters: false,
            thinking_budget: 500,
        }
    }
}

impl WorkspaceConfig {
    pub fn load_or_default<P: AsRef<Path>>(workspace: P) -> Self {
        let candidates = [
            workspace.as_ref().join(".rs").join("config.json"),
            workspace.as_ref().join(".rs-agent").join("config.json"),
            workspace.as_ref().join(".pyra").join("config.json"),
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

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
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
        let env_candidates = [
            dirs::home_dir().map(|h| h.join(".config/rs-agent/.env")),
            dirs::home_dir().map(|h| h.join(".config/pyra-agent/.env")),
        ];

        let env_path = env_candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(".env"));

        let mut lines = Vec::new();
        if let Ok(content) = fs::read_to_string(env_path) {
            lines = content.lines().map(|s| s.trim().to_string()).collect();
        }

        let get_active_key = |key_name: &str| -> Option<String> {
            for l in &lines {
                if !l.starts_with('#') && l.contains('=') {
                    let parts: Vec<&str> = l.splitn(2, '=').collect();
                    let k = parts[0].trim().trim_start_matches("export ");
                    if k == key_name {
                        let v = parts[1].split('#').next().unwrap_or("").trim();
                        let clean = v.trim_matches('"').trim_matches('\'');
                        if clean.len() >= 8 && !clean.contains("your-") && !clean.contains("replace-me") {
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

        if let Some(key) = get_active_key("CUSTOM2_API_KEY") {
            return Self {
                base_url: get_var("CUSTOM2_URL").unwrap_or_else(|| "https://api.x.ai/v1/chat/completions".into()),
                api_key: key,
                model: get_var("CUSTOM2_MODEL").unwrap_or_else(|| "grok-beta".into()),
            };
        }

        if let Some(key) = get_active_key("CUSTOM_API_KEY") {
            return Self {
                base_url: get_var("CUSTOM_URL").unwrap_or_else(|| "https://router.huggingface.co/v1/chat/completions".into()),
                api_key: key,
                model: get_var("CUSTOM_MODEL").unwrap_or_else(|| "space".into()),
            };
        }

        if let Some(key) = get_active_key("OPENROUTER_API_KEY") {
            return Self {
                base_url: "https://openrouter.ai/api/v1/chat/completions".into(),
                api_key: key,
                model: get_var("OPENROUTER_MODEL").unwrap_or_else(|| "openrouter/free".into()),
            };
        }

        let local_model = query_local_model_name().await.unwrap_or_else(|| "LFM3-1.7B".into());
        Self {
            base_url: "http://127.0.0.1:8080/v1/chat/completions".into(),
            api_key: "not-needed".into(),
            model: local_model,
        }
    }
}

/// Loads active (uncommented) rules from system_instructions.md

/// Loads active (uncommented) rules from system_instructions.md
pub fn load_global_instructions() -> Option<String> {
    use std::fs;
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
