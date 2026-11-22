//! Background click worker. One thread per run; stopped via channel so even
//! hour-long intervals stop instantly (recv_timeout wakes on drop/send).

use crate::config::{ClickType, MouseButton, Settings};
use eframe::egui;
use enigo::{Button, Coordinate, Direction, Enigo, Mouse};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Instant;

pub fn new_enigo() -> Result<Enigo, String> {
    let settings = enigo::Settings { linux_delay: 0, ..Default::default() };
    Enigo::new(&settings).map_err(|e| e.to_string())
}

pub struct Clicker {
    running: Arc<AtomicBool>,
    stop_tx: Option<Sender<()>>,
}

impl Clicker {
    pub fn new() -> Self {
        Self { running: Arc::new(AtomicBool::new(false)), stop_tx: None }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn start(&mut self, s: &Settings, repaint: egui::Context) {
        if self.is_running() {
            return;
        }
        let (tx, rx) = mpsc::channel::<()>();
        self.stop_tx = Some(tx);
        self.running.store(true, Ordering::Release);

        let running = self.running.clone();
        let interval = s.interval();
        let button = match s.button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        let clicks_per_tick = if s.click_type == ClickType::Double { 2 } else { 1 };
        let limit = (!s.repeat_until_stopped).then_some(s.repeat_count.max(1));
        let pos = s.use_fixed_pos.then_some((s.x, s.y));

        std::thread::Builder::new()
            .name("clicker".into())
            .spawn(move || {
                if let Ok(mut enigo) = new_enigo() {
                    let mut done = 0u32;
                    let mut next = Instant::now();
                    loop {
                        if let Some((x, y)) = pos {
                            let _ = enigo.move_mouse(x, y, Coordinate::Abs);
                        }
                        for _ in 0..clicks_per_tick {
                            let _ = enigo.button(button, Direction::Click);
                        }
                        done += 1;
                        if limit.is_some_and(|l| done >= l) {
                            break;
                        }
                        // Fixed-deadline scheduling: no drift from click latency.
                        next += interval;
                        let now = Instant::now();
                        if next < now {
                            next = now; // fell behind, don't burst to catch up
                        }
                        match rx.recv_timeout(next - now) {
                            Err(RecvTimeoutError::Timeout) => continue,
                            _ => break, // stop requested or sender dropped
                        }
                    }
                }
                running.store(false, Ordering::Release);
                repaint.request_repaint();
            })
            .expect("spawn clicker thread");
    }

    pub fn stop(&mut self) {
        // Dropping the sender wakes the worker immediately.
        self.stop_tx = None;
    }
}
