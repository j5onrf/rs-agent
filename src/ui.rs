use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};
use std::process::Command;
use std::time::{Duration, Instant};

// Exact ANSI constants (inherits active terminal palette)
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BRIGHT: &str = "\x1b[1;36m"; // Primary theme accent
const ANSI_ACCENT: &str = "\x1b[36m"; // Regular theme accent
const ANSI_WHITE: &str = "\x1b[1;37m"; // Bold foreground
const ANSI_MUTED: &str = "\x1b[90m"; // Color 8 / Muted
const ANSI_DIM: &str = "\x1b[2m"; // Faint / Low contrast

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

fn resolve_git_status() -> String {
    let output = Command::new("git").args(["status", "--porcelain", "-b"]).output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut lines = text.lines();
            let header = lines.next().unwrap_or("").trim_start_matches("##").trim();

            let branch = header.split("...").next().unwrap_or(header).split_whitespace().next().unwrap_or("detached");

            let dirty = lines.filter(|l| !l.trim().is_empty()).count();
            let status = if dirty == 0 {
                "clean".to_string()
            } else if dirty == 1 {
                "1 modified".to_string()
            } else {
                format!("{} modified", dirty)
            };

            let res = format!("git: {} · {}", branch, status);
            if res.chars().count() > 32 {
                let truncated: String = res.chars().take(31).collect();
                format!("{}…", truncated)
            } else {
                res
            }
        }
        _ => "git: [none]".to_string(),
    }
}

// Shared inner rows for Style 2 (Dot Slab) and Style 3 (Minimal Dot Trail)
#[allow(clippy::manual_is_multiple_of)]
fn render_dot_slab_inner(model: &str, profile: &str) -> (String, String) {
    let slab_w: usize = 47;
    let max_txt = model.chars().count().max(profile.chars().count());
    let mut cutout_end = 3 + max_txt + 2;
    if cutout_end % 2 != 0 {
        cutout_end += 1;
    }

    let mut l1 = String::with_capacity(128);
    l1.push_str(slab_shader(0, slab_w));
    l1.push('·');
    l1.push_str(ANSI_RESET);
    l1.push_str("  ");
    l1.push_str(ANSI_WHITE);
    l1.push_str(model);
    l1.push_str(ANSI_RESET);
    let curr_vis1 = 3 + model.chars().count();
    if curr_vis1 < cutout_end {
        for _ in 0..(cutout_end - curr_vis1) {
            l1.push(' ');
        }
    }
    for x in cutout_end..slab_w {
        if x % 2 == 0 {
            l1.push_str(slab_shader(x, slab_w));
            l1.push('·');
            l1.push_str(ANSI_RESET);
        } else {
            l1.push(' ');
        }
    }

    let mut l2 = String::with_capacity(128);
    l2.push_str(slab_shader(0, slab_w));
    l2.push('·');
    l2.push_str(ANSI_RESET);
    l2.push_str("  ");
    l2.push_str(ANSI_MUTED);
    l2.push_str(profile);
    l2.push_str(ANSI_RESET);
    let curr_vis2 = 3 + profile.chars().count();
    if curr_vis2 < cutout_end {
        for _ in 0..(cutout_end - curr_vis2) {
            l2.push(' ');
        }
    }
    for x in cutout_end..slab_w {
        if x % 2 == 0 {
            l2.push_str(slab_shader(x, slab_w));
            l2.push('·');
            l2.push_str(ANSI_RESET);
        } else {
            l2.push(' ');
        }
    }

    (l1, l2)
}

