#![allow(dead_code)]

use crate::config::ToolTier;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub tier: ToolTier,
    pub yolo: bool,
    pub thinking_budget: u32,
    pub box_style: Option<usize>,
    pub system_prompt: String,
}

impl Profile {
    pub fn parse(name: &str, raw_content: &str) -> Self {
        let mut tier = ToolTier::Smol;
        let mut yolo = false;
        let mut thinking_budget = 500;
        let mut box_style = None;
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
                            "reasoning_budget" | "thinking_budget" | "budget" => {
                                if let Ok(b) = val.parse::<u32>() {
                                    thinking_budget = b;
                                }
                            }
                            "box" | "box_style" => {
                                if let Ok(b) = val.parse::<usize>() {
                                    box_style = Some(b.clamp(1, 3));
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
            thinking_budget,
            box_style,
            system_prompt: body,
        }
    }

    pub fn load(profile_name: &str) -> Option<Self> {
        let base_dirs = [
            dirs::home_dir().map(|h| h.join(".config/rs-agent/profiles")),
            dirs::home_dir().map(|h| h.join(".config/py-agent/skills/profiles")),
        ];

        for dir in base_dirs.into_iter().flatten() {
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
        let mut list = Vec::new();
        if let Some(dir) = dirs::home_dir().map(|h| h.join(".config/rs-agent/profiles")) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            list.push(stem.to_string());
                        }
                    }
                }
            }
        }
        list.sort();
        list
    }

    pub fn select_interactive(target_workspace: &Path) -> Option<String> {
        let options = Self::list_available();
        if options.is_empty() {
            return None;
        }

        let ws_name = target_workspace
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");

        println!("\n\x1b[1;36m[rs init]\x1b[0m Select Agent Profile for workspace \x1b[1;33m{}\x1b[0m:\n", ws_name);

        let mut sel_idx = 0;
        let num_opts = options.len();

        let _ = enable_raw_mode();
        let mut stdout = io::stdout();

        loop {
            for (idx, opt) in options.iter().enumerate() {
                if idx == sel_idx {
                    print!("\r\x1b[K\x1b[1;32m  > {:2}. {:<24}\x1b[0m\n", idx + 1, opt);
                } else {
                    print!("\r\x1b[K\x1b[37m    {:2}. {:<24}\x1b[0m\n", idx + 1, opt);
                }
            }
            print!("\r\x1b[K\n\r\x1b[K  \x1b[2m:: Enter: select | ↑/↓: navigate | Esc: default\x1b[0m\r");
            let _ = stdout.flush();

            if let Ok(Event::Key(key_event)) = event::read() {
                match key_event.code {
                    KeyCode::Up => {
                        sel_idx = (sel_idx + num_opts - 1) % num_opts;
                    }
                    KeyCode::Down => {
                        sel_idx = (sel_idx + 1) % num_opts;
                    }
                    KeyCode::Enter => {
                        let _ = disable_raw_mode();
                        print!("\r\x1b[J");
                        let chosen = options[sel_idx].clone();
                        println!("\x1b[1;32m✓ Profile set to: {}\x1b[0m\n", chosen);
                        return Some(chosen);
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') if key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        let _ = disable_raw_mode();
                        print!("\r\x1b[J\nCancelled.\n");
                        return None;
                    }
                    _ => {}
                }
            }

            print!("\x1b[{}A", num_opts + 2);
        }
    }
}
