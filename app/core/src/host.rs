//! Runs an [`Engine`] on its own timer thread (GUI_PLAN Phase 0 step 4).
//!
//! The engine lives in a shared slot. A front end that needs direct access
//! for a frame calls [`EngineHost::checkout`], works on it through `Deref`,
//! then [`EngineHost::checkin`]. While it is checked in, the timer thread
//! ticks it, so downloads, timers and post-processing keep running whether
//! or not the front end draws frames. Without [`EngineHost::start`] the
//! engine simply stays local (tests, screenshot mode) and the front end
//! ticks it itself.

use crate::engine::Engine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub const TICK_INTERVAL: Duration = Duration::from_millis(50);

pub struct EngineHost {
    local: Option<Engine>,
    shared: Arc<Mutex<Option<Engine>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EngineHost {
    pub fn new(engine: Engine) -> Self {
        Self {
            local: Some(engine),
            shared: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    /// Starts the timer thread. The engine stays checked out until the next
    /// [`checkin`](Self::checkin).
    pub fn start(&mut self, interval: Duration) {
        if self.thread.is_some() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let stop = Arc::clone(&self.stop);
        self.thread = Some(
            std::thread::Builder::new()
                .name("engine".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if let Ok(mut slot) = shared.lock() {
                            if let Some(engine) = slot.as_mut() {
                                engine.tick();
                            }
                        }
                        std::thread::sleep(interval);
                    }
                })
                .expect("spawn engine thread"),
        );
    }

    /// True when the timer thread ticks the engine; the front end must not.
    pub fn threaded(&self) -> bool {
        self.thread.is_some()
    }

    /// Takes the engine for direct use. Blocks only while a tick runs.
    pub fn checkout(&mut self) {
        if self.local.is_none() {
            let mut slot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            self.local = slot.take();
        }
    }

    /// Hands the engine back to the timer thread (no-op without one).
    pub fn checkin(&mut self) {
        if self.thread.is_some() {
            if let Some(engine) = self.local.take() {
                let mut slot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                *slot = Some(engine);
            }
        }
    }

    /// Runs `f` on the engine wherever it currently is.
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Engine) -> R) -> R {
        let was_local = self.local.is_some();
        self.checkout();
        let r = f(self.local.as_mut().expect("engine present"));
        if !was_local {
            self.checkin();
        }
        r
    }
}

impl std::ops::Deref for EngineHost {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        self.local
            .as_ref()
            .expect("engine is checked in; call EngineHost::checkout first")
    }
}

impl std::ops::DerefMut for EngineHost {
    fn deref_mut(&mut self) -> &mut Engine {
        self.local
            .as_mut()
            .expect("engine is checked in; call EngineHost::checkout first")
    }
}

impl Drop for EngineHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;

    #[test]
    fn engine_ticks_on_its_thread_while_checked_in() {
        let mut host = EngineHost::new(Engine::new(Settings::default(), true));
        host.start(Duration::from_millis(5));
        assert!(host.threaded());
        let before = host.last_tick;
        host.checkin();
        std::thread::sleep(Duration::from_millis(80));
        host.checkout();
        assert!(host.last_tick > before, "the engine thread did not tick");
        // Checked out: the thread must leave it alone.
        let held = host.last_tick;
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(host.last_tick, held);
        let n = host.with(|e| e.queue.len());
        assert_eq!(n, host.queue.len());
    }
}
