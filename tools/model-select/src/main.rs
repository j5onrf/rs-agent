use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, Show);
        let _ = stdout.flush();
    }
}

struct CustomSlot {
    index: usize,
    name: String,
    model: String,
    key_line_idx: Option<usize>,
    is_active: bool,
    saved_key: String,
}

fn get_icon_for_slot(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("gemini") {
        "✨"
    } else if lower.contains("openrouter") {
        "🌐"
    } else if lower.contains("tokenharb") {
        "⚓"
    } else if lower.contains("claude") || lower.contains("anthropic") {
        "✳️"
    } else if lower.contains("huggingface") || lower.contains("spaces") || lower.contains("hf") {
        "🤗"
    } else if lower.contains("grok") || lower.contains("xai") || lower.contains("deepseek") {
        "🪐"
    } else if lower.contains("openai") || lower.contains("gpt") {
        "🤖"
    } else if lower.contains("ollama") {
        "🦙"
    } else {
        "🪐"
    }
}

fn get_env_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config/rs-agent/.env")
    } else {
        dirs::home_dir()
            .map(|h| h.join(".config/rs-agent/.env"))
            .unwrap_or_else(|| PathBuf::from(".env"))
    }
}

fn extract_quoted_val(line: &str) -> &str {
    if let Some(pos) = line.find('=') {
        let rest = line[pos + 1..].trim();
        let clean = rest.split('#').next().unwrap_or(rest).trim();
        return clean.trim_matches(|c| c == '"' || c == '\'');
    }
    ""
}

