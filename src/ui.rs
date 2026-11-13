use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::IsTerminal;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

// Exact ANSI constants (inherits active terminal palette)
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BRIGHT: &str = "\x1b[1;36m"; // Primary theme accent
const ANSI_ACCENT: &str = "\x1b[36m"; // Regular theme accent
const ANSI_WHITE: &str = "\x1b[1;37m"; // Bold foreground
const ANSI_MUTED: &str = "\x1b[90m"; // Color 8 / Muted
const ANSI_DIM: &str = "\x1b[2m"; // Faint / Low contrast

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, Show);
        let _ = stdout.flush();
    }
}

#[inline(always)]
fn cell_dither_threshold(x: usize, y: usize) -> f64 {
    let mut n = (x as u64 * 374761393) ^ (y as u64 * 668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) % 1000) as f64 / 1000.0
}

#[inline(always)]
fn slab_shader(x: usize, total_w: usize) -> &'static str {
    let ratio = x as f64 / total_w as f64;
    if ratio < 0.35 {
        ANSI_DIM
    } else if ratio < 0.70 {
        ANSI_MUTED
    } else if ratio < 0.90 {
        ANSI_ACCENT
    } else {
        ANSI_BRIGHT
    }
}

#[inline(always)]
fn model_initials(model: &str) -> (char, char) {
    let mut chars = model.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, ' '),
        _ => ('R', 'A'),
    }
}

#[inline(always)]
fn calculate_reveal(progress: f64) -> f64 {
    if progress < 0.35 {
        0.0
    } else if progress >= 1.0 {
        1.0
    } else {
        ((progress - 0.35) / 0.65).min(1.0)
    }
}

#[inline(always)]
fn push_skeleton_dot(row: &mut String, x: usize, y: usize, t: f64) {
    let band = ((x as f64 * 0.16 - y as f64 * 0.32 - t * 2.8) * 1.3).cos();
    let intensity = if band > 0.0 { band.powi(6) } else { 0.0 };

    let color = if intensity > 0.6 {
        ANSI_WHITE
    } else if intensity > 0.25 {
        ANSI_BRIGHT
    } else if intensity > 0.05 {
        ANSI_ACCENT
    } else {
        ANSI_MUTED
    };

    let ch = if intensity > 0.6 {
        '●'
    } else if intensity > 0.25 {
        '•'
    } else {
        '·'
    };

    row.push_str(color);
    row.push(ch);
    row.push_str(ANSI_RESET);
}

// 0.003ms filesystem ancestor probe avoiding unnecessary child process spawning
fn in_git_repo() -> bool {
    let mut cur = std::env::current_dir().ok();
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return true;
        }
        cur = dir.parent().map(Path::to_path_buf);
    }
    false
}

fn resolve_git_status() -> String {
    if !in_git_repo() {
        return "git: [none]".to_string();
    }

    // --no-optional-locks guarantees no lock contention; --no-renames speeds up status
    let Ok(out) =
        Command::new("git").args(["--no-optional-locks", "status", "--porcelain", "-b", "--no-renames"]).output()
    else {
        return "git: [none]".to_string();
    };

    if !out.status.success() {
        return "git: [none]".to_string();
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("").trim_start_matches("##").trim();
    let branch = header.split("...").next().unwrap_or(header).split_whitespace().next().unwrap_or("detached");

    let dirty = lines.filter(|l| !l.trim().is_empty()).count();
    let status = match dirty {
        0 => "clean".to_string(),
        1 => "1 modified".to_string(),
        _ => format!("{} modified", dirty),
    };

    let res = format!("git: {} · {}", branch, status);
    if res.chars().count() > 32 {
        let truncated: String = res.chars().take(31).collect();
        format!("{}…", truncated)
    } else {
        res
    }
}

// Progressive column-sliced laser wipe for Style 2 & 3 (col == slab_w renders full frame)
fn render_dot_slab_inner_wipe(model: &str, profile: &str, col: usize) -> (String, String) {
    let slab_w: usize = 47;
    let max_avail = slab_w.saturating_sub(6);

    let model_clamped: String =
        if model.chars().count() > max_avail { model.chars().take(max_avail).collect() } else { model.to_string() };

    let profile_clamped: String = if profile.chars().count() > max_avail {
        profile.chars().take(max_avail).collect()
    } else {
        profile.to_string()
    };

    let max_txt = model_clamped.chars().count().max(profile_clamped.chars().count());
    let mut cutout_end = (3 + max_txt + 2).min(slab_w);
    if !cutout_end.is_multiple_of(2) && cutout_end < slab_w {
        cutout_end += 1;
    }

    let render_line = |text: &str, color: &'static str| -> String {
        let mut l = String::with_capacity(128);
        if col == 0 {
            return l;
        }

        // Col 0: Dot
        l.push_str(slab_shader(0, slab_w));
        l.push('·');
        l.push_str(ANSI_RESET);

        if col <= 1 {
            return l;
        }
        l.push(' ');
        if col <= 2 {
            return l;
        }
        l.push(' ');

        // Text starts at col 3
        let chars: Vec<char> = text.chars().collect();
        let txt_len = chars.len();
        let txt_visible = if col > 3 { (col - 3).min(txt_len) } else { 0 };

        if txt_visible > 0 {
            l.push_str(color);
            for c in &chars[..txt_visible] {
                l.push(*c);
            }
            l.push_str(ANSI_RESET);
        }

        // Padding spaces before dot trail
        let curr_x = 3 + txt_len;
        if col > curr_x && curr_x < cutout_end {
            let pad_end = col.min(cutout_end);
            for _ in curr_x..pad_end {
                l.push(' ');
            }
        }

        // Trailing dots
        if col > cutout_end {
            let trail_end = col.min(slab_w);
            for x in cutout_end..trail_end {
                if x % 2 == 0 {
                    l.push_str(slab_shader(x, slab_w));
                    l.push('·');
                    l.push_str(ANSI_RESET);
                } else {
                    l.push(' ');
                }
            }
        }

        l
    };

    (render_line(&model_clamped, ANSI_WHITE), render_line(&profile_clamped, ANSI_MUTED))
}

