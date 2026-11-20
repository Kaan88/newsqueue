//! Repeatable off-screen visual matrix; uses only the in-memory demo engine.
use super::*;
use serde_json::{json, Value};
use std::path::Path;

fn measure(window: HWND) -> Value {
    unsafe extern "system" fn each(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        if !IsWindowVisible(h).as_bool() { return TRUE; }
        let rows = &mut *(lp.0 as *mut Vec<Value>);
        let parent = GetParent(h).unwrap_or_default();
        let mut class = [0u16; 128];
        let len = GetClassNameW(h, &mut class);
        let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
        let mut pc = [0u16; 128];
        let n = GetClassNameW(parent, &mut pc);
        let parent_class = String::from_utf16_lossy(&pc[..n.max(0) as usize]);
        if !matches!(parent_class.as_str(), "NQDlg" | "NQPanel" | "NQTabPanel" | "NewsQueueMain" | "NewsQueuePage") { return TRUE; }
        let mut r = RECT::default();
        let mut pr = RECT::default();
        let _ = GetWindowRect(h, &mut r);
        let _ = GetClientRect(parent, &mut pr);
        let mut origin = POINT {x: r.left, y: r.top};
        let _ = ScreenToClient(parent, &mut origin);
        let width = r.right-r.left;
        let height = r.bottom-r.top;
        let text = form::text(h);
        let mut issues = vec![];
        if origin.x < -1 || origin.y < -1 || origin.x+width > pr.right+1 || origin.y+height > pr.bottom+1 {
            issues.push("outside-parent");
        }
        let style = GetWindowLongW(h, GWL_STYLE) as u32;
        let mut needed = 0;
        if class == "Static" && style & 0x1f == 0 && !text.is_empty() {
            let dc = GetDC(Some(h));
            let old = SelectObject(dc, HGDIOBJ(send(h, WM_GETFONT, 0, 0) as *mut _));
            let mut z = wz(&text); z.pop();
            let mut tr = RECT {right: width, ..Default::default()};
            DrawTextW(dc, &mut z, &mut tr, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
            needed = tr.bottom;
            if needed > height+1 { issues.push("text-height"); }
            SelectObject(dc, old);
            ReleaseDC(Some(h), dc);
        }
        rows.push(json!({"class":class,"id":GetDlgCtrlID(h),"text":text,
            "bounds":[origin.x,origin.y,width,height],"text_height":needed,"issues":issues}));
        TRUE
    }
    let mut rows = Vec::<Value>::new();
    unsafe { let _ = EnumChildWindows(Some(window), Some(each), LPARAM(&mut rows as *mut _ as isize)); }
    json!(rows)
}

fn shot(dir: &Path, name: &str, window: HWND, records: &mut Vec<Value>) {
    show_offscreen(window);
    let path = dir.join(format!("{name}.png"));
    capture(window, path.to_str().unwrap());
    assert!(path.exists(), "missing capture {name}");
    records.push(json!({"name":name,"controls":measure(window)}));
}

fn reset(state: &str, connections: bool) {
    let mut e = demo_engine();
    e.settings.thread_list_visible = connections;
    if state == "empty" { e.queue.clear(); }
    if state == "long" {
        for c in &mut e.queue {
            c.subject = format!("Very.Long.Collection.Name.İstanbul.日本語.2026.{}", c.subject.repeat(3));
            c.expanded = true;
        }
    }
    let mut host = EngineHost::new(e);
    host.pump_local();
    let handle = host.handle();
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
    ui(|u| { u.handle = Some(handle); u.seq = 0; u.sel = None; });
    refresh();
}

pub fn run(main: HWND, path: &str) {
    let dir = Path::new(path);
    std::fs::create_dir_all(dir).unwrap();
    let mut records = vec![];
    unsafe { let _ = KillTimer(Some(main), TIMER_REFRESH); }
    for state in ["normal", "empty", "long"] {
        for connections in [true, false] {
            reset(state, connections);
            for (width,height) in [(760,480),(1024,700),(1280,820)] {
                unsafe { SetWindowPos(main,None,0,0,s(width),s(height),SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE).unwrap(); }
                layout(main);
                for tab in 0..4 {
                    select_tab(tab);
                    shot(dir,&format!("main-{state}-connections-{connections}-{width}-{height}-tab-{tab}"),main,&mut records);
                }
            }
        }
    }
    reset("normal", true);
    for page in 0..15 {
        let d = setup::build(main,page);
        shot(dir,&format!("setup-{page:02}"),d,&mut records);
        unsafe { DestroyWindow(d).unwrap(); }
    }
    for name in ["server","stats","notes","about","columns","pause","speed","version","shutdown","unrar-password","rename","execute","rss-feeds"] {
        let d = dialogs::demo(name,main).unwrap();
        shot(dir,name,d,&mut records);
        unsafe { DestroyWindow(d).unwrap(); }
    }
    for index in 0..21 {
        let (name,d)=dialogs::audit_variant(main,index).unwrap();
        shot(dir,&name,d,&mut records);
        unsafe { DestroyWindow(d).unwrap(); }
    }
    for existing in [false,true] {
        for ssl in [false,true] {
            for auth in [false,true] {
                let mut server = with_engine(|e| e.settings.servers[0].clone());
                server.ssl=ssl; server.auth=auth;
                server.port=if ssl {563} else {119};
                let d=setup::build_server(main,&server,existing);
                shot(dir,&format!("server-edit-{existing}-ssl-{ssl}-auth-{auth}"),d,&mut records);
                unsafe { DestroyWindow(d).unwrap(); }
            }
        }
    }
    std::fs::write(dir.join("controls.json"),serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    println!("PASS: {} hidden audit captures",records.len());
    unsafe { DestroyWindow(main).unwrap(); }
}

/// Exercise actual common-control scrolling using messages to our off-screen
/// test windows only. No global input, cursor movement, capture or focus.
pub fn scroll_smoke(main: HWND) {
    assert!(ui(|u|u.demo));
    unsafe {let _=KillTimer(Some(main),TIMER_REFRESH);}
    show_offscreen(main);
    with_engine(|e| {
        let collection=e.queue[1].clone();
        e.queue=(0..500).map(|i|{let mut c=collection.clone();c.id=1000+i;c.expanded=false;c.subject=format!("Scroll fixture {i:04}");c}).collect();
        let thread=e.threads[0].clone();e.threads=vec![thread;500];
        let par=e.par2[0].clone();e.par2=vec![par;500];
        let rss=e.rss[0].clone();e.rss=vec![rss;500];
        let line=e.logs[0].clone();
        e.logs=(0..150).map(|i|{let mut l=line.clone();l.message=format!("Log fixture {i:04}");l}).collect();
    });
    ui(|u|u.seq=0);refresh();layout(main);
    let lists=ui(|u|[u.queue,u.conns,u.par2,u.log,u.rss]);
    let mut slowest=Duration::ZERO;
    for (index,list) in lists.into_iter().enumerate() {
        select_tab(match index {0|1=>0,2=>1,3=>2,_=>3});
        send(list,LVM_ENSUREVISIBLE,0,0);
        let mut previous=send(list,LVM_GETTOPINDEX,0,0);
        for _ in 0..12 {
            let begin=Instant::now();
            send(list,WM_MOUSEWHEEL,(-120i16 as u16 as usize)<<16,0);
            slowest=slowest.max(begin.elapsed());
            let top=send(list,LVM_GETTOPINDEX,0,0);
            assert!(top>=previous,"list {index} scroll moved backwards");previous=top;
            assert!(scroll::active());
            let seq=ui(|u|{let seq=u.seq;u.seq=0;seq});
            HOST.with(|host|{let _borrow=host.borrow_mut();send(main,WM_TIMER,TIMER_REFRESH,0);});
            assert_eq!(ui(|u|u.seq),0,"scroll timer accessed engine");
            ui(|u|u.seq=seq);
        }
        assert!(previous>0,"list {index} did not scroll");
        pump(180);
        let before=send(list,LVM_GETTOPINDEX,0,0);
        refresh();
        assert_eq!(send(list,LVM_GETTOPINDEX,0,0),before,"refresh moved list {index}");
        send(list,WM_VSCROLL,SB_LINEDOWN.0 as usize,0);
        assert!(scroll::active());
        pump(180);
    }
    select_tab(0);
    let queue=lists[0];form::lv_select(queue,0);
    send(queue,LVM_SCROLL,0,s(1500) as isize);
    let before=send(queue,LVM_GETTOPINDEX,0,0);
    assert!(before>0);
    with_engine(|e|e.queue.swap(0,1));refresh();
    assert_eq!(form::lv_selected(queue),Some(1));
    assert_eq!(send(queue,LVM_GETTOPINDEX,0,0),before,"selection restore snapped viewport");
    let x=unsafe {GetScrollPos(queue,SB_HORZ)};
    send(queue,WM_HSCROLL,SB_LINERIGHT.0 as usize,0);
    assert!(unsafe {GetScrollPos(queue,SB_HORZ)}>x);
    pump(180);let x=unsafe {GetScrollPos(queue,SB_HORZ)};refresh();
    assert_eq!(unsafe {GetScrollPos(queue,SB_HORZ)},x);
    select_tab(2);let log=lists[3];
    send(log,LVM_ENSUREVISIBLE,0,0);
    let before=send(log,LVM_GETTOPINDEX,0,0);
    with_engine(|e|{let mut line=e.logs[0].clone();line.message="New entry while reading earlier entries".into();e.logs.push_back(line);});
    refresh();assert_eq!(send(log,LVM_GETTOPINDEX,0,0),before,"log append snapped to bottom");
    let count=send(log,LVM_GETITEMCOUNT,0,0);send(log,LVM_ENSUREVISIBLE,(count-1) as usize,0);
    with_engine(|e|{let mut line=e.logs[0].clone();line.message="Follow latest entry".into();e.logs.push_back(line);});
    refresh();
    assert!(send(log,LVM_GETTOPINDEX,0,0)+send(log,LVM_GETCOUNTPERPAGE,0,0)>=send(log,LVM_GETITEMCOUNT,0,0)-1,"log tail stopped following");
    println!("PASS: five native lists, 60 wheel steps, vertical/horizontal scrolling, timer isolation, queue-selection viewport, log reading/tail-follow; slowest wheel dispatch {:.2} ms",slowest.as_secs_f64()*1000.0);
    unsafe {DestroyWindow(main).unwrap();}
}
