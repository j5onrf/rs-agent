use std::fs;
use std::path::PathBuf;

pub struct ContextEntry {
    pub command: String,
    pub aliases: Vec<String>,
}

impl ContextEntry {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains("--->") {
            return None;
        }

        let parts: Vec<&str> = line.splitn(2, "--->").collect();
        let command = parts[0].trim().to_string();
        let aliases = parts[1]
            .split(',')
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();

        Some(Self { command, aliases })
    }
}

pub struct ContextRouter {
    entries: Vec<ContextEntry>,
}

impl ContextRouter {
    pub fn load() -> Self {
        let path = dirs::home_dir()
            .map(|h| h.join(".config/pyra-agent/pyra-context.md"))
            .filter(|p| p.exists())
            .or_else(|| dirs::home_dir().map(|h| h.join(".config/py-agent/ai-context.md")))
            .unwrap_or_else(|| PathBuf::from("pyra-context.md"));

        let mut entries = Vec::new();
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                if let Some(entry) = ContextEntry::parse(line) {
                    entries.push(entry);
                }
            }
        }

        Self { entries }
    }

    pub fn resolve(&self, query: &str) -> Option<String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return None;
        }

        // Pass 1: Exact alias match
        for entry in &self.entries {
            if entry.aliases.iter().any(|a| a == &q) {
                return Some(entry.command.clone());
            }
        }

        // Pass 2: Prefix / Substring match
        for entry in &self.entries {
            if entry.aliases.iter().any(|a| a.starts_with(&q) || q.starts_with(a)) {
                return Some(entry.command.clone());
            }
        }

        None
    }
}
