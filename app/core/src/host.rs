//! Runs an [`Engine`] on its own timer thread (GUI_PLAN Phase 0 step 4).
//!
//! The engine lives in a shared slot. A front end that needs direct access
//! for a frame calls [`EngineHost::checkout`], works on it through `Deref`,
//! then [`EngineHost::checkin`]. While it is checked in, the timer thread
//! ticks it, so downloads, timers and post-processing keep running whether
//! or not the front end draws frames. Without [`EngineHost::start`] the
//! engine simply stays local (tests, screenshot mode) and the front end
//! ticks it itself.

use crate::command::Command;
use crate::engine::Engine;
use crate::snapshot::{EngineHandle, Snapshot};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

pub const TICK_INTERVAL: Duration = Duration::from_millis(50);

pub struct EngineHost {
    local: Option<Engine>,
    shared: Arc<Mutex<Option<Engine>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pump: Arc<Pump>,
    sender: Sender<Command>,
}

/// Commands in, snapshots out; shared by the engine thread and the UI.
struct Pump {
    commands: Mutex<Receiver<Command>>,
    snapshot: Arc<RwLock<Snapshot>>,
    seq: AtomicU64,
}

impl Pump {
    /// Applies queued commands, optionally ticks, then publishes a snapshot.
    fn run(&self, engine: &mut Engine, tick: bool) {
        if let Ok(rx) = self.commands.lock() {
            while let Ok(command) = rx.try_recv() {
                engine.apply(command);
            }
        }
        if tick {
            engine.tick();
        }
        self.publish(engine);
    }

    fn publish(&self, engine: &Engine) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let snap = Snapshot::of(engine, seq);
        *self.snapshot.write().unwrap_or_else(|e| e.into_inner()) = snap;
    }
}

impl EngineHost {
    pub fn new(engine: Engine) -> Self {
        let (sender, rx) = channel();
        let pump = Arc::new(Pump {
            commands: Mutex::new(rx),
            snapshot: Arc::new(RwLock::new(Snapshot::of(&engine, 0))),
            seq: AtomicU64::new(0),
        });
        Self {
            local: Some(engine),
            shared: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            pump,
            sender,
        }
    }

    /// A handle for another front end (or thread): send commands, read snapshots.
    pub fn handle(&self) -> EngineHandle {
        EngineHandle {
            commands: self.sender.clone(),
            snapshot: Arc::clone(&self.pump.snapshot),
        }
    }

    /// Without a thread: run queued commands, tick, publish. The front end
    /// calls this instead of `Engine::tick` (screenshot mode, tests).
    pub fn tick_local(&mut self) {
        if let Some(engine) = self.local.as_mut() {
            self.pump.run(engine, true);
        }
    }

    /// Like [`tick_local`](Self::tick_local) without the engine tick: the
    /// sample data of screenshot mode stays as it is.
    pub fn pump_local(&mut self) {
        if let Some(engine) = self.local.as_mut() {
            self.pump.run(engine, false);
        }
    }

    /// Starts the timer thread. The engine stays checked out until the next
    /// [`checkin`](Self::checkin).
    pub fn start(&mut self, interval: Duration) {
        self.start_with(interval, true);
    }

    /// Like [`start`](Self::start); with `tick` false the thread only runs
    /// commands, so a showcase engine keeps its sample state.
    pub fn start_with(&mut self, interval: Duration, tick: bool) {
        if self.thread.is_some() {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let stop = Arc::clone(&self.stop);
        let pump = Arc::clone(&self.pump);
        self.thread = Some(
            std::thread::Builder::new()
                .name("engine".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if let Ok(mut slot) = shared.lock() {
                            if let Some(engine) = slot.as_mut() {
                                pump.run(engine, tick);
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
        // Commands sent from other handles also apply while the UI holds it.
        if let Some(engine) = self.local.as_mut() {
            self.pump.run(engine, false);
        }
    }

    /// Hands the engine back to the timer thread (no-op without one).
    pub fn checkin(&mut self) {
        if self.thread.is_some() {
            if let Some(engine) = self.local.take() {
                self.pump.publish(&engine);
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
    fn handle_sends_commands_and_reads_snapshots_across_threads() {
        let mut host = EngineHost::new(Engine::new(Settings::default(), true));
        let handle = host.handle();
        let first = handle.read(|s| s.queue.first().map(|r| r.id)).expect("demo queue");
        let rows = host.queue.len();
        host.start(Duration::from_millis(5));
        host.checkin();
        let remote = handle.clone();
        std::thread::spawn(move || assert!(remote.send(Command::Delete(first))))
            .join()
            .unwrap();
        std::thread::sleep(Duration::from_millis(80));
        let snap = handle.snapshot();
        assert!(snap.seq > 0);
        assert_eq!(snap.queue.len(), rows - 1);
        assert!(snap.queue.iter().all(|r| r.id != first));
        host.checkout();
        assert_eq!(host.queue.len(), rows - 1);
    }

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
