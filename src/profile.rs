#![allow(dead_code)]

use crate::config::ToolTier;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub tier: ToolTier,
    pub yolo: bool,
    pub adapters: bool,
    pub thinking_budget: u32,
    pub system_prompt: String,
}

impl Profile {
    pub fn parse(name: &str, raw_content: &str) -> Self {
        let mut tier = ToolTier::Smol;
        let mut yolo = false;
        let mut adapters = false;
        let mut thinking_budget = 500;
        let mut body = raw_content.to_string();

        let trimmed = raw_content.trim_start();
        if trimmed.starts_with("---") {
            let parts: Vec<&str> = trimmed.splitn(3, "---").collect();
            if parts.len() >= 3 {
                let fm = parts[1];
                body = parts[2].trim().to_string();

                for line in fm.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        let key = k.trim().to_lowercase();
                        let clean_v = v.split('#').next().unwrap_or("").trim();
                        let val = clean_v.trim_matches('"').trim_matches('\'').to_lowercase();

                        match key.as_str() {
                            "purebash" => {
                                if val == "true" || val == "1" || val == "yes" {
                                    tier = ToolTier::PureBash;
                                }
                            }
                            "yolo" => {
                                yolo = val == "true" || val == "1" || val == "yes";
                            }
                            "adapters" | "adapter" => {
                                adapters = val == "true" || val == "1" || val == "yes";
                            }
                            "reasoning_budget" | "thinking_budget" | "budget" => {
                                if let Ok(b) = val.parse::<u32>() {
                                    thinking_budget = b;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Self {
            name: name.to_string(),
            tier,
            yolo,
            adapters,
            thinking_budget,
            system_prompt: body,
        }
    }

    pub fn profile_dirs() -> Vec<PathBuf> {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return Vec::new(),
        };

        [
            home.join(".config/rs-agent/profiles"),
        ]
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
    }

    pub fn load(profile_name: &str) -> Option<Self> {
        for dir in Self::profile_dirs() {
            let direct = dir.join(format!("{}.md", profile_name));
            if direct.is_file() {
                if let Ok(content) = fs::read_to_string(&direct) {
                    return Some(Self::parse(profile_name, &content));
                }
            }

            let norm_target = profile_name.replace(['-', '_'], "").to_lowercase();
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem.replace(['-', '_'], "").to_lowercase() == norm_target {
                                if let Ok(content) = fs::read_to_string(&path) {
                                    return Some(Self::parse(stem, &content));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn list_available() -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut list = Vec::new();

        for dir in Self::profile_dirs() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem != "chat" && seen.insert(stem.to_string()) {
                                list.push(stem.to_string());
                            }
                        }
                    }
                }
            }
        }
        list.sort();
        list
    }

    fn is_local_env(base_url: Option<&str>, model_name: &str) -> bool {
        if let Some(url) = base_url {
            let u = url.to_lowercase();
            if u.contains("localhost") || u.contains("127.0.0.1") || u.contains(":8080") || u.contains(":11434") {
                return true;
            }
        }
        let m = model_name.to_lowercase();
        m.contains("local") || m.contains("llama-server") || m.contains("lfm") || m.contains("gguf")
    }

    pub fn find_workspace_candidates(ws_name: &str, available: &[String]) -> Vec<String> {
        let ws_clean = ws_name.replace(['-', '_'], "").to_lowercase();
        let mut matches = Vec::new();

        for opt in available {
            let opt_clean = opt.replace(['-', '_'], "").to_lowercase();
            if opt_clean == ws_clean {
                matches.push(opt.clone());
                continue;
            }
            for sep in ['-', '_'] {
                let prefix = format!("{}{}", ws_name.to_lowercase(), sep);
                if opt.to_lowercase().starts_with(&prefix) && !matches.contains(opt) {
                    matches.push(opt.clone());
                }
            }
            if opt_clean.starts_with(&ws_clean) && !matches.contains(opt) {
                matches.push(opt.clone());
            }
        }
        matches
    }

    pub fn find_model_candidates(model_name: &str, available: &[String]) -> Vec<String> {
        let m_lower = model_name.to_lowercase();
        let m_clean = m_lower.replace(['-', '_', '.', '/'], "");
        let mut matches = Vec::new();

        let tokens: Vec<&str> = model_name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .collect();

        for opt in available {
            let opt_lower = opt.to_lowercase();
            let opt_clean = opt_lower.replace(['-', '_'], "");

            if m_clean.contains(&opt_clean) || opt_clean.contains(&m_clean) {
                if !matches.contains(opt) {
                    matches.push(opt.clone());
                    continue;
                }
            }

            for token in &tokens {
                let tok_lower = token.to_lowercase();
                if opt_lower.contains(&tok_lower) || tok_lower.contains(&opt_lower) {
                    if !matches.contains(opt) {
                        matches.push(opt.clone());
                        break;
                    }
                }
            }

            if (tokens.iter().any(|t| t.starts_with("gem")) || m_lower.contains("gem"))
                && (opt_lower.starts_with("gem") || opt_lower.contains("-gem") || opt_lower.contains("_gem"))
            {
                if !matches.contains(opt) {
                    matches.push(opt.clone());
                }
            }
        }
        matches
    }

    pub fn resolve_for_workspace(ws_name: &str, model_name: &str, base_url: Option<&str>) -> String {
        let available = Self::list_available();
        if available.is_empty() {
            return ws_name.to_string();
        }

        let mut candidates = Self::find_workspace_candidates(ws_name, &available);

        if candidates.is_empty() {
            candidates = Self::find_model_candidates(model_name, &available);
        }

        if candidates.is_empty() {
            candidates = available.clone();
        }

        if candidates.len() == 1 {
            return candidates[0].clone();
        }

        let is_local = Self::is_local_env(base_url, model_name);
        let default_idx = candidates
            .iter()
            .position(|c| {
                let cl = c.to_lowercase();
                if is_local {
                    cl.contains("loc") || cl.contains("local")
                } else if model_name.to_lowercase().contains("gem") {
                    cl.contains("gem")
                } else if model_name.to_lowercase().contains("grok") {
                    cl.contains("grok")
                } else if model_name.to_lowercase().contains("claude") {
                    cl.contains("claude") || cl.contains("sonnet")
                } else {
                    false
                }
            })
            .unwrap_or(0);

        let title = format!("Multiple profiles match \x1b[1;33m{}\x1b[0m. Select one", ws_name);
        Self::select_menu(&title, &candidates, default_idx).unwrap_or_else(|| candidates[default_idx].clone())
    }

    pub fn select_interactive(target_workspace: &Path, current_profile: &str) -> Option<String> {
        let options = Self::list_available();
        if options.is_empty() {
            return None;
        }

        let ws_name = target_workspace
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");

        let default_idx = options.iter().position(|opt| opt == current_profile).unwrap_or(0);
        let title = format!("Select Agent Profile for workspace \x1b[1;33m{}\x1b[0m", ws_name);
        Self::select_menu(&title, &options, default_idx)
    }

    fn select_menu(prompt_title: &str, options: &[String], default_idx: usize) -> Option<String> {
        if options.is_empty() {
            return None;
        }
        if options.len() == 1 {
            return Some(options[0].clone());
        }

        let mut sel_idx = default_idx.min(options.len() - 1);
        let num_opts = options.len();
        let _ = enable_raw_mode();
        let mut stdout = io::stdout();

        print!("\x1b[?25l");

        print!("\r\x1b[2K\x1b[1;36m[ai]\x1b[0m {}:\r\n", prompt_title);
        print!("\r\x1b[2K\r\n");

        let mut first = true;
        let lines_to_ascend = num_opts + 3;

        loop {
            if !first {
                print!("\x1b[{}A\r", lines_to_ascend - 2);
            }
            first = false;

            for (idx, opt) in options.iter().enumerate() {
                if idx == sel_idx {
                    print!("\r\x1b[2K\x1b[1;36m  ❯ \x1b[1;37m{:2}. \x1b[1;36m{:<24}\x1b[0m\r\n", idx + 1, opt);
                } else {
                    print!("\r\x1b[2K\x1b[2m    {:2}. {:<24}\x1b[0m\r\n", idx + 1, opt);
                }
            }
            print!("\r\x1b[2K\r\n");
            print!("\r\x1b[2K  \x1b[2m:: Enter: confirm | ↑/↓: navigate | Esc: default\x1b[0m");
            let _ = stdout.flush();

            if let Ok(Event::Key(key_event)) = event::read() {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                match key_event.code {
                    KeyCode::Up => {
                        sel_idx = (sel_idx + num_opts - 1) % num_opts;
                    }
                    KeyCode::Down => {
                        sel_idx = (sel_idx + 1) % num_opts;
                    }
                    KeyCode::Enter => {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", lines_to_ascend);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        return Some(options[sel_idx].clone());
                    }
                    KeyCode::Esc
                    | KeyCode::Char('q')
                    | KeyCode::Char('c')
                        if key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", lines_to_ascend);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        return Some(options[default_idx.min(options.len() - 1)].clone());
                    }
                    _ => {}
                }
            }
        }
    }
}
