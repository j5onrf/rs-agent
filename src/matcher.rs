use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ContextEntry {
    pub command: String,
    pub aliases: Vec<String>,
    pub gated: bool,
}

impl ContextEntry {
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        let parts: Vec<&str> = trimmed.split("--->").collect();
        if parts.len() != 2 {
            return None;
        }

        let mut raw_cmd = parts[0].trim();
        let gated = if raw_cmd.starts_with("[gate]") {
            raw_cmd = raw_cmd["[gate]".len()..].trim();
            true
        } else {
            false
        };
        let command = raw_cmd.to_string();

        let aliases: Vec<String> = parts[1]
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        if aliases.is_empty() {
            return None;
        }

        Some(Self { command, aliases, gated })
    }
}

pub struct ContextRouter {
    pub entries: Vec<ContextEntry>,
}

impl ContextRouter {
    pub fn load() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let ctx_path = PathBuf::from(home).join(".config/rs-agent/rs-context.md");
        let mut entries = Vec::new();

        if let Ok(content) = fs::read_to_string(ctx_path) {
            for line in content.lines() {
                if let Some(entry) = ContextEntry::parse(line) {
                    entries.push(entry);
                }
            }
        }

        Self { entries }
    }

    pub fn resolve(&self, query: &str) -> Option<String> {
        self.resolve_entry(query).map(|e| e.command)
    }

    pub fn resolve_entry(&self, query: &str) -> Option<ContextEntry> {
        let q = query.trim().to_lowercase();
        for entry in &self.entries {
            if entry.aliases.iter().any(|a| a == &q) {
                return Some(entry.clone());
            }
        }
        None
    }
}
