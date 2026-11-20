use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct SelectOutcome {
    pub changed: bool,
    pub summary: String,
}

struct RawModeGuard {
    alt_screen: bool,
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        if self.alt_screen {
            let _ = crossterm::execute!(stdout, Show, LeaveAlternateScreen);
        } else {
            let _ = crossterm::execute!(stdout, Show);
        }
        let _ = disable_raw_mode();
        let _ = stdout.flush();
    }
}

struct ProviderSlot {
    name: String,
    model: String,
    key_var_name: String,
    key_line_idx: Option<usize>,
    is_active: bool,
}

fn get_icon_for_slot(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("gemini") || lower.contains("google") {
        "✨"
    } else if lower.contains("openrouter") {
        "🌐"
    } else if lower.contains("tokenharb") {
        "⚓"
    } else if lower.contains("claude") || lower.contains("anthropic") {
        "✳️"
    } else if lower.contains("deepseek") {
        "🪐"
    } else if lower.contains("openai") || lower.contains("gpt") {
        "🤖"
    } else if lower.contains("groq") {
        "⚡"
    } else {
        "🪐"
    }
}

fn get_env_path() -> PathBuf {
    dirs::home_dir().map(|h| h.join(".config/rs-agent/.env")).unwrap_or_else(|| PathBuf::from(".env"))
}

/// Quote-aware value extractor. Preserves '#' within quotes.
fn extract_quoted_val(line: &str) -> &str {
    if let Some(pos) = line.find('=') {
        let rest = line[pos + 1..].trim();
        if let Some(stripped) = rest.strip_prefix('"') {
            if let Some(end) = stripped.find('"') {
                return &stripped[..end];
            }
        } else if let Some(stripped) = rest.strip_prefix('\'') {
            if let Some(end) = stripped.find('\'') {
                return &stripped[..end];
            }
        } else {
            return rest.split('#').next().unwrap_or(rest).trim();
        }
    }
    ""
}

fn strip_leading_comment(line: &str) -> &str {
    line.trim().trim_start_matches('#').trim_start()
}

fn is_dummy_val(val: &str) -> bool {
    let v = val.trim().to_lowercase();
    v.is_empty() || v.contains("your-key-here") || v.contains("your_key_here") || v.contains("your-actual-api-key")
}