pub fn render_frame(model: &str, profile: &str, style: usize, progress: f64) -> (Vec<String>, usize) {
    let git_info = if style >= 4 { resolve_git_status() } else { String::new() };
    render_frame_inner(model, profile, &git_info, style, progress)
}

fn clamp_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else if max_chars > 1 {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    } else {
        s.chars().take(max_chars).collect()
    }
}

fn short_cwd() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let s = cwd.display().to_string();
    if let Some(home) = dirs::home_dir() {
        let h = home.display().to_string();
        if let Some(rest) = s.strip_prefix(&h) {
            return if rest.is_empty() { "~".to_string() } else { format!("~{}", rest) };
        }
    }
    s
}

fn render_claude_box(model: &str, profile: &str, git_info: &str, progress: f64) -> (Vec<String>, usize) {
    let term_w = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(92);
    let w = term_w.saturating_sub(2).clamp(48, 92);
    let ver = env!("CARGO_PKG_VERSION");
    let title_plain = format!(" rs-agent v{} ", ver);
    let title_colored = format!(" {ANSI_BRIGHT}rs-agent{ANSI_RESET} {ANSI_DIM}v{}{ANSI_RESET} ", ver);
    let top_fill = w.saturating_sub(5 + title_plain.chars().count()).max(1);
    let top = format!("{ANSI_MUTED}╭───{ANSI_RESET}{}{ANSI_MUTED}{}╮{ANSI_RESET}", title_colored, "─".repeat(top_fill));
    let bot = format!("{ANSI_MUTED}╰{}╯{ANSI_RESET}", "─".repeat(w.saturating_sub(2)));

    let (ch0, ch1) = model_initials(model);
    let cwd = short_cwd();

    let rows: Vec<String> = if w >= 74 {
        let left_w = (w - 3) / 2;
        let right_w = (w - 3) - left_w;
        let lv = left_w.saturating_sub(11);

        let m_c = clamp_chars(model, lv);
        let p_c = clamp_chars(profile, lv);
        let g_c = clamp_chars(git_info, lv);
        let cwd_c = clamp_chars(&cwd, left_w.saturating_sub(3));

        let left: [(String, usize); 4] = [
            (format!("  {ANSI_BRIGHT}╭────╮{ANSI_RESET}  {ANSI_WHITE}{}{ANSI_RESET}", m_c), 10 + m_c.chars().count()),
            (format!("  {ANSI_BRIGHT}│ {ANSI_WHITE}{}{}{ANSI_BRIGHT} │{ANSI_RESET}  {ANSI_ACCENT}{}{ANSI_RESET}", ch0, ch1, p_c), 10 + p_c.chars().count()),
            (format!("  {ANSI_BRIGHT}╰────╯{ANSI_RESET}  {ANSI_MUTED}{}{ANSI_RESET}", g_c), 10 + g_c.chars().count()),
            (format!("  {ANSI_DIM}{}{ANSI_RESET}", cwd_c), 2 + cwd_c.chars().count()),
        ];

        let r0 = clamp_chars("Shortcuts & Modes", right_w.saturating_sub(1));
        let r1 = clamp_chars("• /p profile   /s skills   /t3 ADE", right_w.saturating_sub(1));
        let r2 = clamp_chars("• /task loop   /box theme  /r reset", right_w.saturating_sub(1));
        let r3 = clamp_chars("↑/↓ history · Ctrl+C exit", right_w.saturating_sub(1));

        let right: [(String, usize); 4] = [
            (format!("\x1b[38;5;215m{}\x1b[0m", r0), r0.chars().count()),
            (format!("{ANSI_DIM}{}{ANSI_RESET}", r1), r1.chars().count()),
            (format!("{ANSI_DIM}{}{ANSI_RESET}", r2), r2.chars().count()),
            (format!("{ANSI_MUTED}{}{ANSI_RESET}", r3), r3.chars().count()),
        ];

        left.into_iter()
            .zip(right)
            .map(|((ls, l_vis), (rs, r_vis))| {
                let l_pad = " ".repeat((left_w - 1).saturating_sub(l_vis));
                let r_pad = " ".repeat((right_w - 1).saturating_sub(r_vis));
                format!("{ANSI_MUTED}│{ANSI_RESET} {ls}{l_pad}{ANSI_MUTED}│{ANSI_RESET} {rs}{r_pad}{ANSI_MUTED}│{ANSI_RESET}")
            })
            .collect()
    } else {
        let inner = w.saturating_sub(3);
        let lv = inner.saturating_sub(10);
        let m_c = clamp_chars(model, lv);
        let p_c = clamp_chars(profile, lv);
        let g_c = clamp_chars(git_info, lv);
        let cwd_c = clamp_chars(&cwd, inner.saturating_sub(2));
        let items: [(String, usize); 4] = [
            (format!(" {ANSI_BRIGHT}╭────╮{ANSI_RESET}  {ANSI_WHITE}{}{ANSI_RESET}", m_c), 9 + m_c.chars().count()),
            (format!(" {ANSI_BRIGHT}│ {ANSI_WHITE}{}{}{ANSI_BRIGHT} │{ANSI_RESET}  {ANSI_ACCENT}{}{ANSI_RESET}", ch0, ch1, p_c), 9 + p_c.chars().count()),
            (format!(" {ANSI_BRIGHT}╰────╯{ANSI_RESET}  {ANSI_MUTED}{}{ANSI_RESET}", g_c), 9 + g_c.chars().count()),
            (format!(" {ANSI_DIM}{}{ANSI_RESET}", cwd_c), 1 + cwd_c.chars().count()),
        ];
        items
            .into_iter()
            .map(|(s, vis)| {
                let pad = " ".repeat(inner.saturating_sub(vis));
                format!("{ANSI_MUTED}│{ANSI_RESET} {s}{pad}{ANSI_MUTED}│{ANSI_RESET}")
            })
            .collect()
    };

    let mut all = Vec::with_capacity(6);
    all.push(top);
    all.extend(rows);
    all.push(bot);
    let total = all.len();
    if progress >= 1.0 {
        (all, total)
    } else {
        let visible = ((total as f64) * progress).ceil() as usize;
        let out: Vec<String> = all
            .into_iter()
            .enumerate()
            .map(|(i, l)| if i < visible { l } else { String::new() })
            .collect();
        (out, total)
    }
}

