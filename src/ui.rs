use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};
use std::time::{Duration, Instant};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_MUTED: &str = "\x1b[90m";
const ANSI_ACCENT: &str = "\x1b[36m";
const ANSI_BRIGHT: &str = "\x1b[1;36m";
const ANSI_WHITE: &str = "\x1b[1;37m";

fn slab_shader(x: usize) -> &'static str {
    let ratio = x as f64 / 46.0;
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

pub fn render_frame(model: &str, profile: &str, style: usize, progress: f64) -> (Vec<String>, usize) {
    let mut lines = Vec::new();

    match style {
        // Preset #3: 2-Line Minimal Floating Dots
        3 => {
            let box_w = 47;
            let max_txt = model.chars().count().max(profile.chars().count()).min(26);
            let cutout_end = (((3 + max_txt + 3) / 2) * 2).min(box_w - 6);
            let front_x = (box_w as f64 + 8.0) * progress;

            for (text, color) in [(model, ANSI_WHITE), (profile, ANSI_MUTED)] {
                let mut row = String::new();
                let clean_txt: String = text.chars().take(max_txt).collect();

                if front_x > 0.0 {
                    row.push_str(&format!("{}·{}  ", slab_shader(0), ANSI_RESET));
                } else {
                    row.push_str("   ");
                }

                for (idx, ch) in clean_txt.chars().enumerate() {
                    let col_idx = 3 + idx;
                    if front_x > col_idx as f64 {
                        row.push_str(&format!("{}{}{}", color, ch, ANSI_RESET));
                    } else {
                        row.push(' ');
                    }
                }

                let cur_len = 3 + clean_txt.chars().count();
                for _ in cur_len..cutout_end {
                    row.push(' ');
                }

                for x in cutout_end..box_w {
                    if x % 2 == 0 {
                        if front_x > x as f64 {
                            row.push_str(&format!("{}·{}", slab_shader(x), ANSI_RESET));
                        } else {
                            row.push(' ');
                        }
                    } else {
                        row.push(' ');
                    }
                }
                lines.push(row);
            }
        }

        // Preset #2: Original Dot Slab (4-line gradient box)
        2 => {
            let box_w = 47;
            let front_x = (box_w as f64 + 8.0) * progress;

            let mut top = String::new();
            for x in 0..box_w {
                if front_x > x as f64 {
                    if x == 4 { top.push('○'); }
                    else if x == 5 { top.push(' '); }
                    else if x % 2 == 0 { top.push_str(slab_shader(x)); top.push('·'); top.push_str(ANSI_RESET); }
                    else { top.push(' '); }
                } else {
                    top.push(' ');
                }
            }
            lines.push(top);

            let max_txt = model.chars().count().max(profile.chars().count()).min(26);
            let cutout_end = (((3 + max_txt + 3) / 2) * 2).min(box_w - 6);

            for (text, color) in [(model, ANSI_WHITE), (profile, ANSI_MUTED)] {
                let mut row = String::new();
                let clean_txt: String = text.chars().take(max_txt).collect();

                if front_x > 0.0 { row.push_str(&format!("{}·{}  ", slab_shader(0), ANSI_RESET)); } else { row.push_str("   "); }
                for (idx, ch) in clean_txt.chars().enumerate() {
                    if front_x > (idx + 3) as f64 { row.push_str(&format!("{}{}{}", color, ch, ANSI_RESET)); } else { row.push(' '); }
                }
                for _ in (3 + clean_txt.chars().count())..cutout_end { row.push(' '); }
                for x in cutout_end..box_w {
                    if x % 2 == 0 {
                        if front_x > x as f64 { row.push_str(&format!("{}·{}", slab_shader(x), ANSI_RESET)); } else { row.push(' '); }
                    } else { row.push(' '); }
                }
                lines.push(row);
            }

            let mut bot = String::new();
            for x in 0..box_w {
                if front_x > x as f64 {
                    if x % 2 == 0 { bot.push_str(&format!("{}·{}", slab_shader(x), ANSI_RESET)); } else { bot.push(' '); }
                } else { bot.push(' '); }
            }
            lines.push(bot);
        }

        // Preset #1: Clean Rounded (Default 4-line wireframe)
        _ => {
            let box_w = 46;
            let interior_w = box_w - 4; // 42 chars inside: 3 left ("│  ") + 42 pad + 1 right ("│") = 46
            let front_x = (box_w as f64 + 8.0) * progress;

            let mut top = String::new();
            for x in 0..box_w {
                if front_x > x as f64 {
                    if x == 0 { top.push('╭'); }
                    else if x == 1 { top.push('─'); }
                    else if x == 2 { top.push(' '); }
                    else if x == 3 { top.push('○'); }
                    else if x == 4 { top.push(' '); }
                    else if x == box_w - 1 { top.push('╮'); }
                    else { top.push('─'); }
                } else {
                    top.push(' ');
                }
            }
            lines.push(format!("{}{}{}", ANSI_BRIGHT, top, ANSI_RESET));

            for (text, color) in [(model, ANSI_WHITE), (profile, ANSI_MUTED)] {
                let mut row = String::new();
                let clean: String = text.chars().take(interior_w).collect();
                let pad = interior_w.saturating_sub(clean.chars().count());

                if front_x > 0.0 {
                    row.push_str(&format!("{}│{}  ", ANSI_BRIGHT, ANSI_RESET));
                } else {
                    row.push_str("   ");
                }

                for (idx, ch) in clean.chars().enumerate() {
                    if front_x > (idx + 3) as f64 {
                        row.push_str(&format!("{}{}{}", color, ch, ANSI_RESET));
                    } else {
                        row.push(' ');
                    }
                }

                for _ in 0..pad {
                    row.push(' ');
                }

                if front_x >= (box_w - 1) as f64 {
                    row.push_str(&format!("{}│{}", ANSI_BRIGHT, ANSI_RESET));
                } else {
                    row.push(' ');
                }

                lines.push(row);
            }

            let mut bot = String::new();
            for x in 0..box_w {
                if front_x > x as f64 {
                    if x == 0 { bot.push('╰'); }
                    else if x == box_w - 1 { bot.push('╯'); }
                    else { bot.push('─'); }
                } else {
                    bot.push(' ');
                }
            }
            lines.push(format!("{}{}{}", ANSI_BRIGHT, bot, ANSI_RESET));
        }
    }

    let count = lines.len();
    (lines, count)
}