fn load_provider_slots(lines: &[String]) -> Vec<ProviderSlot> {
    let mut slots = Vec::new();

    const DIRECT_CANDIDATES: &[(&str, &str, &str)] = &[
        ("GEMINI", "Google Gemini", "gemini-3.5-flash-lite"),
        ("OPENROUTER", "OpenRouter", "openrouter/free"),
        ("OPENAI", "OpenAI", "gpt-4o"),
        ("ANTHROPIC", "Anthropic Claude", "claude-3-5-sonnet"),
    ];

    for &(pfx, def_name, def_model) in DIRECT_CANDIDATES {
        let key_var = format!("{}_API_KEY", pfx);
        let model_var = format!("{}_MODEL", pfx);

        let has_active_model = lines.iter().any(|l| {
            let u = l.trim();
            !u.starts_with('#') && u.starts_with(&model_var)
        });

        let mut key_idx = None;
        let mut is_active = false;
        let mut key_val = "";

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let uncommented = trimmed.trim_start_matches('#').trim();
            if uncommented.starts_with(&key_var) {
                key_idx = Some(idx);
                is_active = !trimmed.starts_with('#');
                key_val = extract_quoted_val(uncommented);
                break;
            }
        }

        let is_configured = has_active_model || is_active || (!is_dummy_val(key_val) && !key_val.is_empty());
        if is_configured {
            let mut slot_model = def_model.to_string();
            let mut found_active = false;
            for m_line in lines {
                let trimmed = m_line.trim();
                if !trimmed.starts_with('#') && trimmed.starts_with(&model_var) {
                    let val = extract_quoted_val(trimmed);
                    if !val.is_empty() {
                        slot_model = val.to_string();
                        found_active = true;
                        break;
                    }
                }
            }
            if !found_active {
                for m_line in lines {
                    let mu = m_line.trim().trim_start_matches('#').trim();
                    if mu.starts_with(&model_var) {
                        let val = extract_quoted_val(mu);
                        if !val.is_empty() {
                            slot_model = val.to_string();
                        }
                    }
                }
            }

            slots.push(ProviderSlot {
                name: def_name.to_string(),
                model: slot_model,
                key_var_name: key_var,
                key_line_idx: key_idx,
                is_active,
            });
        }
    }

    for i in 1..=9 {
        let pfx = format!("CUSTOM{}", i);
        let name_var = format!("{}_NAME", pfx);
        let model_var = format!("{}_MODEL", pfx);
        let key_var = format!("{}_API_KEY", pfx);
        let url_var = format!("{}_URL", pfx);

        let is_configured = lines.iter().any(|l| {
            let u = l.trim();
            !u.starts_with('#') && (u.starts_with(&key_var) || u.starts_with(&url_var) || u.starts_with(&model_var))
        });

        if !is_configured {
            continue;
        }

        let mut slot_name = format!("Custom {}", i);
        let mut slot_model = "default".to_string();
        let mut key_idx = None;
        let mut is_active = false;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let uncommented = trimmed.trim_start_matches('#').trim();

            if uncommented.starts_with(&name_var) {
                let val = extract_quoted_val(uncommented);
                if !val.is_empty() {
                    slot_name = val.to_string();
                }
            } else if !trimmed.starts_with('#') && trimmed.starts_with(&model_var) {
                let val = extract_quoted_val(trimmed);
                if !val.is_empty() {
                    slot_model = val.to_string();
                }
            } else if slot_model == "default" && uncommented.starts_with(&model_var) {
                let val = extract_quoted_val(uncommented);
                if !val.is_empty() {
                    slot_model = val.to_string();
                }
            } else if uncommented.starts_with(&key_var) || (i == 1 && uncommented.starts_with("CUSTOM_API_KEY")) {
                key_idx = Some(idx);
                is_active = !trimmed.starts_with('#');
            }
        }

        slots.push(ProviderSlot {
            name: slot_name,
            model: slot_model,
            key_var_name: key_var,
            key_line_idx: key_idx,
            is_active,
        });
    }

    slots
}

fn load_governors(lines: &[String]) -> (usize, Option<usize>, usize, Option<usize>) {
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
            if val == "0" || val.eq_ignore_ascii_case("unlimited") {
                max_rounds = 0;
            } else if let Ok(v) = val.parse::<usize>() {
                max_rounds = v;
            }
        }
    }

    (max_tokens, tokens_line_idx, max_rounds, rounds_line_idx)
}

fn save_to_env(
    env_path: &Path,
    lines: &mut Vec<String>,
    slots: &mut [ProviderSlot],
    max_tokens: usize,
    tokens_line_idx: Option<usize>,
    max_rounds: usize,
    rounds_line_idx: Option<usize>,
) -> io::Result<()> {
    for slot in slots.iter_mut() {
        if let Some(l_idx) = slot.key_line_idx {
            let raw = strip_leading_comment(&lines[l_idx]);
            if slot.is_active {
                lines[l_idx] = raw.to_string();
            } else {
                lines[l_idx] = format!("#{}", raw);
            }
        } else if slot.is_active {
            let new_line = format!("{}=\"your-key-here\"", slot.key_var_name);
            lines.push(new_line);
            slot.key_line_idx = Some(lines.len() - 1);
        }
    }

    if let Some(l_idx) = tokens_line_idx {
        lines[l_idx] = format!("AI_MAX_TOKENS=\"{}\"", max_tokens);
    } else {
        lines.push(format!("AI_MAX_TOKENS=\"{}\"", max_tokens));
    }

    if let Some(l_idx) = rounds_line_idx {
        lines[l_idx] = format!("AI_MAX_AGENT_ROUNDS=\"{}\"", max_rounds);
    } else {
        lines.push(format!("AI_MAX_AGENT_ROUNDS=\"{}\"", max_rounds));
    }

    let mut out_content = lines.join("\n");
    if !out_content.ends_with('\n') {
        out_content.push('\n');
    }

    let parent = env_path.parent().unwrap_or(Path::new("."));
    let tmp_path = parent.join(format!(".env.tmp.{}", std::process::id()));
    let _ = fs::remove_file(&tmp_path);

    fs::write(&tmp_path, &out_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp_path, env_path)?;
    Ok(())
}

