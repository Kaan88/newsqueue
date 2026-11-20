//! Passive, debug-build menu timing. No input injection or global hooks.
//! Buffers timings in memory and writes only after the menu closes.
#[cfg(debug_assertions)]
mod diagnostic {
    use std::{cell::RefCell, io::Write, time::Instant};
    use windows::Win32::{Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*};

    struct Trace {
        start: Instant,
        hook: HHOOK,
        entries: Vec<(u128, &'static str, usize)>,
    }
    thread_local! { static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) }; }

    fn record(event: &'static str, value: usize) {
        TRACE.with(|slot| {
            if let Some(trace) = slot.borrow_mut().as_mut() {
                if trace.entries.len() < 4096 {
                    trace.entries.push((trace.start.elapsed().as_micros(), event, value));
                }
            }
        });
    }

    unsafe extern "system" fn filter(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if code == MSGF_MENU as i32 && lp.0 != 0 {
            let msg = &*(lp.0 as *const MSG);
            if matches!(msg.message, WM_MOUSEMOVE | WM_NCMOUSEMOVE) {
                record("move", 0);
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }

    pub fn enter() {
        if TRACE.with(|slot| slot.borrow().is_some()) { return; }
        let hook = unsafe { SetWindowsHookExW(WH_MSGFILTER, Some(filter), None, GetCurrentThreadId()) }.unwrap_or_default();
        TRACE.with(|slot| *slot.borrow_mut() = Some(Trace {
            start: Instant::now(), hook, entries: Vec::with_capacity(4096),
        }));
        record("enter", 0);
    }

    pub fn selected(item: usize) { record("select", item); }

    pub fn exit() {
        record("exit", 0);
        let trace = TRACE.with(|slot| slot.borrow_mut().take());
        if let Some(trace) = trace {
            if !trace.hook.is_invalid() { unsafe { let _ = UnhookWindowsHookEx(trace.hook); } }
            // Only relative times and numeric menu state, never labels or paths.
            if let Ok(exe) = std::env::current_exe() {
                if let Some(parent) = exe.parent() {
                    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(parent.join("menu-hover.log")) {
                        let _ = writeln!(file, "menu session (microseconds)");
                        for (time, event, value) in trace.entries {
                            let _ = writeln!(file, "{time} {event} {value}");
                        }
                    }
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
pub use diagnostic::{enter, exit, selected};
#[cfg(not(debug_assertions))]
pub fn enter() {}
#[cfg(not(debug_assertions))]
pub fn exit() {}
#[cfg(not(debug_assertions))]
pub fn selected(_: usize) {}
