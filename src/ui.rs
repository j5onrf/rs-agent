use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

// Exact ANSI constants (inherits active terminal palette)
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BRIGHT: &str = "\x1b[1;36m"; // Primary theme accent
const ANSI_ACCENT: &str = "\x1b[36m";   // Regular theme accent
const ANSI_WHITE: &str = "\x1b[1;37m";  // Bold foreground
const ANSI_MUTED: &str = "\x1b[90m";    // Color 8 / Muted
const ANSI_DIM: &str = "\x1b[2m";       // Faint / Low contrast

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
    let Ok(out) = Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain", "-b", "--no-renames"])
        .output()
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
    if cutout_end % 2 != 0 && cutout_end < slab_w {
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
            if cutout_end % 2 != 0 && cutout_end < slab_w {
                cutout_end += 1;
            }

            let mut grid: [Option<(char, &'static str)>; 235] = [None; 235];

            // Row 0
            for x in 0..slab_w {
                if x == 4 {
                    grid[x] = Some(('○', ANSI_BRIGHT));
                } else if x % 2 == 0 {
                    grid[x] = Some(('·', slab_shader(x, slab_w)));
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
    let mut git_handle = if style >= 4 {
        Some(std::thread::spawn(resolve_git_status))
    } else {
        None
    };
    let mut git_info = String::from("git: scanning…");

    let _guard = RawModeGuard;
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(stdout, Hide);
    let _ = stdout.flush();

    let duration = match style {
        1 => Duration::from_millis(160),
        2 | 3 => Duration::from_millis(200),
        4 | 5 | 6 => Duration::from_millis(550),
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