fn format_slot_model(raw: &str) -> String {
    let mut parts = raw.split(',').map(str::trim).filter(|s| !s.is_empty());
    let first = parts.next().unwrap_or(raw);
    let extra = parts.count();
    if extra > 0 {
        format!("{} (+{})", first, extra)
    } else {
        first.to_string()
    }
}

fn render_tui(
    slots: &[ProviderSlot],
    max_tokens: usize,
    max_rounds: usize,
    cursor: usize,
    status_message: Option<&str>,
) -> String {
    let mut buf = String::with_capacity(2048);
    buf.push_str("\x1b[2J\x1b[H\r\n   \x1b[1;37m  Model Select\x1b[0m\r\n");
    buf.push_str("   \x1b[90m────────────────────────────────────────────────────────────\x1b[0m\r\n\r\n");

    for (idx, slot) in slots.iter().enumerate() {
        let pointer = if cursor == idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
        let icon = get_icon_for_slot(&slot.name);
        let status = if slot.is_active {
            format!("\x1b[1;32m{}\x1b[0m", format_slot_model(&slot.model))
        } else {
            "\x1b[90mDISABLED\x1b[0m".to_string()
        };
        buf.push_str(&format!("   {} {}  \x1b[1;37m{:<28}\x1b[0m {}\r\n", pointer, icon, slot.name, status));
    }

    buf.push_str("\r\n");

    let tok_idx = slots.len();
    let rnd_idx = slots.len() + 1;
    let exit_idx = slots.len() + 2;

    let tok_ptr = if cursor == tok_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
    buf.push_str(&format!(
        "   {} 🧠  \x1b[1;37m{:<28}\x1b[0m \x1b[1;33m{} tokens\x1b[0m\r\n",
        tok_ptr, "Context Budget", max_tokens
    ));

    let rnd_ptr = if cursor == rnd_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
    let rnd_str = if max_rounds == 0 { "unlimited".to_string() } else { format!("{} steps", max_rounds) };
    buf.push_str(&format!(
        "   {} 🔄  \x1b[1;37m{:<28}\x1b[0m \x1b[1;33m{}\x1b[0m\r\n\r\n",
        rnd_ptr, "Task Loop Limit", rnd_str
    ));

    let exit_ptr = if cursor == exit_idx { "\x1b[1;36m❯\x1b[0m " } else { "  " };
    buf.push_str(&format!("   {} ✕  \x1b[1;32mSave & Exit\x1b[0m\r\n", exit_ptr));
    buf.push_str("   \x1b[90m────────────────────────────────────────────────────────────\x1b[0m\r\n");

    if let Some(msg) = status_message {
        buf.push_str(&format!("   {}\r\n", msg));
    } else {
        buf.push_str("   \x1b[90m▲/▼: Navigate | Enter/Space: Toggle | Q/Esc: Exit\x1b[0m\r\n");
    }

    buf
}