pub fn render_frame(model: &str, profile: &str, style: usize, progress: f64) -> (Vec<String>, usize) {
    let git_info = resolve_git_status();
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
                    format!("{ANSI_BRIGHT}{}{ANSI_RESET}", &"╭─ ○ "[..col.min(5)])
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
            // Style 2: Dot Slab (Nothing OS) - 4 lines, 47 cols odd
            let slab_w: usize = 47;
            let (l1, l2) = render_dot_slab_inner(model, profile);

            let mut top = String::with_capacity(128);
            for x in 0..slab_w {
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
            for x in 0..slab_w {
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
            // Style 3: Minimal Dot Trail (Exact Style 2 inner rows without top and bottom)
            let (l1, l2) = render_dot_slab_inner(model, profile);
            (vec![l1, l2], 2)
        }
        4 => {
            // Style 4: Themed Boxed Badge (Zero-allocation fixed stack grid)
            let line_count: usize = 5;
            let initials = if model.len() >= 2 { &model[..2] } else { "RA" };

            // Stack grid: 46 * 5 = 230 cells (no heap allocation, zero hash lookups)
            let mut grid: [Option<(char, &'static str)>; 230] = [None; 230];

            // Row 0
            grid[0] = Some(('╭', ANSI_BRIGHT));
            #[allow(clippy::needless_range_loop)]
            for x in 1..box_w - 1 {
                grid[x] = Some(('─', ANSI_BRIGHT));
            }
            grid[box_w - 1] = Some(('╮', ANSI_BRIGHT));

            // Row 4
            grid[4 * box_w] = Some(('╰', ANSI_BRIGHT));
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
            for (i, c) in initials.chars().take(2).enumerate() {
                grid[2 * box_w + (5 + i)] = Some((c, ANSI_WHITE));
            }
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

            let reveal = if progress < 0.35 {
                0.0
            } else if progress >= 1.0 {
                1.0
            } else {
                ((progress - 0.35) / 0.65).min(1.0)
            };

            let t = progress * 2.0;
            let mut lines = Vec::with_capacity(line_count);

            for y in 0..line_count {
                let mut row = String::with_capacity(256);

                for x in 0..box_w {
                    let is_avatar = (3..9).contains(&x) && (1..=3).contains(&y);
                    let is_border = y == 0 || y == 4 || x == 0 || x == box_w - 1;
                    let is_text_bar = (1..=3).contains(&y)
                        && match y {
                            1 => x >= 11 && x < 11 + model.len().max(16).min(box_w - 13),
                            2 => x >= 11 && x < 11 + profile.len().max(12).min(box_w - 13),
                            3 => x >= 11 && x < 11 + git_info.chars().count().max(12).min(box_w - 13),
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
                        let band = ((x as f64 * 0.16 - y as f64 * 0.32 - t * 2.8) * 1.3).cos();
                        let intensity = if band > 0.0 { band.powi(6) } else { 0.0 };

                        if intensity > 0.6 {
                            row.push_str(ANSI_WHITE);
                            row.push('●');
                            row.push_str(ANSI_RESET);
                        } else if intensity > 0.25 {
                            row.push_str(ANSI_BRIGHT);
                            row.push('•');
                            row.push_str(ANSI_RESET);
                        } else if intensity > 0.05 {
                            row.push_str(ANSI_ACCENT);
                            row.push('·');
                            row.push_str(ANSI_RESET);
                        } else {
                            row.push_str(ANSI_MUTED);
                            row.push('·');
                            row.push_str(ANSI_RESET);
                        }
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
            let initials = if model.len() >= 2 { &model[..2] } else { "RA" };

            let model_len = model.chars().count();
            let profile_len = profile.chars().count();
            let git_len = git_info.chars().count();

            // Stack grid: 46 * 3 = 138 cells
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
            for (i, c) in initials.chars().take(2).enumerate() {
                grid[box_w + (4 + i)] = Some((c, ANSI_WHITE));
            }
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

            let reveal = if progress < 0.35 {
                0.0
            } else if progress >= 1.0 {
                1.0
            } else {
                ((progress - 0.35) / 0.65).min(1.0)
            };

            let t = progress * 2.0;
            let mut lines = Vec::with_capacity(line_count);

            for y in 0..line_count {
                let bar_len = match y {
                    0 => model_len.max(16).min(box_w - 13),
                    1 => profile_len.max(12).min(box_w - 13),
                    _ => git_len.max(12).min(box_w - 13),
                };

                let row_end = if progress >= 1.0 {
                    match y {
                        0 => 10 + model_len,
                        1 => 10 + profile_len,
                        _ => 10 + git_len,
                    }
                } else {
                    10 + bar_len
                };

                let mut row = String::with_capacity(128);

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
                        let band = ((x as f64 * 0.16 - y as f64 * 0.32 - t * 2.8) * 1.3).cos();
                        let intensity = if band > 0.0 { band.powi(6) } else { 0.0 };

                        if intensity > 0.6 {
                            row.push_str(ANSI_WHITE);
                            row.push('●');
                            row.push_str(ANSI_RESET);
                        } else if intensity > 0.25 {
                            row.push_str(ANSI_BRIGHT);
                            row.push('•');
                            row.push_str(ANSI_RESET);
                        } else if intensity > 0.05 {
                            row.push_str(ANSI_ACCENT);
                            row.push('·');
                            row.push_str(ANSI_RESET);
                        } else {
                            row.push_str(ANSI_MUTED);
                            row.push('·');
                            row.push_str(ANSI_RESET);
                        }
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
    let git_info = resolve_git_status();

    if !animate {
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

    let _ = enable_raw_mode();
    print!("\x1b[?25l");
    let _ = io::stdout().flush();

    let duration = match style {
        1 => Duration::from_millis(160),
        4 | 5 => Duration::from_millis(550),
        _ => Duration::from_millis(100),
    };

    let start_time = Instant::now();
    let mut buffered_char = None;
    let mut first_frame = true;

    loop {
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

    let _ = disable_raw_mode();
    print!("\x1b[?25h");
    let _ = io::stdout().flush();

    buffered_char
}
