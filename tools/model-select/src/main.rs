use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_ENV: &str = r#"# Rs-Agent Provider Cascade (.env) — Priority: First active key | Switch with: ms
# Fallback: Local llama-server (:8080) when all cloud keys are commented with '#'.

# ── 1. Google Gemini ──────────────────────────────────────────────────────────
#GEMINI_API_KEY="AQ.your-key-here"
GEMINI_MODEL="gemini-3.5-flash-lite"

# ── 2. OpenRouter ─────────────────────────────────────────────────────────────
#OPENROUTER_API_KEY="sk-or-v1-your-key-here"
OPENROUTER_MODEL="openrouter/free"

# ── 3. TokenHarbor ────────────────────────────────────────────────────────────
#TOKENHARBOR_API_KEY="thk_live_your-key-here"
TOKENHARBOR_MODEL="deepseek-v4.1-flash:free"

# ── More Direct Providers e.g. ────────────────────────────────────────────────
# OPENAI_API_KEY="sk-your-key-here"
# ANTHROPIC_API_KEY="sk-ant-your-key-here"

# ── 4. Custom Direct Endpoint ─────────────────────────────────────────────────
# Add: Define CUSTOM1_*, CUSTOM2_*, up to CUSTOM9_*
# CUSTOM1_NAME="Custom Provider"
# CUSTOM1_API_KEY="sk-your-key-here"
CUSTOM1_URL="https://api.deepseek.com/chat/completions"
CUSTOM1_MODEL="deepseek-flash"

# ── MCP Server Keys (Optional) ────────────────────────────────────────────────
# API keys for tools configured in plugins/mcp/servers.json
# FIRECRAWL_API_KEY="fc-your-actual-api-key"

# ── Runtime Governors ─────────────────────────────────────────────────────────
AI_MAX_TOKENS="8192"
AI_MAX_AGENT_ROUNDS="10"
"#;

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, Show);
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

fn ensure_env_file(env_path: &Path) {
    if !env_path.exists() {
        if let Some(parent) = env_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(env_path, DEFAULT_ENV);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(env_path, fs::Permissions::from_mode(0o600));
        }
    }
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
            let clean = rest.split('#').next().unwrap_or(rest).trim();
            return clean;
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
        ("TOKENHARBOR", "TokenHarbor", "deepseek-v4.1-flash:free"),
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
            for m_line in lines {
                let mu = m_line.trim().trim_start_matches('#').trim();
                if mu.starts_with(&model_var) {
                    let val = extract_quoted_val(mu);
                    if !val.is_empty() {
                        slot_model = val.to_string();
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
            } else if uncommented.starts_with(&model_var) {
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
            if let Ok(v) = val.parse::<usize>() {
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
) -> std::io::Result<()> {
    for slot in slots.iter_mut() {
        if let Some(l_idx) = slot.key_line_idx {
            let raw = strip_leading_comment(&lines[l_idx]);
            if slot.is_active {
                lines[l_idx] = raw.to_string();
            } else {
                lines[l_idx] = format!("#{}", raw);
            }
        } else if slot.is_active {
            // Append missing key line dynamically so TUI and disk never diverge
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
            format!("\x1b[1;32m{}\x1b[0m", slot.model)
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
    buf.push_str(&format!(
        "   {} 🔄  \x1b[1;37m{:<28}\x1b[0m \x1b[1;33m{} steps\x1b[0m\r\n\r\n",
        rnd_ptr, "Task Loop Limit", max_rounds
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

fn main() -> std::io::Result<()> {
    let env_path = get_env_path();
    ensure_env_file(&env_path);

    // Fail loudly if .env cannot be read; never proceed with empty defaults
    let raw_env = match fs::read_to_string(&env_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("\x1b[1;31m[error] Failed to read {}: {}\x1b[0m", env_path.display(), e);
            return Err(e);
        }
    };

    let mut lines: Vec<String> = raw_env.lines().map(|s| s.to_string()).collect();

    let mut provider_slots = load_provider_slots(&lines);
    let (mut max_tokens, tokens_line_idx, mut max_rounds, rounds_line_idx) = load_governors(&lines);

    enable_raw_mode()?;
    let _guard = RawModeGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, Hide);

    let num_slots = provider_slots.len();
    let tok_idx = num_slots;
    let rnd_idx = num_slots + 1;
    let total_options = num_slots + 3;
    let mut cursor = 0;
    let mut status_message: Option<String> = None;

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
                            _ => 5,
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
                                status_message = Some(format!(
                                    "\x1b[1;32m✓\x1b[0m Task Loop Limit: \x1b[1;33m{} steps\x1b[0m \x1b[2m[saved]\x1b[0m",
                                    max_rounds
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
    let _ = crossterm::execute!(stdout, Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0));
    if let Some(active) = provider_slots.iter().find(|s| s.is_active) {
        let icon = get_icon_for_slot(&active.name);
        println!("\r\n   \x1b[1;32m✓\x1b[0m {}  \x1b[1;37m{}\x1b[0m active ({})\r\n", icon, active.name, active.model);
    } else {
        println!("\r\n   \x1b[1;33m✓\x1b[0m All cloud providers disabled. Using local server on :8080.\r\n");
    }

    Ok(())
}