pub fn run_selector(alt_screen: bool) -> io::Result<SelectOutcome> {
    let env_path = get_env_path();
    let raw_env = fs::read_to_string(&env_path)
        .map_err(|e| io::Error::new(e.kind(), format!("Failed to read {}: {}", env_path.display(), e)))?;

    let mut lines: Vec<String> = raw_env.lines().map(|s| s.to_string()).collect();
    let mut provider_slots = load_provider_slots(&lines);
    let (mut max_tokens, tokens_line_idx, mut max_rounds, rounds_line_idx) = load_governors(&lines);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let _guard = RawModeGuard { alt_screen };
    if alt_screen {
        let _ = crossterm::execute!(stdout, EnterAlternateScreen, Hide);
    } else {
        let _ = crossterm::execute!(stdout, Hide);
    }

    let num_slots = provider_slots.len();
    let tok_idx = num_slots;
    let rnd_idx = num_slots + 1;
    let total_options = num_slots + 3;
    let mut cursor = 0;
    let mut status_message: Option<String> = None;
    let mut changed = false;

    loop {
        let frame = render_tui(&provider_slots, max_tokens, max_rounds, cursor, status_message.as_deref());
        print!("{}", frame);
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
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if cursor < num_slots {
                        let will_activate = !provider_slots[cursor].is_active;

                        for slot in &mut provider_slots {
                            slot.is_active = false;
                        }
                        provider_slots[cursor].is_active = will_activate;

                        let save_res = save_to_env(
                            &env_path,
                            &mut lines,
                            &mut provider_slots,
                            max_tokens,
                            tokens_line_idx,
                            max_rounds,
                            rounds_line_idx,
                        );

                        let icon = get_icon_for_slot(&provider_slots[cursor].name);
                        match save_res {
                            Ok(()) => {
                                changed = true;
                                if will_activate {
                                    status_message = Some(format!(
                                        "\x1b[1;32m✓\x1b[0m {} \x1b[1;37m{}\x1b[0m enabled ({}) \x1b[2m[saved]\x1b[0m",
                                        icon, provider_slots[cursor].name, provider_slots[cursor].model
                                    ));
                                } else {
                                    status_message = Some(format!(
                                        "\x1b[1;33m✓\x1b[0m {} \x1b[1;37m{}\x1b[0m disabled. Local :8080 active \x1b[2m[saved]\x1b[0m",
                                        icon, provider_slots[cursor].name
                                    ));
                                }
                            }
                            Err(e) => {
                                status_message =
                                    Some(format!("\x1b[1;31m✗\x1b[0m Save failed: {} \x1b[2m[not saved]\x1b[0m", e));
                            }
                        }
                    } else if cursor == tok_idx {
                        max_tokens = match max_tokens {
                            4096 => 8192,
                            8192 => 16384,
                            16384 => 32768,
                            32768 => 65536,
                            65536 => 131072,
                            131072 => 1048576,
                            _ => 4096,
                        };
                        let save_res = save_to_env(
                            &env_path,
                            &mut lines,
                            &mut provider_slots,
                            max_tokens,
                            tokens_line_idx,
                            max_rounds,
                            rounds_line_idx,
                        );
                        match save_res {
                            Ok(()) => {
                                changed = true;
                                status_message = Some(format!(
                                    "\x1b[1;32m✓\x1b[0m Context Budget: \x1b[1;33m{} tokens\x1b[0m \x1b[2m[saved]\x1b[0m",
                                    max_tokens
                                ));
                            }
                            Err(e) => {
                                status_message =
                                    Some(format!("\x1b[1;31m✗\x1b[0m Save failed: {} \x1b[2m[not saved]\x1b[0m", e));
                            }
                        }
                    } else if cursor == rnd_idx {
                        max_rounds = match max_rounds {
                            5 => 10,
                            10 => 15,
                            15 => 20,
                            20 => 25,
                            25 => 30,
                            30 => 0,
                            0 => 5,
                            _ => 15,
                        };
                        let save_res = save_to_env(
                            &env_path,
                            &mut lines,
                            &mut provider_slots,
                            max_tokens,
                            tokens_line_idx,
                            max_rounds,
                            rounds_line_idx,
                        );
                        match save_res {
                            Ok(()) => {
                                changed = true;
                                let rnd_saved = if max_rounds == 0 { "unlimited".to_string() } else { format!("{} steps", max_rounds) };
                                status_message = Some(format!(
                                    "\x1b[1;32m✓\x1b[0m Task Loop Limit: \x1b[1;33m{}\x1b[0m \x1b[2m[saved]\x1b[0m",
                                    rnd_saved
                                ));
                            }
                            Err(e) => {
                                status_message =
                                    Some(format!("\x1b[1;31m✗\x1b[0m Save failed: {} \x1b[2m[not saved]\x1b[0m", e));
                            }
                        }
                    } else {
                        break;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    break;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break;
                }
                _ => {}
            }
        }
    }

    drop(_guard);
    if !alt_screen {
        let _ = crossterm::execute!(stdout, Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0));
    }

    let summary = if let Some(active) = provider_slots.iter().find(|s| s.is_active) {
        let icon = get_icon_for_slot(&active.name);
        format!("\x1b[1;32m✓\x1b[0m {}  \x1b[1;37m{}\x1b[0m active ({})", icon, active.name, active.model)
    } else {
        "\x1b[1;33m✓\x1b[0m All cloud providers disabled. Using local server on :8080.".to_string()
    };

    Ok(SelectOutcome { changed, summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_roundtrip_toggles_provider_and_governors() {
        let tmp = std::env::temp_dir().join(format!("rs_agent_ms_test_{}.env", std::process::id()));
        let mut lines: Vec<String> = [
            "#GEMINI_API_KEY=\"AQ.test-key\"",
            "GEMINI_MODEL=\"gemini-4-hydrogen\"",
            "OPENROUTER_API_KEY=\"sk-or-test\"",
            "OPENROUTER_MODEL=\"openrouter/free\"",
            "AI_MAX_TOKENS=\"8192\"",
            "AI_MAX_AGENT_ROUNDS=\"10\"",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let mut slots = load_provider_slots(&lines);
        let (_, tok_idx, _, rnd_idx) = load_governors(&lines);
        assert_eq!(slots.len(), 2);
        assert!(!slots[0].is_active && slots[1].is_active);

        slots[0].is_active = true;
        slots[1].is_active = false;
        save_to_env(&tmp, &mut lines, &mut slots, 65536, tok_idx, 20, rnd_idx).unwrap();

        let saved: Vec<String> = fs::read_to_string(&tmp).unwrap().lines().map(String::from).collect();
        let reloaded = load_provider_slots(&saved);
        let (tok, _, rnd, _) = load_governors(&saved);
        let _ = fs::remove_file(&tmp);

        assert!(reloaded[0].is_active && !reloaded[1].is_active);
        assert_eq!(tok, 65536);
        assert_eq!(rnd, 20);
    }

    #[test]
    fn test_load_provider_slots_prioritizes_active_model_over_commented_presets() {
        let lines: Vec<String> = vec![
            "#OPENROUTER_API_KEY=\"sk-test\"\n".into(),
            "OPENROUTER_MODEL=\"nvidia/nemotron-3-ultra-550b-a55b:free, nvidia/nemotron-3.5-lightning:free\"".into(),
            "# OPENROUTER_MODEL=\"openrouter/free\"".into(),
        ];
        let slots = load_provider_slots(&lines);
        let or_slot = slots.iter().find(|s| s.name == "OpenRouter").expect("OpenRouter slot missing");
        assert_eq!(
            or_slot.model,
            "nvidia/nemotron-3-ultra-550b-a55b:free, nvidia/nemotron-3.5-lightning:free"
        );
        assert_eq!(
            format_slot_model(&or_slot.model),
            "nvidia/nemotron-3-ultra-550b-a55b:free (+1)"
        );
    }
}
