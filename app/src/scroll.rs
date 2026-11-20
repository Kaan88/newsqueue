//! Keep periodic UI work out of native scrollbar tracking and wheel bursts.
use std::{cell::RefCell, time::{Duration, Instant}};
use windows::Win32::{Foundation::*, UI::{Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::*}};

#[derive(Default)]
struct State { depth: usize, until: Option<Instant> }
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
const SUBCLASS: usize = 0x4e515343;

pub fn active() -> bool {
    STATE.with(|s| { let s=s.borrow(); s.depth>0 || s.until.is_some_and(|t|Instant::now()<t) })
}

pub fn install(window: HWND) {
    unsafe { assert!(SetWindowSubclass(window,Some(proc),SUBCLASS,0).as_bool()); }
}

unsafe extern "system" fn proc(h: HWND,m: u32,wp: WPARAM,lp: LPARAM,id: usize,_: usize)->LRESULT {
    let scrolling=matches!(m,WM_MOUSEWHEEL|WM_MOUSEHWHEEL|WM_VSCROLL|WM_HSCROLL)
        || (m==WM_NCLBUTTONDOWN && matches!(wp.0 as u32,HTVSCROLL|HTHSCROLL));
    if scrolling { STATE.with(|s|s.borrow_mut().depth+=1); }
    // A scrollbar click can run a nested modal loop here. Its timer messages
    // must not enter snapshot rebuilding or block on the engine mutex.
    let result=DefSubclassProc(h,m,wp,lp);
    if scrolling {
        STATE.with(|s|{let mut s=s.borrow_mut();s.depth-=1;s.until=Some(Instant::now()+Duration::from_millis(150));});
        crate::refresh_soon();
    }
    if m==WM_NCDESTROY { let _=RemoveWindowSubclass(h,Some(proc),id); }
    result
}
