use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct InlineSpinner {
    running: Arc<AtomicBool>,
    message: Arc<Mutex<String>>,
    handle: Option<JoinHandle<()>>,
}

impl Default for InlineSpinner {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineSpinner {
    pub fn new() -> Self {
        Self { running: Arc::new(AtomicBool::new(false)), message: Arc::new(Mutex::new(String::new())), handle: None }
    }

    pub fn start(&mut self, msg: &str) {
        self.stop();

        // Finding 2 Fix: Gate interactive ticker exclusively to TTY devices
        if !io::stdout().is_terminal() {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        if let Ok(mut m) = self.message.lock() {
            m.clear();
            m.push_str(msg);
        }

        let running = Arc::clone(&self.running);
        let message = Arc::clone(&self.message);

        let handle = thread::Builder::new()
            .name("inline-spinner".into())
            .spawn(move || {
                let start_time = Instant::now();
                while running.load(Ordering::Relaxed) {
                    let elapsed = start_time.elapsed().as_secs_f64();

                    // Finding 1 Fix: Scope stdout lock tightly so it drops before thread::sleep
                    {
                        let stdout = io::stdout();
                        let mut lock = stdout.lock();
                        if let Ok(m) = message.lock() {
                            let _ = write!(lock, "\r\x1b[2K  {} \x1b[2m{:.1}s\x1b[0m", &*m, elapsed);
                        }
                        let _ = lock.flush();
                    }

                    thread::sleep(Duration::from_millis(100));
                }
            })
            .ok();

        self.handle = handle;
    }

    pub fn update(&self, msg: &str) {
        if let Ok(mut m) = self.message.lock() {
            m.clear();
            m.push_str(msg);
        }
    }

    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            if io::stdout().is_terminal() {
                let stdout = io::stdout();
                let mut lock = stdout.lock();
                let _ = write!(lock, "\r\x1b[2K");
                let _ = lock.flush();
            }
        }
    }
}

impl Drop for InlineSpinner {
    fn drop(&mut self) {
        self.stop();
    }
}