fn render_frame_inner(model: &str, profile: &str, git_info: &str, style: usize, progress: f64) -> (Vec<String>, usize) {
    let box_w: usize = 46;

    match style {
        1 => {
            // Style 1: Clean Rounded (Snappy laser wipe)
            let top_border = format!("{ANSI_BRIGHT}╭─ ○ {:─<w$}╮{ANSI_RESET}", "", w = box_w - 6);
            let model_line = format!(
                "{ANSI_BRIGHT}│{ANSI_RESET}  {ANSI_WHITE}{:<w$}{ANSI_RESET}{ANSI_BRIGHT}│{ANSI_RESET}",
                model,
                w = box_w - 4
            );
            let prof_line = format!(
                "{ANSI_BRIGHT}│{ANSI_RESET}  {ANSI_MUTED}{:<w$}{ANSI_RESET}{ANSI_BRIGHT}│{ANSI_RESET}",
                profile,
                w = box_w - 4
            );
            let bot_border = format!("{ANSI_BRIGHT}╰{:─<w$}╯{ANSI_RESET}", "", w = box_w - 2);

            if progress >= 1.0 {
                (vec![top_border, model_line, prof_line, bot_border], 4)
            } else {
                let col = ((box_w as f64) * progress).round() as usize;

                let top = if col == 0 {
                    String::new()
                } else if col >= box_w {
                    top_border
                } else if col <= 5 {
                    format!("{ANSI_BRIGHT}{}{ANSI_RESET}", "╭─ ○ ".chars().take(col.min(5)).collect::<String>())
                } else {
                    let fill = col - 5;
                    format!("{ANSI_BRIGHT}╭─ ○ {:─<w$}{ANSI_RESET}", "", w = fill)
                };

                let bot = if col == 0 {
                    String::new()
                } else if col >= box_w {
                    bot_border
                } else {
                    format!("{ANSI_BRIGHT}╰{:─<w$}{ANSI_RESET}", "", w = col.saturating_sub(1))
                };

                let l1 = if col == 0 {
                    String::new()
                } else if col >= box_w {
                    model_line
                } else {
                    let inner = col.saturating_sub(3);
                    let txt: String = model.chars().take(inner).collect();
                    format!("{ANSI_BRIGHT}│{ANSI_RESET}  {ANSI_WHITE}{}{ANSI_RESET}", txt)
                };

                let l2 = if col == 0 {
                    String::new()
                } else if col >= box_w {
                    prof_line
                } else {
                    let inner = col.saturating_sub(3);
                    let txt: String = profile.chars().take(inner).collect();
                    format!("{ANSI_BRIGHT}│{ANSI_RESET}  {ANSI_MUTED}{}{ANSI_RESET}", txt)
                };

                (vec![top, l1, l2, bot], 4)
            }
        }
        2 => {
            // Style 2: Dot Slab (Nothing OS) - 4 lines with snappy laser wipe
            let slab_w: usize = 47;
            let col = if progress >= 1.0 { slab_w } else { ((slab_w as f64) * progress).round() as usize };
            let (l1, l2) = render_dot_slab_inner_wipe(model, profile, col);

            let mut top = String::with_capacity(128);
            for x in 0..col.min(slab_w) {
                if x == 4 {
                    top.push_str(ANSI_BRIGHT);
                    top.push('○');
                    top.push_str(ANSI_RESET);
                } else if x % 2 == 0 {
                    top.push_str(slab_shader(x, slab_w));
                    top.push('·');
                    top.push_str(ANSI_RESET);
                } else {
                    top.push(' ');
                }
            }

            let mut bot = String::with_capacity(128);
            for x in 0..col.min(slab_w) {
                if x % 2 == 0 {
                    bot.push_str(slab_shader(x, slab_w));
                    bot.push('·');
                    bot.push_str(ANSI_RESET);
                } else {
                    bot.push(' ');
                }
            }

            (vec![top, l1, l2, bot], 4)
        }
        3 => {
            // Style 3: Minimal Dot Trail with snappy laser wipe
            let slab_w: usize = 47;
            let col = if progress >= 1.0 { slab_w } else { ((slab_w as f64) * progress).round() as usize };
            let (l1, l2) = render_dot_slab_inner_wipe(model, profile, col);
            (vec![l1, l2], 2)
        }
        4 => {
            // Style 4: Themed Boxed Badge (Zero-allocation fixed stack grid)
            let line_count: usize = 5;
            let model_chars = model.chars().count();
            let profile_chars = profile.chars().count();
            let git_chars = git_info.chars().count();

            let mut grid: [Option<(char, &'static str)>; 230] = [None; 230];

            // Row 0
            grid[0] = Some(('╭', ANSI_BRIGHT));
            #[allow(clippy::needless_range_loop)]
            for x in 1..box_w - 1 {
                grid[x] = Some(('─', ANSI_BRIGHT));
            }
            grid[2] = Some((' ', ANSI_RESET));
            grid[3] = Some(('○', ANSI_BRIGHT));
            grid[4] = Some((' ', ANSI_RESET));
            grid[box_w - 1] = Some(('╮', ANSI_BRIGHT));

            // Row 4
            grid[4 * box_w] = Some(('╰', ANSI_BRIGHT));
            #[allow(clippy::needless_range_loop)]
            for x in 1..box_w - 1 {
                grid[4 * box_w + x] = Some(('─', ANSI_BRIGHT));
            }
            grid[4 * box_w + (box_w - 1)] = Some(('╯', ANSI_BRIGHT));

            // Rows 1..=3 borders
            for y in 1..=3 {
                grid[y * box_w] = Some(('│', ANSI_BRIGHT));
                grid[y * box_w + (box_w - 1)] = Some(('│', ANSI_BRIGHT));
            }

            // Avatar frame
            let av1 = "╭────╮";
            for (i, c) in av1.chars().enumerate() {
                grid[box_w + (3 + i)] = Some((c, ANSI_BRIGHT));
            }
            grid[2 * box_w + 3] = Some(('│', ANSI_BRIGHT));
            let (ch0, ch1) = model_initials(model);
            grid[2 * box_w + 5] = Some((ch0, ANSI_WHITE));
            grid[2 * box_w + 6] = Some((ch1, ANSI_WHITE));
            grid[2 * box_w + 8] = Some(('│', ANSI_BRIGHT));
            let av3 = "╰────╯";
            for (i, c) in av3.chars().enumerate() {
                grid[3 * box_w + (3 + i)] = Some((c, ANSI_BRIGHT));
            }

            // Content lines
            for (i, c) in model.chars().enumerate() {
                if 11 + i < box_w - 2 {
                    grid[box_w + (11 + i)] = Some((c, ANSI_WHITE));
                }
            }
            for (i, c) in profile.chars().enumerate() {
                if 11 + i < box_w - 2 {
                    grid[2 * box_w + (11 + i)] = Some((c, ANSI_ACCENT));
                }
            }
            for (i, c) in git_info.chars().enumerate() {
                if 11 + i < box_w - 2 {
                    grid[3 * box_w + (11 + i)] = Some((c, ANSI_MUTED));
                }
            }

            let reveal = calculate_reveal(progress);
            let t = progress * 2.0;
            let mut lines = Vec::with_capacity(line_count);

            for y in 0..line_count {
                let mut row = String::with_capacity(512);

                for x in 0..box_w {
                    let is_avatar = (3..9).contains(&x) && (1..=3).contains(&y);
                    let is_border = y == 0 || y == 4 || x == 0 || x == box_w - 1;
                    let is_text_bar = (1..=3).contains(&y)
                        && match y {
                            1 => x >= 11 && x < 11 + model_chars.max(16).min(box_w - 13),
                            2 => x >= 11 && x < 11 + profile_chars.max(12).min(box_w - 13),
                            3 => x >= 11 && x < 11 + git_chars.max(12).min(box_w - 13),
                            _ => false,
                        };
                    let is_skeleton = is_border || is_avatar || is_text_bar;
                    let cell_thresh = if is_avatar { cell_dither_threshold(3, 1) } else { cell_dither_threshold(x, y) };

                    if progress >= 1.0 || (reveal > 0.0 && cell_thresh < reveal) {
                        if let Some((ch, col)) = grid[y * box_w + x] {
                            row.push_str(col);
                            row.push(ch);
                            row.push_str(ANSI_RESET);
                        } else {
                            row.push(' ');
                        }
                    } else if is_skeleton {
                        push_skeleton_dot(&mut row, x, y, t);
                    } else {
                        row.push(' ');
                    }
                }

                lines.push(row);
            }

            (lines, line_count)
        }
        5 => {
            // Style 5: Borderless Floating Badge (Zero-allocation fixed stack grid)
            let line_count: usize = 3;
            let model_chars = model.chars().count();
            let profile_chars = profile.chars().count();
            let git_chars = git_info.chars().count();

            let mut grid: [Option<(char, &'static str)>; 138] = [None; 138];

            // Row 0
            let av1 = "╭────╮";
            for (i, c) in av1.chars().enumerate() {
                grid[2 + i] = Some((c, ANSI_BRIGHT));
            }
            for (i, c) in model.chars().enumerate() {
                if 10 + i < box_w {
                    grid[10 + i] = Some((c, ANSI_WHITE));
                }
            }

            // Row 1
            grid[box_w + 2] = Some(('│', ANSI_BRIGHT));
            grid[box_w + 3] = Some((' ', ANSI_RESET));
            let (ch0, ch1) = model_initials(model);
            grid[box_w + 4] = Some((ch0, ANSI_WHITE));
            grid[box_w + 5] = Some((ch1, ANSI_WHITE));
            grid[box_w + 6] = Some((' ', ANSI_RESET));
            grid[box_w + 7] = Some(('│', ANSI_BRIGHT));
            for (i, c) in profile.chars().enumerate() {
                if 10 + i < box_w {
                    grid[box_w + (10 + i)] = Some((c, ANSI_ACCENT));
                }
            }

            // Row 2
            let av3 = "╰────╯";
            for (i, c) in av3.chars().enumerate() {
                grid[2 * box_w + (2 + i)] = Some((c, ANSI_BRIGHT));
            }
            for (i, c) in git_info.chars().enumerate() {
                if 10 + i < box_w {
                    grid[2 * box_w + (10 + i)] = Some((c, ANSI_MUTED));
                }
            }

            let reveal = calculate_reveal(progress);
            let t = progress * 2.0;
            let mut lines = Vec::with_capacity(line_count);

            for y in 0..line_count {
                let bar_len = match y {
                    0 => model_chars.max(16).min(box_w - 13),
                    1 => profile_chars.max(12).min(box_w - 13),
                    _ => git_chars.max(12).min(box_w - 13),
                };

                let row_end = if progress >= 1.0 {
                    match y {
                        0 => (10 + model_chars).min(box_w),
                        1 => (10 + profile_chars).min(box_w),
                        _ => (10 + git_chars).min(box_w),
                    }
                } else {
                    (10 + bar_len).min(box_w)
                };

                let mut row = String::with_capacity(512);

                for x in 0..row_end {
                    let is_avatar = (2..8).contains(&x);
                    let is_text_bar = x >= 10 && x < 10 + bar_len;
                    let is_skeleton = is_avatar || is_text_bar;
                    let cell_thresh = if is_avatar { cell_dither_threshold(2, 0) } else { cell_dither_threshold(x, y) };

                    if progress >= 1.0 || (reveal > 0.0 && cell_thresh < reveal) {
                        if let Some((ch, col)) = grid[y * box_w + x] {
                            row.push_str(col);
                            row.push(ch);
                            row.push_str(ANSI_RESET);
                        } else {
                            row.push(' ');
                        }
                    } else if is_skeleton {
                        push_skeleton_dot(&mut row, x, y, t);
                    } else {
                        row.push(' ');
                    }
                }

                lines.push(row);
            }

            (lines, line_count)
        }
        6 => {
            // Style 6: Dot Slab Avatar Badge (Style 5 + Style 2)
            let slab_w: usize = 47;
            let line_count: usize = 5;
            let max_avail = slab_w.saturating_sub(14);

            let m_len = model.chars().count().min(max_avail);
            let p_len = profile.chars().count().min(max_avail);
            let g_len = git_info.chars().count().min(max_avail);
            let max_txt = m_len.max(p_len).max(g_len);

            let mut cutout_end = (11 + max_txt + 2).min(slab_w);
            if !cutout_end.is_multiple_of(2) && cutout_end < slab_w {
                cutout_end += 1;
            }

            let mut grid: [Option<(char, &'static str)>; 235] = [None; 235];

            // Row 0
            for (x, cell) in grid.iter_mut().enumerate().take(slab_w) {
                if x == 4 {
                    *cell = Some(('○', ANSI_BRIGHT));
                } else if x.is_multiple_of(2) {
                    *cell = Some(('·', slab_shader(x, slab_w)));
                }
            }

            // Row 4
            for x in 0..slab_w {
                if x % 2 == 0 {
                    grid[4 * slab_w + x] = Some(('·', slab_shader(x, slab_w)));
                }
            }

            // Rows 1..=3
            for y in 1..=3 {
                grid[y * slab_w] = Some(('·', slab_shader(0, slab_w)));
            }

            // Avatar frame
            let av1 = "╭────╮";
            for (i, c) in av1.chars().enumerate() {
                grid[slab_w + 3 + i] = Some((c, ANSI_BRIGHT));
            }
            grid[2 * slab_w + 3] = Some(('│', ANSI_BRIGHT));
            let (ch0, ch1) = model_initials(model);
            grid[2 * slab_w + 5] = Some((ch0, ANSI_WHITE));
            grid[2 * slab_w + 6] = Some((ch1, ANSI_WHITE));
            grid[2 * slab_w + 8] = Some(('│', ANSI_BRIGHT));
            let av3 = "╰────╯";
            for (i, c) in av3.chars().enumerate() {
                grid[3 * slab_w + 3 + i] = Some((c, ANSI_BRIGHT));
            }

            // Content lines
            for (i, c) in model.chars().take(max_avail).enumerate() {
                grid[slab_w + (11 + i)] = Some((c, ANSI_WHITE));
            }
            for (i, c) in profile.chars().take(max_avail).enumerate() {
                grid[2 * slab_w + (11 + i)] = Some((c, ANSI_ACCENT));
            }
            for (i, c) in git_info.chars().take(max_avail).enumerate() {
                grid[3 * slab_w + (11 + i)] = Some((c, ANSI_MUTED));
            }

            // Trailing dots
            for y in 1..=3 {
                for x in cutout_end..slab_w {
                    if x % 2 == 0 {
                        grid[y * slab_w + x] = Some(('·', slab_shader(x, slab_w)));
                    }
                }
            }

            let reveal = calculate_reveal(progress);
            let t = progress * 2.0;
            let mut lines = Vec::with_capacity(line_count);

            for y in 0..line_count {
                let mut row = String::with_capacity(512);

                for x in 0..slab_w {
                    let is_avatar = (3..9).contains(&x) && (1..=3).contains(&y);
                    let is_text_bar = (1..=3).contains(&y) && (x >= 11 && x < cutout_end);
                    let is_dot = (y == 0 || y == 4 || x == 0 || x >= cutout_end) && (x % 2 == 0);
                    let is_skeleton = is_avatar || is_text_bar || is_dot;
                    let cell_thresh = if is_avatar { cell_dither_threshold(3, 1) } else { cell_dither_threshold(x, y) };

                    if progress >= 1.0 || (reveal > 0.0 && cell_thresh < reveal) {
                        if let Some((ch, col)) = grid[y * slab_w + x] {
                            row.push_str(col);
                            row.push(ch);
                            row.push_str(ANSI_RESET);
                        } else {
                            row.push(' ');
                        }
                    } else if is_skeleton {
                        push_skeleton_dot(&mut row, x, y, t);
                    } else {
                        row.push(' ');
                    }
                }

                lines.push(row);
            }

            (lines, line_count)
        }

        7 => render_claude_box(model, profile, git_info, progress),
        _ => render_frame_inner(model, profile, git_info, 1, progress),
    }
}

pub fn draw_box(model: &str, profile: &str, style: usize, animate: bool) -> Option<char> {
    // Early exit without terminal switching if unbuffered or raw mode fails
    if !animate || enable_raw_mode().is_err() {
        let git_info = if style >= 4 { resolve_git_status() } else { String::new() };
        let (lines, _) = render_frame_inner(model, profile, &git_info, style, 1.0);
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        for line in lines {
            let _ = writeln!(handle, "{}", line);
        }
        let _ = writeln!(handle);
        let _ = handle.flush();
        return None;
    }

    // Zero-latency boot: spawn git status in background so Frame 0 renders immediately
    let mut git_handle = if style >= 4 { Some(std::thread::spawn(resolve_git_status)) } else { None };
    let mut git_info = String::from("git: scanning…");

    let _guard = RawModeGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, Hide);
    let _ = stdout.flush();

    let duration = match style {
        1 | 7 => Duration::from_millis(150),
        2 | 3 => Duration::from_millis(200),
        4..=6 => Duration::from_millis(550),
        _ => Duration::from_millis(200),
    };

    let start_time = Instant::now();
    let mut buffered_char = None;
    let mut first_frame = true;

    loop {
        // Collect background git status as soon as it completes
        if let Some(ref handle) = git_handle {
            if handle.is_finished() {
                if let Some(h) = git_handle.take() {
                    if let Ok(info) = h.join() {
                        git_info = info;
                    }
                }
            }
        }

        let elapsed = start_time.elapsed();
        let progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0);

        if progress >= 1.0 {
            break;
        }

        let (lines, line_count) = render_frame_inner(model, profile, &git_info, style, progress);

        let stdout = io::stdout();
        let mut handle = stdout.lock();
        if !first_frame {
            let _ = write!(handle, "\x1b[{}A\r", line_count);
        }
        first_frame = false;

        for line in &lines {
            let _ = write!(handle, "{}\r\n", line);
        }
        let _ = handle.flush();

        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    if let KeyCode::Char(c) = key.code {
                        buffered_char = Some(c);
                        break;
                    }
                }
            }
        }
    }

    // Ensure background git resolution has completed before rendering final resting frame
    if let Some(h) = git_handle.take() {
        if let Ok(info) = h.join() {
            git_info = info;
        }
    }

    // Final resting frame
    let (lines, line_count) = render_frame_inner(model, profile, &git_info, style, 1.0);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if !first_frame {
        let _ = write!(handle, "\x1b[{}A\r", line_count);
    }
    for line in &lines {
        let _ = write!(handle, "{}\r\n", line);
    }
    let _ = write!(handle, "\r\n");
    let _ = handle.flush();

    buffered_char
}


pub enum InputResult {
    Line(String),
    Eof,
}

struct InputGuard;

impl Drop for InputGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, DisableBracketedPaste, Show);
        let _ = disable_raw_mode();
        let _ = stdout.flush();
    }
}