pub fn draw_box(model: &str, profile: &str, style: usize, animate: bool) -> Option<char> {
    if !animate || !crossterm::tty::IsTty::is_tty(&io::stdout()) {
        let (lines, _) = render_frame(model, profile, style, 1.0);
        for l in lines {
            println!("{}", l);
        }
        println!();
        return None;
    }

    let _ = enable_raw_mode();
    let mut stdout = io::stdout();
    print!("\x1b[?25l");
    let _ = stdout.flush();

    let duration = Duration::from_millis(350);
    let start = Instant::now();
    let mut captured_key = None;
    let mut first = true;
    let mut line_count = 0;

    loop {
        if event::poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(Event::Key(k)) = event::read() {
                if let KeyCode::Char(c) = k.code {
                    captured_key = Some(c);
                }
            }
            break;
        }

        let elapsed = start.elapsed();
        if elapsed >= duration {
            break;
        }

        let progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0);
        let (lines, cnt) = render_frame(model, profile, style, progress);

        if !first {
            print!("\x1b[{}A\r", line_count);
        }
        first = false;
        line_count = cnt;

        for l in &lines {
            print!("\r\x1b[2K{}\r\n", l);
        }
        let _ = stdout.flush();

        std::thread::sleep(Duration::from_millis(16));
    }

    let (final_lines, cnt) = render_frame(model, profile, style, 1.0);
    if !first {
        print!("\x1b[{}A\r", cnt);
    }
    for l in &final_lines {
        print!("\r\x1b[2K{}\r\n", l);
    }
    print!("\r\n\x1b[?25h");
    let _ = stdout.flush();
    let _ = disable_raw_mode();

    captured_key
}
