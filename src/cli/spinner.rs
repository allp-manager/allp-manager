use std::{
    io::{self, IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub fn start(message: impl Into<String>, enabled: bool) -> Self {
        if !enabled || !io::stderr().is_terminal() {
            return Self {
                running: Arc::new(AtomicBool::new(false)),
                handle: None,
            };
        }

        let message = message.into();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let handle = thread::spawn(move || {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut index = 0usize;
            while thread_running.load(Ordering::Relaxed) {
                eprint!("\r{} {}", frames[index % frames.len()], message);
                let _ = io::stderr().flush();
                index += 1;
                thread::sleep(Duration::from_millis(80));
            }
            eprint!("\r\x1b[2K");
            let _ = io::stderr().flush();
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.finish();
    }
}