pub fn read_ruled_input(
    status_line: &str,
    history: &[String],
    initial_char: Option<char>,
    minimal: bool,
) -> io::Result<InputResult> {
    if !io::stdin().is_terminal() || enable_raw_mode().is_err() {
        print!("\x1b[?25h❯ ");
        if let Some(c) = initial_char {
            print!("{}", c);
        }
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Ok(InputResult::Eof);
        }
        let full = if let Some(c) = initial_char {
            format!("{}{}", c, line.trim_end_matches(['\r', '\n']))
        } else {
            line.trim_end_matches(['\r', '\n']).to_string()
        };
        return Ok(InputResult::Line(full));
    }

    let _guard = InputGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, EnableBracketedPaste, Show);

    let mut buf: Vec<char> = Vec::with_capacity(128);
    if let Some(c) = initial_char {
        if !c.is_control() {
            buf.push(c);
        }
    }
    let mut cur: usize = buf.len();
    let mut view_start: usize = 0;
    let mut hist_idx: Option<usize> = None;
    let mut draft: Vec<char> = Vec::new();
    let mut first_draw = true;

    loop {
        let term_w = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(96);
        let w = term_w.clamp(40, 120);
        let rule = "─".repeat(w);
        let text_avail = w.saturating_sub(3).max(10);

        if cur < view_start {
            view_start = cur;
        } else if cur > view_start + text_avail {
            view_start = cur - text_avail;
        }
        let view_end = (view_start + text_avail).min(buf.len());
        let visible_text: String = buf[view_start..view_end].iter().collect();
        let show_hint = buf.first() == Some(&'/');

        let status_clamped = clamp_chars(status_line, w.saturating_sub(2));
        let hint_clamped = clamp_chars(
            "  Commands: /p profile · /s skills · /task loop · /t3 ADE · /box theme · /pad · /r reset",
            w.saturating_sub(1),
        );

        {
            let mut handle = stdout.lock();
            let col = 3 + (cur - view_start);
            if minimal {
                let _ = write!(handle, "\r\x1b[2K{ANSI_BRIGHT}❯{ANSI_RESET} {visible_text}\x1b[{col}G");
                first_draw = false;
            } else {
                if !first_draw {
                    let _ = write!(handle, "\x1b[1A\r");
                }
                first_draw = false;

                let _ = write!(handle, "\x1b[2K{ANSI_MUTED}{rule}{ANSI_RESET}\r\n");
                let _ = write!(handle, "\x1b[2K{ANSI_BRIGHT}❯{ANSI_RESET} {visible_text}\r\n");
                let _ = write!(handle, "\x1b[2K{ANSI_MUTED}{rule}{ANSI_RESET}\r\n");
                let _ = write!(handle, "\x1b[2K{ANSI_DIM}  {status_clamped}{ANSI_RESET}");

                let up = if show_hint {
                    let _ = write!(handle, "\r\n\x1b[2K{ANSI_MUTED}{hint_clamped}{ANSI_RESET}\x1b[J");
                    3
                } else {
                    let _ = write!(handle, "\x1b[J");
                    2
                };

                let _ = write!(handle, "\x1b[{up}A\x1b[{col}G");
            }
            let _ = handle.flush();
        }

        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    let submitted: String = buf.iter().collect();
                    {
                        let mut handle = stdout.lock();
                        let clear_seq = if minimal { "\r\x1b[2K" } else { "\x1b[1A\r\x1b[J" };
                        let _ = write!(handle, "{clear_seq}");
                        let _ = handle.flush();
                    }
                    drop(_guard);
                    if !submitted.trim().is_empty() {
                        println!("{ANSI_BRIGHT}❯{ANSI_RESET} {}", submitted);
                    }
                    return Ok(InputResult::Line(submitted));
                }
                KeyCode::Char('c') | KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if key.code == KeyCode::Char('d') && !buf.is_empty() {
                        if cur < buf.len() {
                            buf.remove(cur);
                        }
                        continue;
                    }
                    let mut handle = stdout.lock();
                    let clear_seq = if minimal { "\r\x1b[2K" } else { "\x1b[1A\r\x1b[J" };
                    let _ = write!(handle, "{clear_seq}");
                    let _ = handle.flush();
                    return Ok(InputResult::Eof);
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    cur = 0;
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    cur = buf.len();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buf.clear();
                    cur = 0;
                    view_start = 0;
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    while cur > 0 && buf[cur - 1].is_whitespace() {
                        cur -= 1;
                        buf.remove(cur);
                    }
                    while cur > 0 && !buf[cur - 1].is_whitespace() {
                        cur -= 1;
                        buf.remove(cur);
                    }
                }
                KeyCode::Backspace if cur > 0 => {
                    cur -= 1;
                    buf.remove(cur);
                }
                KeyCode::Delete if cur < buf.len() => {
                    buf.remove(cur);
                }
                KeyCode::Left => {
                    cur = cur.saturating_sub(1);
                }
                KeyCode::Right if cur < buf.len() => {
                    cur += 1;
                }
                KeyCode::Home => {
                    cur = 0;
                }
                KeyCode::End => {
                    cur = buf.len();
                }
                KeyCode::Up if !history.is_empty() => {
                    if hist_idx.is_none() {
                        draft = buf.clone();
                        hist_idx = Some(history.len() - 1);
                    } else if let Some(idx) = hist_idx {
                        if idx > 0 {
                            hist_idx = Some(idx - 1);
                        }
                    }
                    if let Some(idx) = hist_idx {
                        buf = history[idx].chars().collect();
                        cur = buf.len();
                        view_start = 0;
                    }
                }
                KeyCode::Down => {
                    if let Some(idx) = hist_idx {
                        if idx + 1 < history.len() {
                            hist_idx = Some(idx + 1);
                            buf = history[idx + 1].chars().collect();
                        } else {
                            hist_idx = None;
                            buf = draft.clone();
                        }
                        cur = buf.len();
                        view_start = 0;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    buf.insert(cur, c);
                    cur += 1;
                }
                _ => {}
            },
            Ok(Event::Paste(pasted)) => {
                for c in pasted.chars() {
                    if c == '\r' || c == '\n' {
                        if buf.get(cur.saturating_sub(1)) != Some(&' ') {
                            buf.insert(cur, ' ');
                            cur += 1;
                        }
                    } else if !c.is_control() {
                        buf.insert(cur, c);
                        cur += 1;
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {
                let mut handle = stdout.lock();
                let clear_seq = if minimal { "\r\x1b[2K" } else { "\x1b[1A\r\x1b[J" };
                let _ = write!(handle, "{clear_seq}");
                let _ = handle.flush();
                return Ok(InputResult::Eof);
            }
        }
    }
}

