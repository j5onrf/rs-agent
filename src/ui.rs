use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

fn cell_dither_threshold(x: usize, y: usize) -> f64 {
    let mut n = (x as u64 * 374761393) ^ (y as u64 * 668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) % 1000) as f64 / 1000.0
}

pub fn render_frame(model: &str, profile: &str, style: usize, progress: f64) -> (Vec<String>, usize) {
    let box_w: usize = 46;

    match style {
        1 => {
            let top_border = format!("╭─ ○ {:─<w$}╮", "", w = box_w - 6);
            let model_line = format!("│  {:<w$}│", model, w = box_w - 4);
            let prof_line = format!("│  {:<w$}│", profile, w = box_w - 4);
            let bot_border = format!("╰{:─<w$}╯", "", w = box_w - 2);

            if progress >= 1.0 {
                (vec![top_border, model_line, prof_line, bot_border], 4)
            } else {
                let max_col = (box_w as f64 * progress) as usize;
                (vec![
                    top_border.chars().take(max_col).collect(),
                    model_line.chars().take(max_col).collect(),
                    prof_line.chars().take(max_col).collect(),
                    bot_border.chars().take(max_col).collect(),
                ], 4)
            }
        }
        2 => {
            let top = "· · ○ · · · · · · · · · · · · · · · · · · · · ·".to_string();
            let m_str = format!("·  {:<12} · · · · · · · · · · · · · · ·", model);
            let p_str = format!("·  {:<12} · · · · · · · · · · · · · · ·", profile);
            let bot = "· · · · · · · · · · · · · · · · · · · · · · · ·".to_string();

            if progress >= 1.0 {
                (vec![top, m_str, p_str, bot], 4)
            } else {
                let max_col = (box_w as f64 * progress) as usize;
                (vec![
                    top.chars().take(max_col).collect(),
                    m_str.chars().take(max_col).collect(),
                    p_str.chars().take(max_col).collect(),
                    bot.chars().take(max_col).collect(),
                ], 4)
            }
        }
        3 => {
            let m_str = format!("·  {:<12} · · · · · · · · · · · · · · ·", model);
            let p_str = format!("·  {:<12} · · · · · · · · · · · · · · ·", profile);

            if progress >= 1.0 {
                (vec![m_str, p_str], 2)
            } else {
                let max_col = (box_w as f64 * progress) as usize;
                (vec![
                    m_str.chars().take(max_col).collect(),
                    p_str.chars().take(max_col).collect(),
                ], 2)
            }
        }
        4 => {
            let line_count: usize = 5;
            let initials = if model.len() >= 2 { &model[..2] } else { "RA" };

            let mut lines = Vec::with_capacity(line_count);
            lines.push("╭────────────────────────────────────────────╮".to_string());

            let mut real_chars: HashMap<(usize, usize), (char, &'static str)> = HashMap::new();

            // 1. Avatar frame (cols 3..9, rows 1..3)
            let av1 = "╭────╮";
            let av2 = format!("│ {:<2} │", initials);
            let av3 = "╰────╯";
            for (i, c) in av1.chars().enumerate() { real_chars.insert((3 + i, 1), (c, "\x1b[38;2;0;242;254m")); }
            for (i, c) in av2.chars().enumerate() { real_chars.insert((3 + i, 2), (c, "\x1b[1;37m")); }
            for (i, c) in av3.chars().enumerate() { real_chars.insert((3 + i, 3), (c, "\x1b[38;2;58;123;213m")); }

            // 2. Dynamic model line (row 1, col 11..)
            for (i, c) in model.chars().enumerate() {
                if 11 + i < box_w - 2 { real_chars.insert((11 + i, 1), (c, "\x1b[1;37m")); }
            }

            // 3. Dynamic profile line (row 2, col 11..)
            for (i, c) in profile.chars().enumerate() {
                if 11 + i < box_w - 2 { real_chars.insert((11 + i, 2), (c, "\x1b[38;2;0;242;254m")); }
            }

            let bars: [(usize, usize, usize); 5] = [
                (3, 1, 6),
                (3, 2, 6),
                (3, 3, 6),
                (11, 1, model.len().max(16).min(box_w - 13)),
                (11, 2, profile.len().max(12).min(box_w - 13)),
            ];

            let reveal = if progress < 0.35 {
                0.0
            } else if progress >= 1.0 {
                1.0
            } else {
                ((progress - 0.35) / 0.65).min(1.0)
            };

            let t = progress * 2.0;

            for y in 1..=3 {
                let mut row = String::with_capacity(256);
                row.push('│');

                for x in 1..=(box_w - 2) {
                    let is_avatar = x >= 3 && x < 9 && y >= 1 && y <= 3;
                    let cell_thresh = if is_avatar { cell_dither_threshold(3, 1) } else { cell_dither_threshold(x, y) };
                    let in_bar = bars.iter().any(|&(bx, by, bl)| y == by && x >= bx && x < bx + bl);

                    if progress >= 1.0 || (reveal > 0.0 && cell_thresh < reveal) {
                        if let Some(&(ch, col)) = real_chars.get(&(x, y)) {
                            row.push_str(col);
                            row.push(ch);
                            row.push_str("\x1b[0m");
                        } else {
                            row.push(' ');
                        }
                    } else if in_bar {
                        let band = ((x as f64 * 0.16 - y as f64 * 0.32 - t * 2.8) * 1.3).cos();
                        let intensity = if band > 0.0 { band.powi(6) } else { 0.0 };

                        if intensity > 0.6 {
                            row.push_str("\x1b[38;2;235;248;255m●\x1b[0m");
                        } else if intensity > 0.25 {
                            row.push_str("\x1b[38;2;0;242;254m•\x1b[0m");
                        } else if intensity > 0.05 {
                            row.push_str("\x1b[38;2;58;123;213m·\x1b[0m");
                        } else {
                            row.push_str("\x1b[38;2;45;65;90m·\x1b[0m");
                        }
                    } else {
                        row.push(' ');
                    }
                }

                while row.chars().filter(|&c| c != '\x1b' && !c.is_ascii_control()).count() < box_w - 1 {
                    row.push(' ');
                }
                row.push('│');
                lines.push(row);
            }

            lines.push("╰────────────────────────────────────────────╯".to_string());
            (lines, line_count)
        }
        _ => render_frame(model, profile, 1, progress),
    }
}

pub fn draw_box(model: &str, profile: &str, style: usize, animate: bool) -> Option<char> {
    if !animate {
        let (lines, _) = render_frame(model, profile, style, 1.0);
        for line in lines {
            println!("{}", line);
        }
        let _ = io::stdout().flush();
        return None;
    }

    let _ = enable_raw_mode();
    let duration = Duration::from_millis(500);
    let start_time = Instant::now();
    let mut buffered_char = None;
    let mut last_line_count = 0;

    loop {
        let elapsed = start_time.elapsed();
        let progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0);

        let (lines, line_count) = render_frame(model, profile, style, progress);
        last_line_count = line_count;

        print!("\r");
        for line in &lines {
            print!("{}\r\n", line);
        }
        print!("\x1b[{}A\r", line_count);
        let _ = io::stdout().flush();

        if event::poll(Duration::from_millis(25)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    if let KeyCode::Char(c) = key.code {
                        buffered_char = Some(c);
                        break;
                    }
                }
            }
        }

        if progress >= 1.0 {
            break;
        }
    }

    let _ = disable_raw_mode();
    print!("\x1b[{}B\r\n", last_line_count);
    let _ = io::stdout().flush();

    buffered_char
}
