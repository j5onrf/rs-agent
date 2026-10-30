use std::io::{self, Write};
use std::time::Instant;

#[derive(Debug, Default, Clone)]
pub struct UsageStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub reasoning_tokens: usize,
    pub cached_tokens: usize,
}

pub struct StreamRenderer {
    pub thinking_started: bool,
    pub answer_started: bool,
    pub at_line_start: bool,
    pub think_col: usize,
    pub think_word_buf: String,
    pub avail_width: usize,
    pub thinking_chars: usize,
    pub answer_chars: usize,
    pub first_token_time: Option<Instant>,
}

impl StreamRenderer {
    pub fn new() -> Self {
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);
        let avail_width = term_width.saturating_sub(4).max(20);

        Self {
            thinking_started: false,
            answer_started: false,
            at_line_start: true,
            think_col: 0,
            think_word_buf: String::with_capacity(64),
            avail_width,
            thinking_chars: 0,
            answer_chars: 0,
            first_token_time: None,
        }
    }

    fn flush_think_word(&mut self) {
        if self.think_word_buf.is_empty() {
            return;
        }

        let w_len = self.think_word_buf.chars().count();

        if self.at_line_start {
            let clean = self.think_word_buf.trim_start();
            print!("\x1b[2m│ \x1b[0m\x1b[2m{}", clean);
            self.think_col = clean.chars().count();
            self.at_line_start = false;
        } else if self.think_col + w_len > self.avail_width {
            let clean = self.think_word_buf.trim_start();
            print!("\x1b[0m\n\x1b[2m│ \x1b[0m\x1b[2m{}", clean);
            self.think_col = clean.chars().count();
        } else {
            print!("{}", self.think_word_buf);
            self.think_col += w_len;
        }

        self.think_word_buf.clear();
    }

    /// Cleanly closes thinking gutter with proper blank line spacing
    pub fn close_thinking(&mut self) {
        self.flush_think_word();
        if self.thinking_started {
            if !self.at_line_start {
                print!("\x1b[0m\n");
            }
            println!();
            self.thinking_started = false;
            self.at_line_start = true;
            self.think_col = 0;
            let _ = io::stdout().flush();
        }
    }

    pub fn on_chunk(&mut self, chunk: &str, is_thinking: bool) {
        if chunk.is_empty() {
            return;
        }

        if self.first_token_time.is_none() {
            self.first_token_time = Some(Instant::now());
        }

        if is_thinking {
            self.thinking_chars += chunk.len();
            if !self.thinking_started {
                self.thinking_started = true;
                self.at_line_start = true;
                self.think_col = 0;
            }

            for ch in chunk.chars() {
                match ch {
                    '\r' => continue,
                    '\n' => {
                        self.flush_think_word();
                        if self.at_line_start {
                            print!("\x1b[2m│\x1b[0m\n");
                        } else {
                            print!("\x1b[0m\n");
                            self.at_line_start = true;
                            self.think_col = 0;
                        }
                    }
                    ' ' | '\t' => {
                        self.think_word_buf.push(ch);
                        self.flush_think_word();
                    }
                    _ => {
                        self.think_word_buf.push(ch);
                        if self.think_word_buf.len() >= self.avail_width {
                            self.flush_think_word();
                        }
                    }
                }
            }
            let _ = io::stdout().flush();
        } else {
            self.answer_chars += chunk.len();

            // Transition from thinking to answer: flush and create clean blank line
            if self.thinking_started && !self.answer_started {
                self.close_thinking();
                self.answer_started = true;
                self.at_line_start = true;
            }

            print!("\x1b[0m{}", chunk);
            let _ = io::stdout().flush();
        }
    }

    pub fn finish(&mut self, stats: &UsageStats, prompt_chars: usize) {
        self.close_thinking();

        let elapsed_gen = self
            .first_token_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.01)
            .max(0.01);

        print!("\x1b[0m\n\n");

        let in_tok = if stats.prompt_tokens > 0 {
            stats.prompt_tokens
        } else {
            (prompt_chars * 10) / 36
        };

        let out_tok = if stats.completion_tokens > 0 {
            stats.completion_tokens
        } else {
            ((self.thinking_chars + self.answer_chars) * 10) / 36
        };

        let r_tok = if stats.reasoning_tokens > 0 {
            stats.reasoning_tokens
        } else {
            (self.thinking_chars * 10) / 36
        };

        let mut parts = Vec::new();

        let in_s = if in_tok >= 1000 {
            format!("↑{:.1}k", in_tok as f64 / 1000.0)
        } else {
            format!("↑{}", in_tok)
        };

        let out_s = if out_tok >= 1000 {
            format!("↓{:.1}k", out_tok as f64 / 1000.0)
        } else {
            format!("↓{}", out_tok)
        };

        let mut tok_s = format!("{} {}", in_s, out_s);
        if r_tok > 0 {
            let r_s = if r_tok >= 1000 {
                format!("R{:.1}k", r_tok as f64 / 1000.0)
            } else {
                format!("R{}", r_tok)
            };
            tok_s.push_str(&format!(" {}", r_s));
        }
        parts.push(tok_s);

        if stats.cached_tokens > 0 && in_tok > 0 {
            let cch = ((stats.cached_tokens as f64 / in_tok as f64) * 100.0).round() as u32;
            parts.push(format!("CH{}%", cch));
        }

        let max_ctx = 8192;
        let ctx_used = in_tok + out_tok;
        let ctx_pct = (ctx_used as f64 / max_ctx as f64) * 100.0;
        parts.push(format!("{:.1}%/8.2k", ctx_pct));

        let speed = out_tok as f64 / elapsed_gen;
        parts.push(format!("{:.1}s @ {:.1} t/s", elapsed_gen, speed));

        let stat_line = parts.join(" · ");
        println!("\x1b[2m  {}\x1b[0m\n", stat_line);
        let _ = io::stdout().flush();
    }
}