pub fn authorize_prompt(violation: &str) -> bool {
    if !io::stdin().is_terminal() || enable_raw_mode().is_err() {
        return false;
    }

    let _guard = RawModeGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, Hide);

    let term_w = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(72);
    let w = term_w.saturating_sub(4).clamp(42, 68);
    let msg = clamp_chars(violation, w.saturating_sub(4));
    let options = ["1. Yes, allow once", "2. No (Esc)"];
    let mut sel: usize = 1; // Safe default: No
    let mut first = true;

    loop {
        {
            let mut handle = stdout.lock();
            if !first {
                let _ = write!(handle, "\x1b[6A\r");
            }
            first = false;

            let title = " Security Gate ";
            let fill = w.saturating_sub(4 + title.chars().count()).max(1);
            let pad = " ".repeat(w.saturating_sub(4 + msg.chars().count()));
            let _ = write!(handle, "\x1b[2K  {ANSI_MUTED}╭─{ANSI_RESET}\x1b[38;5;215m{title}{ANSI_RESET}{ANSI_MUTED}{}╮{ANSI_RESET}\r\n", "─".repeat(fill));
            let _ = write!(handle, "\x1b[2K  {ANSI_MUTED}│{ANSI_RESET} {ANSI_WHITE}{msg}{ANSI_RESET}{pad} {ANSI_MUTED}│{ANSI_RESET}\r\n");
            let _ = write!(handle, "\x1b[2K  {ANSI_MUTED}╰{}╯{ANSI_RESET}\r\n", "─".repeat(w.saturating_sub(2)));
            let _ = write!(handle, "\x1b[2K  {ANSI_DIM}Do you want to proceed?{ANSI_RESET}\r\n");
            for (i, opt) in options.iter().enumerate() {
                if i == sel {
                    let _ = write!(handle, "\x1b[2K  {ANSI_BRIGHT}❯ {ANSI_WHITE}{opt}{ANSI_RESET}\r\n");
                } else {
                    let _ = write!(handle, "\x1b[2K    {ANSI_DIM}{opt}{ANSI_RESET}\r\n");
                }
            }
            let _ = handle.flush();
        }

        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Up | KeyCode::Down => sel = 1 - sel,
                KeyCode::Char('1') | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    sel = 0;
                    break;
                }
                KeyCode::Char('2') | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    sel = 1;
                    break;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    sel = 1;
                    break;
                }
                KeyCode::Enter => break,
                _ => {}
            },
            Ok(_) => {}
            Err(_) => {
                sel = 1;
                break;
            }
        }
    }

    {
        let mut handle = stdout.lock();
        let _ = write!(handle, "\x1b[6A\r\x1b[J");
        let _ = handle.flush();
    }
    drop(_guard);

    let approved = sel == 0;
    if approved {
        println!("  \x1b[1;32m✓\x1b[0m \x1b[2mAuthorized: {}\x1b[0m", msg);
    } else {
        println!("  \x1b[1;31m✗\x1b[0m \x1b[2mDenied: {}\x1b[0m", msg);
    }
    approved
}