fn main() -> std::io::Result<()> {
    let env_path = get_env_path();
    let raw_env = fs::read_to_string(&env_path).unwrap_or_default();
    let mut lines: Vec<String> = raw_env.lines().map(|s| s.to_string()).collect();

    let mut custom_slots: Vec<CustomSlot> = Vec::new();

    for i in 1..=9 {
        let key_prefix = format!("CUSTOM{}_API_KEY", i);
        let name_prefix = format!("CUSTOM{}_NAME", i);
        let model_prefix = format!("CUSTOM{}_MODEL", i);

        let slot_present = lines.iter().any(|l| {
            let u = l.trim().trim_start_matches('#').trim();
            u.starts_with(&name_prefix) || u.starts_with(&key_prefix) || u.starts_with(&model_prefix)
        });

        if !slot_present {
            continue;
        }

        let mut slot_name = format!("Custom {}", i);
        let mut slot_model = "default".to_string();
        let mut key_idx = None;
        let mut is_active = false;
        let mut saved_key = String::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let uncommented = trimmed.trim_start_matches('#').trim();

            if uncommented.starts_with(&name_prefix) {
                let val = extract_quoted_val(uncommented);
                if !val.is_empty() {
                    slot_name = val.to_string();
                }
            } else if uncommented.starts_with(&model_prefix) {
                let val = extract_quoted_val(uncommented);
                if !val.is_empty() {
                    slot_model = val.to_string();
                }
            } else if uncommented.starts_with(&key_prefix) || (i == 1 && uncommented.starts_with("CUSTOM_API_KEY")) {
                key_idx = Some(idx);
                let val = extract_quoted_val(uncommented);
                if !val.is_empty() && !val.contains("your_key_here") {
                    saved_key = val.to_string();
                }
                is_active = !trimmed.starts_with('#');
            }
        }

        custom_slots.push(CustomSlot {
            index: i,
            name: slot_name,
            model: slot_model,
            key_line_idx: key_idx,
            is_active,
            saved_key,
        });
    }

    let mut max_tokens = 8192;
    let mut tokens_line_idx = None;

    let mut max_rounds = 10;
    let mut rounds_line_idx = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let uncommented = trimmed.trim_start_matches('#').trim();

        if uncommented.starts_with("AI_MAX_TOKENS") {
            tokens_line_idx = Some(idx);
            let val = extract_quoted_val(uncommented);
            if let Ok(v) = val.parse::<usize>() {
                max_tokens = v;
            }
        } else if uncommented.starts_with("AI_MAX_AGENT_ROUNDS") {
            rounds_line_idx = Some(idx);
            let val = extract_quoted_val(uncommented);
            if let Ok(v) = val.parse::<usize>() {
                max_rounds = v;
            }
        }
    }

    enable_raw_mode()?;
    let _guard = RawModeGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, Hide);

    let num_slots = custom_slots.len();
    let tok_idx = num_slots;
    let rnd_idx = num_slots + 1;
    let exit_idx = num_slots + 2;
    let total_options = num_slots + 3;
    let mut cursor = 0;

    loop {
        let mut buf = String::with_capacity(2048);
        buf.push_str("\x1b[2J\x1b[H\r\n   \x1b[1;37m  Model Select\x1b[0m\r\n");
        buf.push_str("   \x1b[90m────────────────────────────────────────────────────────────\x1b[0m\r\n\r\n");

        for (idx, slot) in custom_slots.iter().enumerate() {
            let pointer = if cursor == idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
            let icon = get_icon_for_slot(&slot.name);
            let status = if slot.is_active {
                format!("\x1b[1;33m{}\x1b[0m", slot.model)
            } else {
                "\x1b[90mDISABLED\x1b[0m".to_string()
            };
            buf.push_str(&format!("   {} {}  \x1b[1;37m{:<28}\x1b[0m {}\r\n", pointer, icon, slot.name, status));
        }

        // Whitespace separation
        buf.push_str("\r\n");

        // Token Budget
        let tok_ptr = if cursor == tok_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
        buf.push_str(&format!("   {} 🧠  \x1b[1;37m{:<28}\x1b[0m \x1b[1;33m{} tokens\x1b[0m\r\n", tok_ptr, "Context Budget", max_tokens));

        // Agent Rounds Limit
        let rnd_ptr = if cursor == rnd_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
        buf.push_str(&format!("   {} 🔄  \x1b[1;37m{:<28}\x1b[0m \x1b[1;33m{} steps\x1b[0m\r\n\r\n", rnd_ptr, "Task Loop Limit", max_rounds));

        // Save & Exit
        let exit_ptr = if cursor == exit_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
        buf.push_str(&format!("   {} ✕  \x1b[1;32mSave & Exit\x1b[0m\r\n", exit_ptr));
        buf.push_str("   \x1b[90m────────────────────────────────────────────────────────────\x1b[0m\r\n");
        buf.push_str("   \x1b[90m▲/▼: Navigate | Space: Toggle | Enter: Select | Q: Quit\x1b[0m\r\n");

        print!("{}", buf);
        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = if cursor > 0 { cursor - 1 } else { total_options - 1 };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    cursor = if cursor + 1 < total_options { cursor + 1 } else { 0 };
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if cursor < num_slots {
                        let will_activate = !custom_slots[cursor].is_active;
                        for slot in &mut custom_slots {
                            slot.is_active = false;
                        }
                        custom_slots[cursor].is_active = will_activate;
                    } else if cursor == tok_idx {
                        max_tokens = match max_tokens {
                            4096 => 8192,
                            8192 => 16384,
                            16384 => 32768,
                            32768 => 65536,
                            _ => 4096,
                        };
                    } else if cursor == rnd_idx {
                        max_rounds = match max_rounds {
                            5 => 10,
                            10 => 15,
                            15 => 20,
                            20 => 25,
                            25 => 30,
                            _ => 5,
                        };
                    } else {
                        break;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    break;
                }
                _ => {}
            }
        }
    }

    for slot in &custom_slots {
        if let Some(l_idx) = slot.key_line_idx {
            let key_val = if slot.saved_key.is_empty() { "sk-your-key-here" } else { &slot.saved_key };
            let prefix = format!("CUSTOM{}_API_KEY", slot.index);
            if slot.is_active {
                lines[l_idx] = format!("{}=\"{}\"", prefix, key_val);
            } else {
                lines[l_idx] = format!("# {}=\"{}\"", prefix, key_val);
            }
        }
    }

    if let Some(l_idx) = tokens_line_idx {
        lines[l_idx] = format!("AI_MAX_TOKENS=\"{}\"", max_tokens);
    }

    if let Some(l_idx) = rounds_line_idx {
        lines[l_idx] = format!("AI_MAX_AGENT_ROUNDS=\"{}\"", max_rounds);
    }

    let mut out_content = lines.join("\n");
    if !out_content.ends_with('\n') {
        out_content.push('\n');
    }
    fs::write(&env_path, out_content)?;

    let _ = crossterm::execute!(stdout, Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0));
    if let Some(active) = custom_slots.iter().find(|s| s.is_active) {
        let icon = get_icon_for_slot(&active.name);
        println!("\r\n   \x1b[1;32m✓\x1b[0m {}  \x1b[1;37m{}\x1b[0m enabled ({})\r\n", icon, active.name, active.model);
    } else {
        println!("\r\n   \x1b[1;33m✓\x1b[0m All cloud providers disabled. Using local server on :8080.\r\n");
    }

    Ok(())
}
