use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

const AMBER: &str = "\x1b[38;2;230;120;60m";
const GREEN: &str = "\x1b[1;32m";
const RED: &str = "\x1b[1;31m";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[38;2;140;140;140m";

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        print!("\x1b[?25h");
        let _ = io::stdout().flush();
    }
}

fn env_path() -> PathBuf {
    dirs::home_dir().unwrap().join(".config/rs-agent/.env")
}

fn cache_path() -> PathBuf {
    dirs::home_dir().unwrap().join(".config/rs-agent/var/model_cache.json")
}

struct EnvMap {
    lines: Vec<String>,
}

impl EnvMap {
    fn load() -> Self {
        let path = env_path();
        let lines = if path.exists() {
            fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        };
        Self { lines }
    }

    fn save(&self) {
        let path = env_path();
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).ok();
        }
        fs::write(&path, self.lines.join("\n") + "\n").ok();
    }

    fn get(&self, key: &str) -> String {
        for line in &self.lines {
            let trimmed = line.trim();
            let check = if trimmed.starts_with('#') {
                trimmed.trim_start_matches('#').trim()
            } else {
                trimmed
            };
            if check.starts_with(&format!("{}=", key)) {
                let parts: Vec<&str> = check.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let mut val = parts[1].trim();
                    if let Some(idx) = val.find(" #") {
                        val = val[..idx].trim();
                    }
                    return val.trim_matches('"').trim_matches('\'').to_string();
                }
            }
        }
        String::new()
    }

    fn set(&mut self, key: &str, val: &str) {
        let mut found = false;
        let clean_val = val.replace('\r', "").replace('\n', "").replace('"', "\\\"");
        for line in self.lines.iter_mut() {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("{}=", key)) || trimmed.starts_with(&format!("#{}=", key)) {
                let prefix = if line.starts_with('#') { "#" } else { "" };
                *line = format!("{}{}={}", prefix, key, clean_val);
                found = true;
                break;
            }
        }
        if !found {
            self.lines.push(format!("{}={}", key, clean_val));
        }
    }

    fn is_active(&self, key: &str) -> bool {
        for line in &self.lines {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') && trimmed.starts_with(&format!("{}=", key)) {
                let val = self.get(key);
                if val == "not-needed" || (val.len() >= 8 && !val.contains("your-key-here")) {
                    return true;
                }
            }
        }
        false
    }

    fn disable(&mut self, key: &str) {
        for line in self.lines.iter_mut() {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') && trimmed.starts_with(&format!("{}=", key)) {
                *line = format!("#{}", line);
            }
        }
    }

    fn enable(&mut self, key: &str) {
        let mut found = false;
        for line in self.lines.iter_mut() {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("#{}=", key)) {
                *line = line.trim_start_matches('#').to_string();
                found = true;
                break;
            }
        }
        if !found {
            self.lines.push(format!("{}=\"not-needed\"", key));
        }
    }

    fn isolate_provider(&mut self, active_key: &str) {
        let provider_keys = [
            "CUSTOM_API_KEY",
            "CUSTOM2_API_KEY",
            "CUSTOM3_API_KEY",
            "GEMINI_API_KEY",
            "OPENROUTER_API_KEY",
        ];
        for pk in provider_keys {
            if pk == active_key {
                self.enable(pk);
            } else {
                self.disable(pk);
            }
        }
    }
}

fn visible_width(s: &str) -> usize {
    s.chars().map(|c| {
        let u = c as u32;
        if (0x2300..=0x23FF).contains(&u)
            || (0x2600..=0x27BF).contains(&u)
            || (0x1F300..=0x1FAFF).contains(&u)
        {
            2
        } else {
            1
        }
    }).sum()
}

fn prompt_user(prompt: &str) -> Option<String> {
    let _ = disable_raw_mode();
    print!("\r\n\x1b[1;36m{}:\x1b[0m ", prompt);
    let _ = io::stdout().flush();
    let mut buf = String::new();
    if io::stdin().read_line(&mut buf).is_ok() {
        let trimmed = buf.trim().to_string();
        let _ = enable_raw_mode();
        print!("\x1b[?25l");
        let _ = io::stdout().flush();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    } else {
        let _ = enable_raw_mode();
        print!("\x1b[?25l");
        let _ = io::stdout().flush();
        None
    }
}

fn fetch_openrouter_free() -> Vec<String> {
    let mut free = vec!["openrouter/free".to_string()];
    if let Ok(out) = Command::new("curl")
        .args(["-s", "https://openrouter.ai/api/v1/models"])
        .output()
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        if id.contains(":free") && !free.contains(&id.to_string()) {
                            free.push(id.to_string());
                        }
                    }
                }
            }
        }
    }
    free
}

