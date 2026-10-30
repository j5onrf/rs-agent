use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct InlineSpinner {
    active: Arc<AtomicBool>,
    message: Arc<Mutex<String>>,
    handle: Option<thread::JoinHandle<()>>,
    start_time: Instant,
}

impl InlineSpinner {
    pub fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            message: Arc::new(Mutex::new(String::new())),
            handle: None,
            start_time: Instant::now(),
        }
    }

    pub fn start(&mut self, msg: &str) {
        self.stop();

        self.active.store(true, Ordering::SeqCst);
        *self.message.lock().unwrap() = msg.to_string();
        self.start_time = Instant::now();

        let active_clone = self.active.clone();
        let msg_clone = self.message.clone();
        let start = self.start_time;

        // Hide terminal cursor while spinning
        print!("\x1b[?25l");
        let _ = io::stdout().flush();

        let handle = thread::spawn(move || {
            let frames = ["● ○ ○", "○ ● ○", "○ ○ ●", "○ ● ○"];
            let mut idx = 0;

            while active_clone.load(Ordering::SeqCst) {
                let frame = frames[idx % frames.len()];
                let text = msg_clone.lock().unwrap().clone();
                let elapsed = start.elapsed().as_secs_f64();

                print!("\r\x1b[2K\x1b[1;36m{}\x1b[0m \x1b[2m{}\x1b[0m \x1b[2m{:.1}s\x1b[0m", frame, text, elapsed);
                let _ = io::stdout().flush();

                idx += 1;
                thread::sleep(Duration::from_millis(75));
            }

            // Wipe spinner residue cleanly
            print!("\r\x1b[2K\r\x1b[?25h");
            let _ = io::stdout().flush();
        });

        self.handle = Some(handle);
    }

    pub fn update(&self, msg: &str) {
        if let Ok(mut lock) = self.message.lock() {
            *lock = msg.to_string();
        }
    }

    pub fn stop(&mut self) {
        if self.active.load(Ordering::SeqCst) {
            self.active.store(false, Ordering::SeqCst);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            print!("\r\x1b[2K\r\x1b[?25h");
            let _ = io::stdout().flush();
        }
    }
}

impl Drop for InlineSpinner {
    fn drop(&mut self) {
        self.stop();
    }
}
