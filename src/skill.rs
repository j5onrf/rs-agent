use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum SkillAction {
    Attach(String),
    Detach(String),
    DetachAll,
}

impl Skill {
    pub fn parse(name: &str, raw_content: &str) -> Self {
        let mut body = raw_content.to_string();

        let trimmed = raw_content.trim_start();
        if trimmed.starts_with("---") {
            let parts: Vec<&str> = trimmed.splitn(3, "---").collect();
            if parts.len() >= 3 {
                body = parts[2].trim().to_string();
            }
        }

        Self {
            name: name.to_string(),
            content: body,
        }
    }

    pub fn skill_dirs() -> Vec<PathBuf> {
        dirs::home_dir()
            .map(|h| vec![h.join(".config/rs-agent/skills")])
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.is_dir())
            .collect()
    }

    pub fn list_available() -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut list = Vec::new();

        for dir in Self::skill_dirs() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            let lower = stem.to_lowercase();
                            if lower != "readme" && lower != "system_instructions" && lower != "instructions" {
                                if seen.insert(stem.to_string()) {
                                    list.push(stem.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        list.sort();
        list
    }

    pub fn load(skill_name: &str) -> Option<Self> {
        for dir in Self::skill_dirs() {
            let direct = dir.join(format!("{}.md", skill_name));
            if direct.is_file() {
                if let Ok(content) = fs::read_to_string(&direct) {
                    return Some(Self::parse(skill_name, &content));
                }
            }
        }
        None
    }

    pub fn select_interactive(active_skills: &[String]) -> Option<SkillAction> {
        let all_skills = Self::list_available();
        if all_skills.is_empty() {
            return None;
        }

        let mut query = String::new();
        let mut sel_idx: usize = 0;
        let max_visible: usize = 8;
        let mut last_rendered_lines: usize = 0;

        let _ = enable_raw_mode();
        let mut stdout = io::stdout();
        print!("\x1b[?25l");

        loop {
            let q_lower = query.to_lowercase();
            let mut filtered: Vec<String> = all_skills
                .iter()
                .filter(|s| s.to_lowercase().contains(&q_lower))
                .cloned()
                .collect();

            let has_detach_all = !active_skills.is_empty() && (q_lower.is_empty() || "detach all clear".contains(&q_lower));
            if has_detach_all {
                filtered.push("__DETACH_ALL__".to_string());
            }

            if sel_idx >= filtered.len() && !filtered.is_empty() {
                sel_idx = filtered.len() - 1;
            }

            if last_rendered_lines > 0 {
                print!("\x1b[{}A\r", last_rendered_lines);
            }

            let visible_count = filtered.len().min(max_visible);
            let start = if sel_idx >= visible_count {
                sel_idx + 1 - visible_count
            } else {
                0
            };
            let end = (start + visible_count).min(filtered.len());

            let active_count = active_skills.len();
            print!("\r\x1b[2K\x1b[1;36m[ai skill]\x1b[0m Manage Skills \x1b[2m({}/3 active):\x1b[0m\r\n", active_count);
            print!("\r\x1b[2K  \x1b[2mSearch:\x1b[0m \x1b[1;37m{}\x1b[0m\x1b[1;36m_\x1b[0m\r\n", query);
            print!("\r\x1b[2K\r\n");

            let item_lines = if filtered.is_empty() {
                print!("\r\x1b[2K    \x1b[2m(no matching skills found)\x1b[0m\r\n");
                1
            } else {
                for (i, item) in filtered[start..end].iter().enumerate() {
                    let actual_idx = start + i;
                    let is_selected = actual_idx == sel_idx;

                    if item == "__DETACH_ALL__" {
                        if is_selected {
                            print!("\r\x1b[2K\x1b[1;31m  ❯ \x1b[1;37m{:2}. \x1b[1;31m✕ Detach all skills ({})\x1b[0m\r\n", actual_idx + 1, active_count);
                        } else {
                            print!("\r\x1b[2K\x1b[2m    {:2}. \x1b[31m✕ Detach all skills ({})\x1b[0m\r\n", actual_idx + 1, active_count);
                        }
                    } else {
                        let is_active = active_skills.contains(item);
                        let badge = if is_active {
                            " \x1b[32m[active - enter to remove]\x1b[0m"
                        } else {
                            ""
                        };

                        if is_selected {
                            print!("\r\x1b[2K\x1b[1;36m  ❯ \x1b[1;37m{:2}. \x1b[1;36m{:<20}\x1b[0m{}\r\n", actual_idx + 1, item, badge);
                        } else {
                            print!("\r\x1b[2K\x1b[2m    {:2}. {:<20}\x1b[0m{}\r\n", actual_idx + 1, item, badge);
                        }
                    }
                }
                visible_count
            };

            print!("\r\x1b[2K\r\n");
            print!("\r\x1b[2K  \x1b[2m:: Enter: toggle/select | ↑/↓: navigate | Type: filter | Esc: cancel\x1b[0m");
            let _ = stdout.flush();

            last_rendered_lines = item_lines + 4;

            if let Ok(Event::Key(key_event)) = event::read() {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                match key_event.code {
                    KeyCode::Up => {
                        if !filtered.is_empty() {
                            sel_idx = (sel_idx + filtered.len() - 1) % filtered.len();
                        }
                    }
                    KeyCode::Down => {
                        if !filtered.is_empty() {
                            sel_idx = (sel_idx + 1) % filtered.len();
                        }
                    }
                    KeyCode::Enter => {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", last_rendered_lines);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        if !filtered.is_empty() {
                            let chosen = &filtered[sel_idx];
                            if chosen == "__DETACH_ALL__" {
                                return Some(SkillAction::DetachAll);
                            } else if active_skills.contains(chosen) {
                                return Some(SkillAction::Detach(chosen.clone()));
                            } else {
                                return Some(SkillAction::Attach(chosen.clone()));
                            }
                        }
                        return None;
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        sel_idx = 0;
                    }
                    KeyCode::Char(c) => {
                        if c == 'c' && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                            print!("\x1b[{}A\r\x1b[J\x1b[?25h", last_rendered_lines);
                            let _ = stdout.flush();
                            let _ = disable_raw_mode();
                            return None;
                        }
                        query.push(c);
                        sel_idx = 0;
                    }
                    KeyCode::Esc => {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", last_rendered_lines);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        return None;
                    }
                    _ => {}
                }
            }
        }
    }
}