fn run_submenu(
    title: &str,
    items: &[String],
    current: &str,
    active: bool,
    extras: &[String],
) -> Option<String> {
    let mut query = String::new();
    let mut sel = 0;

    loop {
        let mut opts = extras.to_vec();
        let filtered: Vec<String> = items
            .iter()
            .filter(|i| query.is_empty() || i.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect();
        opts.extend(filtered);

        if sel >= opts.len() && !opts.is_empty() {
            sel = opts.len() - 1;
        }

        print!(
            "\x1b[H\x1b[2J\r\n   {}  SELECT {}:{}\r\n   {}────────────────────────────────────────────────────────────{}\r\n\r\n",
            BOLD,
            title.to_uppercase(),
            RESET,
            DIM,
            RESET
        );
        if !query.is_empty() {
            print!("   🔍  Filter: {}{}{}_\r\n\r\n", GREEN, query, AMBER);
        }

        let start = sel.saturating_sub(7);
        let end = (start + 14).min(opts.len());

        for i in start..end {
            let opt = &opts[i];
            let is_cur = opt == current;
            let bullet = if i == sel { format!("{}❯{}", AMBER, RESET) } else { " ".to_string() };

            let display = if i == 0 && !active && opt.contains("Turn Off") {
                format!(" {}{} (disabled){}", RED, opt, RESET)
            } else if is_cur && active {
                format!(" {}{} (active){}", GREEN, opt, RESET)
            } else {
                format!(" {}", opt)
            };

            let b_style = if i == sel { BOLD } else { "" };
            print!("     {}{} {}\r\n", b_style, bullet, display);
        }

        print!(
            "\r\n   {}────────────────────────────────────────────────────────────{}\r\n",
            DIM, RESET
        );
        print!(
            "   {}Enter: select | ↑/↓: navigate | Type: filter | Esc: cancel{}\r\n",
            DIM, RESET
        );
        let _ = io::stdout().flush();

        if let Ok(Event::Key(k)) = event::read() {
            if k.kind != KeyEventKind::Press { continue; }
            match k.code {
                KeyCode::Esc => return None,
                KeyCode::Up => sel = sel.saturating_sub(1),
                KeyCode::Down => sel = (sel + 1).min(opts.len().saturating_sub(1)),
                KeyCode::Enter => {
                    if !opts.is_empty() { return Some(opts[sel].clone()); }
                }
                KeyCode::Backspace => { query.pop(); sel = 0; }
                KeyCode::Char('c') if k.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => return None,
                KeyCode::Char(c) => { query.push(c); sel = 0; }
                _ => {}
            }
        }
    }
}

fn main() {
    let _guard = RawModeGuard;
    let _ = enable_raw_mode();
    print!("\x1b[?25l");
    let _ = io::stdout().flush();

    let mut env = EnvMap::load();
    let mut menu_idx = 0;
    let mut msg = String::new();

    loop {
        let is_custom1 = env.is_active("CUSTOM_API_KEY");
        let is_custom2 = env.is_active("CUSTOM2_API_KEY");
        let is_or = env.is_active("OPENROUTER_API_KEY");
        let is_gem = env.is_active("GEMINI_API_KEY");
        let is_gnd = env.is_active("GND_KEY");

        let c1_mod = env.get("CUSTOM_MODEL");
        let c2_mod = env.get("CUSTOM2_MODEL");
        let or_mod = env.get("OPENROUTER_MODEL");
        let gnd_mod = env.get("GND_MODEL");
        let ctx_mod = env.get("AI_MAX_TOKENS");
        let turns_mod = env.get("AI_MAX_AGENT_ROUNDS");

        let ctx_display = if ctx_mod.is_empty() { "8192 tokens".to_string() } else { format!("{} tokens", ctx_mod) };
        let turns_display = if turns_mod.is_empty() { "10 steps".to_string() } else { format!("{} steps", turns_mod) };

        // (label, is_active, current_value, env_key, is_spaced)
        let items = [
            ("🤗  Custom 1 (Hugging Face / Local)", is_custom1, c1_mod.clone(), "CUSTOM_API_KEY", true),
            ("🪐  Custom 2 (DeepSeek / xAI)", is_custom2, c2_mod.clone(), "CUSTOM2_API_KEY", true),
            ("🌐  OpenRouter Universal", is_or, or_mod.clone(), "OPENROUTER_API_KEY", true),
            ("✨  Google Gemini", is_gem, env.get("GEMINI_MODEL"), "GEMINI_API_KEY", true),
            ("🔍  Search Grounding (/gnd)", is_gnd, gnd_mod.clone(), "GND_KEY", false),
            ("🧠  Context Budget", true, ctx_display, "AI_MAX_TOKENS", false),
            ("🔄  Task Loop Limit", true, turns_display, "AI_MAX_AGENT_ROUNDS", false),
            ("↺  Refresh Cloud Lists", false, String::new(), "", true),
            ("✕  Save & Exit", false, String::new(), "", false),
        ];

        print!(
            "\x1b[H\x1b[2J\r\n   {}  Model Select{}\r\n   {}────────────────────────────────────────────────────────────{}\r\n\r\n",
            BOLD, RESET, DIM, RESET
        );

        for (i, (label, active, val, _, is_spaced)) in items.iter().enumerate() {
            let cursor = if i == menu_idx {
                format!("   {}❯{}  {}", AMBER, RESET, BOLD)
            } else {
                "      ".to_string()
            };

            let status = if *active && i < 5 {
                format!("{}{}{}", GREEN, val, RESET)
            } else if !*active && i < 5 {
                format!("{}DISABLED{}", RED, RESET)
            } else if i == 5 || i == 6 {
                format!("{}{}{}", GREEN, val, RESET)
            } else {
                String::new()
            };

            let label_width = visible_width(label);
            let pad = if 36 > label_width { 36 - label_width } else { 2 };
            let spacing = " ".repeat(pad);

            let extra_nl = if *is_spaced { "\r\n" } else { "" };
            print!("{}{}{}{}\r\n{}", cursor, label, spacing, status, extra_nl);
        }

        print!(
            "   {}────────────────────────────────────────────────────────────{}\r\n",
            DIM, RESET
        );
        let footer = if msg.is_empty() {
            format!(
                "{}▲/▼: Navigate | Space: Toggle | Enter: Select | Q: Quit{}",
                DIM, RESET
            )
        } else {
            msg.clone()
        };
        print!("   {}\r\n", footer);
        let _ = io::stdout().flush();
        msg.clear();

        if let Ok(Event::Key(k)) = event::read() {
            if k.kind != KeyEventKind::Press { continue; }
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Up => menu_idx = menu_idx.saturating_sub(1),
                KeyCode::Down => menu_idx = (menu_idx + 1).min(items.len() - 1),
                KeyCode::Char(' ') => {
                    let tgt = &items[menu_idx];
                    if tgt.3.contains("API_KEY") {
                        if env.is_active(tgt.3) {
                            env.disable(tgt.3);
                            msg = format!("✓ {} disabled.", tgt.0.trim());
                        } else {
                            env.isolate_provider(tgt.3);
                            msg = format!("✓ {} activated.", tgt.0.trim());
                        }
                        env.save();
                    } else if tgt.3 == "AI_MAX_AGENT_ROUNDS" {
                        let cycle_rounds = ["5", "10", "15", "20", "25", "30", "50"];
                        let cur_r = env.get("AI_MAX_AGENT_ROUNDS");
                        let next_r = cycle_rounds
                            .iter()
                            .position(|&x| x == cur_r)
                            .map(|idx| cycle_rounds[(idx + 1) % cycle_rounds.len()])
                            .unwrap_or("10");
                        env.set("AI_MAX_AGENT_ROUNDS", next_r);
                        env.save();
                        msg = format!("✓ Task Loop Limit: {}", next_r);
                    }
                }
                KeyCode::Enter => {
                    let tgt = &items[menu_idx];
                    if tgt.0.contains("Exit") { break; }

                    if tgt.0.contains("Refresh") {
                        msg = format!("{}↺ Fetching remote models...{}", AMBER, RESET);
                        print!("\x1b[2A\r\x1b[2K   {}\r\n", msg);
                        let _ = io::stdout().flush();
                        let free = fetch_openrouter_free();
                        let mut cache = HashMap::new();
                        cache.insert("free", free);
                        if let Some(p) = cache_path().parent() {
                            let _ = fs::create_dir_all(p);
                        }
                        let _ = fs::write(cache_path(), serde_json::to_string(&cache).unwrap_or_default());
                        msg = "✓ Endpoints synchronized.".to_string();
                        continue;
                    }

                    if tgt.3 == "AI_MAX_TOKENS" {
                        let opts = ["4096", "8192", "16384", "32768", "65536", "131072"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>();
                        let cur_tokens = env.get("AI_MAX_TOKENS");
                        if let Some(res) = run_submenu(
                            "Context Tokens",
                            &opts,
                            &cur_tokens,
                            true,
                            &["✏️  Custom Limit".to_string()],
                        ) {
                            let val = if res.contains("Custom") {
                                prompt_user("Tokens").unwrap_or(cur_tokens)
                            } else {
                                res
                            };
                            env.set("AI_MAX_TOKENS", &val);
                            env.save();
                        }
                        continue;
                    }

                    if tgt.3 == "AI_MAX_AGENT_ROUNDS" {
                        let opts = [
                            "5 (Fast / Strict Tool Limit)",
                            "10 (10 turns - Default)",
                            "15 (15 turns - Extended)",
                            "20 (20 turns - Deep Investigation)",
                            "25 (25 turns - High Autonomy)",
                            "30 (30 turns - Heavy Refactoring)",
                            "50 (50 turns - Autonomous Loop)",
                        ].into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
                        let cur_r = env.get("AI_MAX_AGENT_ROUNDS");
                        if let Some(res) = run_submenu(
                            "Task Loop Limit",
                            &opts,
                            &cur_r,
                            true,
                            &["✏️  Custom Turns Limit".to_string()],
                        ) {
                            let val = if res.contains("Custom") {
                                prompt_user("Steps").unwrap_or(cur_r)
                            } else {
                                res.split_whitespace().next().unwrap_or("10").to_string()
                            };
                            env.set("AI_MAX_AGENT_ROUNDS", &val);
                            env.save();
                        }
                        continue;
                    }

                    if tgt.3.contains("API_KEY") || tgt.3 == "GND_KEY" {
                        let mut opts = vec![];
                        if tgt.0.contains("OpenRouter") {
                            if let Ok(c) = fs::read_to_string(cache_path()) {
                                if let Ok(j) = serde_json::from_str::<serde_json::Value>(&c) {
                                    if let Some(arr) = j.get("free").and_then(|v| v.as_array()) {
                                        opts = arr
                                            .iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect();
                                    }
                                }
                            }
                            if opts.is_empty() {
                                opts.push("openrouter/free".to_string());
                            }
                        } else if tgt.0.contains("Gemini") || tgt.0.contains("Grounding") {
                            opts = vec![
                                "gemini-3.5-flash-lite".into(),
                                "gemini-2.5-flash".into(),
                                "gemini-2.0-flash".into(),
                            ];
                        } else if tgt.0.contains("Custom 1") {
                            opts = vec!["local-model".into(), "space".into()];
                        } else if tgt.0.contains("Custom 2") {
                            opts = vec!["grok-beta".into(), "deepseek-v4.1-flash:free".into()];
                        }

                        let title = format!("{} Models", tgt.0.trim());
                        let extras = [
                            format!("🚫 Turn Off {}", tgt.0.trim()),
                            "✏️  Edit Model Name".into(),
                            "🔗 Edit Endpoint URL".into(),
                            "🔑 Edit API Key".into(),
                        ];

                        if let Some(res) = run_submenu(&title, &opts, &tgt.2, tgt.1, &extras) {
                            if res.contains("Turn Off") {
                                env.disable(tgt.3);
                                msg = format!("✓ {} disabled.", tgt.0.trim());
                            } else if res.contains("Edit Model Name") {
                                if let Some(m) = prompt_user("Model Name") {
                                    let mod_key = tgt.3
                                        .replace("API_KEY", "MODEL")
                                        .replace("GND_KEY", "GND_MODEL");
                                    env.set(&mod_key, &m);
                                    msg = format!("✓ Model set to {}", m);
                                }
                            } else if res.contains("Endpoint URL") {
                                if let Some(u) = prompt_user("Endpoint URL") {
                                    let url_key = tgt.3.replace("API_KEY", "URL");
                                    env.set(&url_key, &u);
                                    msg = format!("✓ URL set to {}", u);
                                }
                            } else if res.contains("API Key") {
                                if let Some(k) = prompt_user("API Key") {
                                    env.set(tgt.3, &k);
                                    env.isolate_provider(tgt.3);
                                    msg = "✓ Key saved & activated.".to_string();
                                }
                            } else {
                                let mod_key = tgt.3
                                    .replace("API_KEY", "MODEL")
                                    .replace("GND_KEY", "GND_MODEL");
                                env.set(&mod_key, &res);
                                if tgt.3.contains("API_KEY") {
                                    env.isolate_provider(tgt.3);
                                }
                                msg = format!("✓ Model set to {}", res);
                            }
                            env.save();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    print!("\x1b[H\x1b[2J");
    let _ = io::stdout().flush();
    println!("\x1b[1;32m✓ Local-AI configuration saved.\x1b[0m");
}
