#![windows_subsystem = "windows"]

// Share the real NewsQueue icon painter, font and Win32 window helpers.
#[path = "../../src/util.rs"]
mod util;

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;
use util::*;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const MAIN: PCWSTR = w!("NewsQueueNativeMockup");
const SETUP: PCWSTR = w!("NewsQueueNativeMockupSetup");
const PANEL: PCWSTR = w!("NewsQueueNativeMockupPanel");
const CONNECT: usize = 101;
const PAUSE: usize = 102;
const NZB: usize = 104;
const CLEANUP: usize = 106;
const OPTIONS: usize = 109;
const EXIT: usize = 115;
const FILTER: usize = 201;
const TAB: usize = 202;
const QUEUE: usize = 203;
const VIEWS: usize = 301;
const COMPACT: usize = 302;
const CONNECTIONS: usize = 303;
const COLLAPSE: usize = 410;
const EXPAND: usize = 411;
const ADD_DEMO: usize = 412;
const SETTINGS_LIST: usize = 601;
const APPLY: usize = 602;

#[derive(Clone, Copy, Default)]
struct Ui {
    toolbar: HWND,
    tab: HWND,
    status: HWND,
    action_band: HWND,
    actions: HWND,
    filter_label: HWND,
    filter: HWND,
    lists: [HWND; 4],
    page_actions: [[HWND; 4]; 3],
    connections: HWND,
}

thread_local! {
    static UI: Cell<Ui> = Cell::new(Ui::default());
    static COMPACT_MODE: Cell<bool> = const { Cell::new(false) };
    static CONNS: Cell<bool> = const { Cell::new(true) };
    static CONNECTED: Cell<bool> = const { Cell::new(true) };
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static EXPANDED: Cell<bool> = const { Cell::new(true) };
    static CLEANED: Cell<bool> = const { Cell::new(false) };
    static IMPORTED: Cell<bool> = const { Cell::new(false) };
    static DIALOG: Cell<HWND> = Cell::new(HWND::default());
}

type Tool = (usize, &'static str, &'static str);
type Column = (&'static str, i32, bool);

const MAIN_TOOLS: &[Tool] = &[
    (CONNECT, "connect", "Connect"),
    (0, "", ""),
    (PAUSE, "pause", "Pause"),
    (103, "pausefor", "Pause for"),
    (0, "", ""),
    (NZB, "nzb", "NZB"),
    (0, "", ""),
    (105, "download", "Download"),
    (0, "", ""),
    (CLEANUP, "cleanup", "Cleanup"),
    (0, "", ""),
    (107, "scheduler", "Scheduler"),
    (0, "", ""),
    (108, "shutdown", "Shutdown"),
    (0, "", ""),
    (OPTIONS, "setup", "Setup"),
    (0, "", ""),
    (110, "home", "Home"),
    (111, "version", "Version?"),
    (112, "about", "About"),
    (113, "notes", "Notes"),
    (114, "stats", "Stats"),
    (116, "tray", "To tray"),
];
const QUEUE_TOOLS: &[Tool] = &[
    (401, "play", "Resume"),
    (402, "pause_s", "Pause"),
    (0, "", ""),
    (403, "top", "Move to top"),
    (404, "up", "Move up"),
    (405, "down", "Move down"),
    (406, "bottom", "Move to bottom"),
    (0, "", ""),
    (COLLAPSE, "collapse", "Collapse all"),
    (EXPAND, "expand", "Expand all"),
    (0, "", ""),
    (407, "unrar", "Unpack"),
    (408, "nounrar", "Do not unpack"),
    (409, "folder", "Change unpack folder"),
    (0, "", ""),
    (413, "rename", "Rename"),
    (414, "export", "Export"),
    (415, "folder", "Open folder"),
    (0, "", ""),
    (416, "key", "Archive password"),
    (417, "eta", "Estimated time left"),
];

const QUEUE_COLUMNS: &[Column] = &[
    ("Subject", 342, false),
    ("Size", 88, true),
    ("Parts", 80, true),
    ("Age", 66, false),
    ("Group", 153, false),
    ("Poster", 128, false),
    ("Status", 123, false),
    ("Download folder", 205, false),
    ("Unrar", 56, false),
    ("Execute", 75, false),
    ("Delete archives", 110, false),
    ("Posted on", 112, false),
];

fn child(parent: HWND, class: &str, title: &str, style: u32, ex: u32, id: usize) -> HWND {
    mkpx(
        class,
        title,
        style | WS_CHILD.0,
        ex,
        0,
        0,
        1,
        1,
        Some(parent),
        id as i32,
    )
}

fn button(parent: HWND, title: &str, id: usize) -> HWND {
    child(
        parent,
        "BUTTON",
        title,
        sty!(WS_TABSTOP, BS_PUSHBUTTON),
        0,
        id,
    )
}

fn move_to(h: HWND, x: i32, y: i32, w: i32, hgt: i32) {
    if !h.is_invalid() {
        unsafe {
            let _ = MoveWindow(h, x, y, w.max(0), hgt.max(0), true);
        }
    }
}

fn show(h: HWND, yes: bool) {
    if !h.is_invalid() {
        unsafe {
            let _ = ShowWindow(h, if yes { SW_SHOWNA } else { SW_HIDE });
        }
    }
}

fn text(h: HWND, value: &str) {
    let value = wz(value);
    unsafe {
        let _ = SetWindowTextW(h, PCWSTR(value.as_ptr()));
    }
}

fn toolbar(owner: HWND, tools: &[Tool], main: bool) -> HWND {
    let h = child(
        owner,
        "ToolbarWindow32",
        "",
        sty!(
            WS_CLIPSIBLINGS,
            TBSTYLE_FLAT,
            TBSTYLE_TOOLTIPS,
            CCS_NORESIZE,
            CCS_NOPARENTALIGN
        ) | if main { 0 } else { CCS_NODIVIDER as u32 },
        0,
        0,
    );
    send(h, TB_BUTTONSTRUCTSIZE, size_of::<TBBUTTON>(), 0);
    let labels: Vec<_> = tools.iter().filter(|t| t.0 != 0).map(|t| t.1).collect();
    send(
        h,
        TB_SETIMAGELIST,
        0,
        imagelist(s(if main { 30 } else { 16 }), &labels),
    );
    send(h, TB_SETMAXTEXTROWS, usize::from(main), 0);
    send(h, TB_SETEXTENDEDSTYLE, 0, TBSTYLE_EX_DOUBLEBUFFER as isize);
    let mut index = 0;
    let mut strings = Vec::new();
    let mut buttons = Vec::new();
    for &(id, _, label) in tools {
        let mut b = TBBUTTON::default();
        if id == 0 {
            b.fsStyle = BTNS_SEP as u8;
            b.iBitmap = s(8);
        } else {
            b.iBitmap = index;
            index += 1;
            b.idCommand = id as i32;
            let enabled = matches!(
                id,
                CONNECT
                    | PAUSE
                    | NZB
                    | CLEANUP
                    | OPTIONS
                    | 112
                    | 116
                    | 401
                    | 402
                    | COLLAPSE
                    | EXPAND
            );
            b.fsState = if enabled { TBSTATE_ENABLED as u8 } else { 0 };
            b.fsStyle = if matches!(id, CONNECT | PAUSE) {
                BTNS_CHECK
            } else {
                BTNS_BUTTON
            } as u8;
            if main {
                b.fsStyle |= BTNS_AUTOSIZE as u8;
            }
            let mut label = wz(label);
            label.push(0);
            b.iString = send(h, TB_ADDSTRINGW, 0, label.as_ptr() as isize);
            strings.push(label);
        }
        buttons.push(b);
    }
    send(h, TB_ADDBUTTONSW, buttons.len(), buttons.as_ptr() as isize);
    send(
        h,
        TB_SETBUTTONSIZE,
        0,
        if main {
            packed(s(55), s(if COMPACT_MODE.get() { 46 } else { 54 }))
        } else {
            packed(s(23), s(22))
        },
    );
    h
}

fn packed(x: i32, y: i32) -> isize {
    ((y as isize) << 16) | x as isize
}

fn list(owner: HWND, id: usize, columns: &[Column], extra: u32) -> HWND {
    let h = child(
        owner,
        "SysListView32",
        "",
        sty!(
            WS_TABSTOP,
            WS_CLIPSIBLINGS,
            LVS_REPORT,
            LVS_SHOWSELALWAYS,
            LVS_SINGLESEL,
            LVS_NOSORTHEADER
        ),
        WS_EX_CLIENTEDGE.0,
        id,
    );
    send(
        h,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        0,
        (LVS_EX_FULLROWSELECT
            | LVS_EX_DOUBLEBUFFER
            | extra
            | if COMPACT_MODE.get() {
                LVS_EX_GRIDLINES
            } else {
                0
            }) as isize,
    );
    for (i, &(name, width, right)) in columns.iter().enumerate() {
        let mut name = wz(name);
        let c = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_FMT,
            fmt: if right { LVCFMT_RIGHT } else { LVCFMT_LEFT },
            cx: s(width),
            pszText: PWSTR(name.as_mut_ptr()),
            ..Default::default()
        };
        send(h, LVM_INSERTCOLUMNW, i, &c as *const _ as isize);
    }
    h
}

fn row(h: HWND, cells: &[&str], icon: Option<i32>, indent: i32) {
    let progress = if icon.is_some() {
        match cells.get(6).copied() {
            Some("Downloading 42%") => 42,
            Some("Downloading 62%") => 62,
            Some("Paused 27%") => -27,
            Some("Paused 42%") => -42,
            Some("Paused 62%") => -62,
            _ => 0,
        }
    } else {
        0
    };
    let mut item = LVITEMW {
        mask: LVIF_TEXT | LVIF_PARAM,
        iItem: send(h, LVM_GETITEMCOUNT, 0, 0) as i32,
        lParam: LPARAM(progress),
        ..Default::default()
    };
    if let Some(icon) = icon {
        item.mask |= LVIF_IMAGE | LVIF_INDENT;
        item.iImage = icon;
        item.iIndent = indent;
    }
    for (i, value) in cells.iter().enumerate() {
        let mut value = wz(value);
        item.iSubItem = i as i32;
        item.pszText = PWSTR(value.as_mut_ptr());
        if i == 0 {
            send(h, LVM_INSERTITEMW, 0, &item as *const _ as isize);
        } else {
            send(
                h,
                LVM_SETITEMTEXTW,
                item.iItem as usize,
                &item as *const _ as isize,
            );
        }
    }
}

fn populate_queue(ui: Ui) {
    let h = ui.lists[0];
    let mut filter = [0; 256];
    let n = unsafe { GetWindowTextW(ui.filter, &mut filter) };
    let filter = String::from_utf16_lossy(&filter[..n as usize]).to_lowercase();
    let active = if PAUSED.get() {
        "Paused 42%"
    } else if !CONNECTED.get() {
        "Waiting"
    } else {
        "Downloading 42%"
    };
    let families = [
        (
            "Open Skies – Episode 04",
            "3.62 GB",
            "2,821",
            "2 d",
            "alt.binaries.hdtv",
            "Skyline",
            active,
            r"D:\Downloads\TV",
            "Yes",
        ),
        (
            "City Walks – Copenhagen",
            "1.48 GB",
            "1,104",
            "5 d",
            "alt.binaries.video",
            "FieldNotes",
            "Queued",
            r"D:\Downloads\Video",
            "Yes",
        ),
        (
            "Night Signals – Live Session",
            "674 MB",
            "509",
            "16 d",
            "alt.binaries.sounds",
            "Wavelength",
            "Paused 27%",
            r"D:\Downloads\Music",
            "Yes",
        ),
        (
            "Public Domain Shorts – Vol. 2",
            "512 MB",
            "398",
            "21 d",
            "alt.binaries.video",
            "Archive",
            "Completed",
            r"D:\Downloads\Video",
            "No",
        ),
    ];
    send(h, WM_SETREDRAW, 0, 0);
    send(h, LVM_DELETEALLITEMS, 0, 0);
    for (i, &(subject, size, parts, age, group, poster, state, path, unpack)) in
        families.iter().enumerate()
    {
        if (i == 3 && CLEANED.get()) || !subject.to_lowercase().contains(&filter) {
            continue;
        }
        let expanded = i == 0 && EXPANDED.get();
        row(
            h,
            &[
                subject,
                size,
                parts,
                age,
                group,
                poster,
                state,
                path,
                unpack,
                "No",
                "No",
                "2026-11-18",
            ],
            Some(i32::from(expanded)),
            0,
        );
        if expanded {
            for (file, size, parts, state) in [
                (
                    "open-skies.s01e04.part01.rar",
                    "476 MB",
                    "368/368",
                    "Completed",
                ),
                (
                    "open-skies.s01e04.part02.rar",
                    "476 MB",
                    "368/368",
                    "Completed",
                ),
                (
                    "open-skies.s01e04.part03.rar",
                    "476 MB",
                    "229/368",
                    if PAUSED.get() {
                        "Paused 62%"
                    } else if CONNECTED.get() {
                        "Downloading 62%"
                    } else {
                        "Waiting"
                    },
                ),
                ("open-skies.s01e04.part04.rar", "476 MB", "0/368", "Queued"),
                ("open-skies.s01e04.par2", "1.24 MB", "2/2", "Completed"),
            ] {
                row(
                    h,
                    &[file, size, parts, "2 d", group, poster, state],
                    Some(2),
                    1,
                );
            }
        }
    }
    if IMPORTED.get() && "sample collection".contains(&filter) {
        row(
            h,
            &[
                "Sample Collection",
                "238 MB",
                "185",
                "1 d",
                "alt.binaries.test",
                "Demo",
                "Queued",
                r"D:\Downloads",
                "Yes",
            ],
            Some(0),
            0,
        );
    }
    send(h, WM_SETREDRAW, 1, 0);
    unsafe {
        let _ = InvalidateRect(Some(h), None, true);
    }
}

fn status(ui: Ui) {
    let connected = CONNECTED.get();
    let running = connected && !PAUSED.get();
    send(ui.toolbar, TB_CHECKBUTTON, CONNECT, isize::from(connected));
    send(ui.toolbar, TB_CHECKBUTTON, PAUSE, isize::from(PAUSED.get()));
    let values = [
        if connected {
            "Connected"
        } else {
            "Disconnected"
        },
        if running { "18.4 MB/s" } else { "0.0 MB/s" },
        if PAUSED.get() {
            "Paused"
        } else if connected {
            "Download active"
        } else {
            "Idle"
        },
        "5.82 GB queued",
        "9.70 GB session",
        "24.1 GB today",
        "D: 621 GB free",
        "Demo data",
    ];
    for (i, value) in values.iter().enumerate() {
        send(ui.status, SB_SETTEXTW, i, wz(value).as_ptr() as isize);
    }
    send(ui.connections, LVM_DELETEALLITEMS, 0, 0);
    for (number, part, stage) in [
        ("1", "open-skies.s01e04.part03.rar", "Receiving"),
        ("2", "open-skies.s01e04.part03.rar", "Receiving"),
        ("3", "open-skies.s01e04.part03.rar", "Waiting"),
        ("4", "open-skies.s01e04.part04.rar", "Waiting"),
    ] {
        row(
            ui.connections,
            &[
                number,
                if running { "Download" } else { "Idle" },
                if connected { part } else { "" },
                if running { stage } else { "Idle" },
                "Example Usenet",
            ],
            None,
            0,
        );
    }
}

unsafe fn menu(parent: HWND) {
    let bar = CreateMenu().unwrap();
    let menus: &[(&str, &[(usize, &str)])] = &[
        (
            "&File",
            &[(NZB, "&Import NZB…\tCtrl+O"), (0, ""), (EXIT, "E&xit")],
        ),
        (
            "&Queue",
            &[
                (401, "&Resume"),
                (402, "&Pause"),
                (CLEANUP, "&Cleanup"),
                (0, ""),
                (COLLAPSE, "Collapse all"),
                (EXPAND, "Expand all"),
            ],
        ),
        (
            "&View",
            &[
                (CONNECTIONS, "Show &connections\tF9"),
                (0, ""),
                (VIEWS, "&Roomy"),
                (COMPACT, "&Compact"),
            ],
        ),
        (
            "&Tools",
            &[(CONNECT, "&Connect / disconnect"), (OPTIONS, "Set&up…")],
        ),
        ("&Help", &[(112, "&About NewsQueue")]),
    ];
    for (name, entries) in menus {
        let popup = CreatePopupMenu().unwrap();
        for &(id, label) in *entries {
            let value = wz(label);
            let flags = if id == 0 {
                MF_SEPARATOR
            } else if (id == COMPACT && COMPACT_MODE.get())
                || (id == VIEWS && !COMPACT_MODE.get())
                || id == CONNECTIONS
            {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            };
            AppendMenuW(popup, flags, id, PCWSTR(value.as_ptr())).unwrap();
        }
        AppendMenuW(bar, MF_POPUP, popup.0 as usize, PCWSTR(wz(name).as_ptr())).unwrap();
    }
    SetMenu(parent, Some(bar)).unwrap();
}

fn build(parent: HWND) {
    unsafe {
        menu(parent);
    }
    let toolbar = toolbar(parent, MAIN_TOOLS, true);
    let tab = child(
        parent,
        "SysTabControl32",
        "",
        sty!(WS_CLIPSIBLINGS, TCS_FOCUSNEVER),
        0,
        TAB,
    );
    send(
        tab,
        TCM_SETIMAGELIST,
        0,
        imagelist(s(16), &["queue", "shield", "page", "rss"]),
    );
    for (index, label) in ["Download queue", "PAR2", "Log", "RSS"].iter().enumerate() {
        let mut label = wz(label);
        let data = TCITEMW {
            mask: TCIF_TEXT | TCIF_IMAGE,
            pszText: PWSTR(label.as_mut_ptr()),
            iImage: index as i32,
            ..Default::default()
        };
        send(tab, TCM_INSERTITEMW, index, &data as *const _ as isize);
    }
    // A single native surface owns the command row. Previously these controls
    // were siblings over the themed tab pane: the toolbar and STATIC label
    // painted button-face rectangles against the tab's lighter background.
    let action_band = child(
        parent,
        "NewsQueueNativeMockupPanel",
        "",
        sty!(WS_CLIPCHILDREN, WS_CLIPSIBLINGS),
        WS_EX_CONTROLPARENT.0,
        204,
    );
    let actions = self::toolbar(action_band, QUEUE_TOOLS, false);
    let filter_label = child(action_band, "STATIC", "&Filter", 2 /* SS_RIGHT */, 0, 0);
    let filter = child(
        action_band,
        "EDIT",
        "",
        sty!(WS_TABSTOP, ES_AUTOHSCROLL),
        WS_EX_CLIENTEDGE.0,
        FILTER,
    );
    send(
        filter,
        EM_SETCUEBANNER,
        0,
        w!("Filter subjects").as_ptr() as isize,
    );
    let queue = list(parent, QUEUE, QUEUE_COLUMNS, LVS_EX_HEADERDRAGDROP);
    send(
        queue,
        LVM_SETIMAGELIST,
        LVSIL_SMALL as usize,
        imagelist(s(16), &["plus", "minus", "file"]),
    );
    let par2 = list(
        parent,
        205,
        &[
            ("Name", 372, false),
            ("Size", 120, true),
            ("Blocks", 80, true),
            ("Info", 225, false),
            ("Unrar", 90, false),
            ("Status", 150, false),
        ],
        0,
    );
    row(
        par2,
        &[
            "Public Domain Shorts – Vol. 2",
            "3.88 MB",
            "67",
            "All files present",
            "No",
            "Verified",
        ],
        None,
        0,
    );
    let log = list(
        parent,
        206,
        &[
            ("Time", 92, false),
            ("Area", 125, false),
            ("Message", 900, false),
        ],
        0,
    );
    row(
        log,
        &["14:27:12", "Download", "Started Open Skies – Episode 04."],
        None,
        0,
    );
    row(
        log,
        &[
            "14:26:58",
            "PAR2",
            "Verified Public Domain Shorts – Vol. 2.",
        ],
        None,
        0,
    );
    row(
        log,
        &[
            "14:26:41",
            "Connection",
            "Connected to Example Usenet (4 connections).",
        ],
        None,
        0,
    );
    let rss = list(
        parent,
        207,
        &[
            ("Name", 440, false),
            ("Date", 150, false),
            ("Description", 570, false),
        ],
        0,
    );
    row(
        rss,
        &[
            "City Walks – Copenhagen",
            "2026-11-20",
            "New video in Example Feed",
        ],
        None,
        0,
    );
    row(
        rss,
        &[
            "Night Signals – Live Session",
            "2026-11-19",
            "New audio in Example Feed",
        ],
        None,
        0,
    );
    let page_actions = [
        [
            button(action_band, "Start", 220),
            button(action_band, "Import…", 221),
            button(action_band, "Remove", 222),
            HWND::default(),
        ],
        [
            button(action_band, "Clear", 223),
            button(action_band, "Copy", 224),
            HWND::default(),
            HWND::default(),
        ],
        [
            button(action_band, "Refresh", 225),
            button(action_band, "Feeds…", 226),
            button(action_band, "Filter help", 227),
            child(
                action_band,
                "BUTTON",
                "Refresh on a timer",
                sty!(WS_TABSTOP, BS_AUTOCHECKBOX),
                0,
                228,
            ),
        ],
    ];
    let connections = list(
        parent,
        208,
        &[
            ("#", 36, true),
            ("Action", 110, false),
            ("Description", 405, false),
            ("Status", 150, false),
            ("Server", 200, false),
        ],
        0,
    );
    let status_bar = child(
        parent,
        "msctls_statusbar32",
        "",
        sty!(SBARS_SIZEGRIP, WS_CLIPSIBLINGS),
        0,
        0,
    );
    let ui = Ui {
        toolbar,
        tab,
        status: status_bar,
        action_band,
        actions,
        filter_label,
        filter,
        lists: [queue, par2, log, rss],
        page_actions,
        connections,
    };
    UI.set(ui);
    for h in [ui.toolbar, ui.tab, ui.status, ui.action_band] {
        show(h, true);
    }
    populate_queue(ui);
    let parts: Vec<_> = [110, 215, 345, 471, 600, 722, 850, -1]
        .into_iter()
        .map(|n| if n < 0 { n } else { s(n) })
        .collect();
    send(ui.status, SB_SETPARTS, parts.len(), parts.as_ptr() as isize);
    status(ui);
    select_tab(parent);
}

fn select_tab(parent: HWND) {
    let ui = UI.get();
    let tab = send(ui.tab, TCM_GETCURSEL, 0, 0);
    for (i, h) in ui.lists.iter().enumerate() {
        show(*h, i as isize == tab);
    }
    for (i, controls) in ui.page_actions.iter().enumerate() {
        for h in controls {
            show(*h, i as isize + 1 == tab);
        }
    }
    for h in [ui.actions, ui.filter, ui.filter_label] {
        show(h, tab == 0);
    }
    show(ui.connections, tab == 0 && CONNS.get());
    layout(parent);
}

fn layout(parent: HWND) {
    let ui = UI.get();
    if ui.tab.is_invalid() {
        return;
    }
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(parent, &mut r);
    }
    send(ui.status, WM_SIZE, 0, 0);
    let mut sb = RECT::default();
    unsafe {
        let _ = GetWindowRect(ui.status, &mut sb);
    }
    let top = s(if COMPACT_MODE.get() { 50 } else { 58 });
    move_to(ui.toolbar, 0, 0, r.right, top);
    let body = r.bottom - top - (sb.bottom - sb.top);
    move_to(ui.tab, 0, top, r.right, body);
    let mut area = RECT {
        top: 0,
        left: 0,
        right: r.right,
        bottom: body,
    };
    send(ui.tab, TCM_ADJUSTRECT, 0, &mut area as *mut _ as isize);
    let (x, y, width, height) = (
        area.left,
        area.top + top,
        area.right - area.left,
        area.bottom - area.top,
    );
    let line = s(30);
    let filter_width = s(183);
    move_to(ui.action_band, x, y, width, line);
    move_to(ui.actions, 0, s(4), width - filter_width - s(60), s(23));
    move_to(
        ui.filter_label,
        width - filter_width - s(50),
        s(7),
        s(40),
        s(18),
    );
    move_to(
        ui.filter,
        width - filter_width - s(4),
        s(4),
        filter_width,
        s(22),
    );
    let connections = if CONNS.get() {
        (height / 4).clamp(s(84), s(158))
    } else {
        0
    };
    let split = if connections > 0 { s(5) } else { 0 };
    move_to(
        ui.lists[0],
        x,
        y + line,
        width,
        height - line - connections - split,
    );
    move_to(
        ui.connections,
        x,
        y + height - connections,
        width,
        connections,
    );
    for h in &ui.lists[1..] {
        move_to(*h, x, y + line, width, height - line);
    }
    for controls in ui.page_actions {
        for (i, h) in controls.into_iter().enumerate() {
            move_to(
                h,
                s(5 + i as i32 * 88),
                s(3),
                s(if i == 3 { 155 } else { 82 }),
                s(24),
            );
        }
    }
}

fn set_density(parent: HWND, compact: bool) {
    COMPACT_MODE.set(compact);
    let ui = UI.get();
    for list in ui.lists.into_iter().chain([ui.connections]) {
        send(
            list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            LVS_EX_GRIDLINES as usize,
            if compact {
                LVS_EX_GRIDLINES as isize
            } else {
                0
            },
        );
    }
    send(
        ui.toolbar,
        TB_SETBUTTONSIZE,
        0,
        packed(s(55), s(if compact { 46 } else { 54 })),
    );
    unsafe {
        let bar = GetMenu(parent);
        let view = GetSubMenu(bar, 2);
        CheckMenuRadioItem(
            view,
            VIEWS as u32,
            COMPACT as u32,
            if compact {
                COMPACT as u32
            } else {
                VIEWS as u32
            },
            MF_BYCOMMAND.0,
        )
        .unwrap();
    }
    layout(parent);
}

fn choose_tab(parent: HWND, index: usize) {
    send(UI.get().tab, TCM_SETCURSEL, index, 0);
    select_tab(parent);
}

unsafe fn draw_progress(queue: HWND, draw: &NMLVCUSTOMDRAW) -> LRESULT {
    let stage = draw.nmcd.dwDrawStage.0;
    if stage == CDDS_PREPAINT.0 {
        return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
    }
    if stage == CDDS_ITEMPREPAINT.0 {
        return LRESULT(CDRF_NOTIFYSUBITEMDRAW as isize);
    }
    let amount = draw.nmcd.lItemlParam.0;
    if stage != (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0) || draw.iSubItem != 6 || amount == 0 {
        return LRESULT(CDRF_DODEFAULT as isize);
    }
    let mut rect = RECT {
        left: LVIR_BOUNDS as i32,
        top: 6,
        ..Default::default()
    };
    if send(
        queue,
        LVM_GETSUBITEMRECT,
        draw.nmcd.dwItemSpec,
        &mut rect as *mut _ as isize,
    ) == 0
    {
        return LRESULT(CDRF_DODEFAULT as isize);
    }
    let _ = InflateRect(&mut rect, -s(5), -s(2));
    let theme = OpenThemeData(Some(queue), w!("Progress"));
    if theme.0 == 0 {
        return LRESULT(CDRF_DODEFAULT as isize);
    }
    let hdc = draw.nmcd.hdc;
    let _ = DrawThemeBackground(theme, hdc, PP_TRANSPARENTBAR.0, PBBS_NORMAL.0, &rect, None);
    let mut fill = rect;
    fill.right = rect.left + (rect.right - rect.left) * amount.unsigned_abs() as i32 / 100;
    let state = if amount < 0 { PBFS_PAUSED } else { PBFS_NORMAL };
    let _ = DrawThemeBackground(theme, hdc, PP_FILL.0, state.0, &fill, None);
    let _ = CloseThemeData(theme);
    let mode = SetBkMode(hdc, TRANSPARENT);
    let color = SetTextColor(hdc, COLORREF(GetSysColor(COLOR_BTNTEXT)));
    let mut label = wz(&format!(
        "{} {}%",
        if amount < 0 { "Paused" } else { "Downloading" },
        amount.unsigned_abs()
    ));
    let format = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    DrawTextW(hdc, &mut label, &mut rect, format);
    let saved = SaveDC(hdc);
    if saved != 0 {
        IntersectClipRect(hdc, fill.left, fill.top, fill.right, fill.bottom);
        SetTextColor(hdc, COLORREF(0x00FF_FFFF));
        DrawTextW(hdc, &mut label, &mut rect, format);
        let _ = RestoreDC(hdc, saved);
    }
    SetTextColor(hdc, color);
    SetBkMode(hdc, BACKGROUND_MODE(mode as u32));
    LRESULT(CDRF_SKIPDEFAULT as isize)
}

fn about(parent: HWND, title: PCWSTR, message: PCWSTR) {
    unsafe {
        MessageBoxW(Some(parent), message, title, MB_OK | MB_ICONINFORMATION);
    }
}

fn command(parent: HWND, id: usize) -> bool {
    let ui = UI.get();
    match id {
        OPTIONS => { open_setup(parent, true); }
        EXIT => unsafe { let _ = DestroyWindow(parent); },
        116 => unsafe { let _ = ShowWindow(parent, SW_MINIMIZE); },
        112 => about(parent, w!("About NewsQueue"), w!("NewsQueue native Win32 mockup.\nThe queue, history and server entries are demo data.")),
        NZB => {
            let choice = unsafe { MessageBoxW(Some(parent), w!("Add the sample NZB collection to the demo queue?"),
                w!("Import NZB"), MB_OKCANCEL | MB_ICONQUESTION) };
            if choice == IDOK { command(parent, ADD_DEMO); }
        }
        ADD_DEMO => { IMPORTED.set(true); choose_tab(parent, 0); populate_queue(ui); }
        CLEANUP => { CLEANED.set(true); populate_queue(ui); }
        CONNECT => { CONNECTED.set(!CONNECTED.get()); populate_queue(ui); status(ui); }
        PAUSE | 401 | 402 => {
            PAUSED.set(match id { 401 => false, 402 => true, _ => !PAUSED.get() });
            populate_queue(ui);
            status(ui);
        }
        COLLAPSE | EXPAND => { EXPANDED.set(id == EXPAND); populate_queue(ui); }
        VIEWS | COMPACT => set_density(parent, id == COMPACT),
        CONNECTIONS => {
            CONNS.set(!CONNS.get());
            unsafe {
                let view = GetSubMenu(GetMenu(parent), 2);
                CheckMenuItem(view, CONNECTIONS as u32,
                    if CONNS.get() { MF_CHECKED.0 } else { MF_UNCHECKED.0 });
            }
            select_tab(parent);
        }
        220 | 221 | 222 | 224 | 225 | 226 | 227 =>
            about(parent, w!("NewsQueue demo"), w!("This command is represented by its native control in the mockup.")),
        223 => { send(ui.lists[2], LVM_DELETEALLITEMS, 0, 0); }
        _ => return false,
    }
    true
}

unsafe extern "system" fn main_proc(h: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_SIZE => layout(h),
        WM_GETMINMAXINFO => {
            let bounds = &mut *(lp.0 as *mut MINMAXINFO);
            bounds.ptMinTrackSize.x = s(854);
            bounds.ptMinTrackSize.y = s(422);
        }
        WM_COMMAND => {
            let id = lo(wp.0 as isize) as usize;
            if id == FILTER && hi(wp.0 as isize) == EN_CHANGE as i32 {
                populate_queue(UI.get());
            } else if !command(h, id) {
                return DefWindowProcW(h, message, wp, lp);
            }
        }
        WM_NOTIFY => {
            let header = &*(lp.0 as *const NMHDR);
            match (header.idFrom, header.code) {
                (TAB, TCN_SELCHANGE) => select_tab(h),
                (QUEUE, NM_CUSTOMDRAW) => {
                    return draw_progress(header.hwndFrom, &*(lp.0 as *const NMLVCUSTOMDRAW));
                }
                (QUEUE, NM_DBLCLK) => {
                    let hit = &*(lp.0 as *const NMITEMACTIVATE);
                    if hit.iItem == 0 {
                        EXPANDED.set(!EXPANDED.get());
                        populate_queue(UI.get());
                    }
                }
                _ => return DefWindowProcW(h, message, wp, lp),
            }
        }
        WM_DESTROY => PostQuitMessage(0),
        _ => return DefWindowProcW(h, message, wp, lp),
    }
    LRESULT(0)
}

fn positioned(h: HWND, x: i32, y: i32, width: i32, height: i32) -> HWND {
    move_to(h, s(x), s(y), s(width), s(height));
    show(h, true);
    h
}

fn label(parent: HWND, value: &str, x: i32, y: i32, width: i32, height: i32) -> HWND {
    positioned(child(parent, "STATIC", value, 0, 0, 0), x, y, width, height)
}

fn edit(parent: HWND, value: &str, x: i32, y: i32, width: i32, id: usize) -> HWND {
    positioned(
        child(
            parent,
            "EDIT",
            value,
            sty!(WS_TABSTOP, ES_AUTOHSCROLL),
            WS_EX_CLIENTEDGE.0,
            id,
        ),
        x,
        y,
        width,
        23,
    )
}

fn check(parent: HWND, value: &str, x: i32, y: i32, id: usize, checked: bool) {
    let check = positioned(
        child(
            parent,
            "BUTTON",
            value,
            sty!(WS_TABSTOP, BS_AUTOCHECKBOX),
            0,
            id,
        ),
        x,
        y,
        344,
        24,
    );
    send(check, BM_SETCHECK, usize::from(checked), 0);
}

fn setup_page(dialog: HWND, index: isize) {
    let old = unsafe { GetDlgItem(Some(dialog), 899) };
    if let Ok(old) = old {
        unsafe {
            let _ = DestroyWindow(old);
        }
    }
    let panel = positioned(
        child(
            dialog,
            "NewsQueueNativeMockupPanel",
            "",
            sty!(WS_CLIPCHILDREN),
            WS_EX_CONTROLPARENT.0,
            899,
        ),
        184,
        10,
        526,
        510,
    );
    let heading = label(
        panel,
        ["General", "NZB", "Servers"][index as usize],
        4,
        2,
        485,
        22,
    );
    send(heading, WM_SETFONT, bold().0 as usize, 1);
    positioned(child(panel, "STATIC", "", 0x10, 0, 0), 4, 27, 518, 2);
    match index {
        0 => {
            let box_ = positioned(
                child(
                    panel,
                    "BUTTON",
                    "Startup and window behavior",
                    BS_GROUPBOX as u32,
                    0,
                    0,
                ),
                4,
                42,
                518,
                128,
            );
            unsafe {
                let _ = SetWindowPos(box_, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            check(panel, "Connect automatically at startup", 17, 66, 700, true);
            check(
                panel,
                "Minimize to the notification area",
                17,
                94,
                701,
                true,
            );
            check(panel, "Close to the notification area", 17, 122, 702, false);
            label(panel, "Queue autosave interval", 4, 193, 174, 20);
            edit(panel, "30", 183, 189, 66, 703);
            label(panel, "seconds", 259, 193, 104, 20);
        }
        1 => {
            label(panel, "Download folder", 4, 55, 130, 19);
            edit(panel, r"D:\Downloads", 146, 51, 370, 710);
            label(panel, "NZB watch folder", 4, 88, 130, 19);
            edit(panel, r"D:\NZB", 146, 84, 370, 711);
            check(
                panel,
                "Automatically import NZBs from the watch folder",
                4,
                126,
                712,
                false,
            );
            check(panel, "Add new collections paused", 4, 153, 713, false);
        }
        _ => {
            for (i, name) in [
                "Add server",
                "Edit server",
                "Delete server",
                "Move up",
                "Move down",
            ]
            .iter()
            .enumerate()
            {
                let action =
                    positioned(button(panel, name, 730 + i), 4 + 104 * i as i32, 43, 98, 25);
                unsafe {
                    let _ = EnableWindow(action, false);
                }
            }
            let servers = positioned(
                list(
                    panel,
                    721,
                    &[
                        ("Name", 125, false),
                        ("Address", 148, false),
                        ("Port", 50, true),
                        ("Conn.", 48, true),
                        ("SSL", 43, false),
                        ("Level", 57, true),
                    ],
                    LVS_EX_CHECKBOXES,
                ),
                4,
                78,
                518,
                235,
            );
            row(
                servers,
                &[
                    "Example Usenet",
                    "news.example.invalid",
                    "563",
                    "4",
                    "Yes",
                    "0",
                ],
                None,
                0,
            );
            row(
                servers,
                &["Backup", "backup.example.invalid", "563", "2", "Yes", "1"],
                None,
                0,
            );
            let state = LVITEMW {
                stateMask: LVIS_STATEIMAGEMASK,
                state: LIST_VIEW_ITEM_STATE_FLAGS(2 << 12),
                ..Default::default()
            };
            send(servers, LVM_SETITEMSTATE, 0, &state as *const _ as isize);
            label(
                panel,
                "Tick a server to use it. Level 0 servers are used first.",
                4,
                327,
                518,
                20,
            );
        }
    }
}

unsafe fn surface_label(wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dc = HDC(wp.0 as *mut c_void);
    let control = HWND(lp.0 as *mut c_void);
    SetBkColor(dc, COLORREF(GetSysColor(COLOR_BTNFACE)));
    SetTextColor(
        dc,
        COLORREF(GetSysColor(if IsWindowEnabled(control).as_bool() {
            COLOR_BTNTEXT
        } else {
            COLOR_GRAYTEXT
        })),
    );
    LRESULT(GetSysColorBrush(COLOR_BTNFACE).0 as isize)
}

unsafe extern "system" fn panel_proc(h: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_CTLCOLORSTATIC {
        return surface_label(wp, lp);
    }
    if matches!(message, WM_COMMAND | WM_NOTIFY) {
        if let Ok(dialog) = GetParent(h) {
            return LRESULT(send(dialog, message, wp.0, lp.0));
        }
    }
    DefWindowProcW(h, message, wp, lp)
}

unsafe extern "system" fn setup_proc(h: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_CTLCOLORSTATIC => return surface_label(wp, lp),
        WM_COMMAND => match lo(wp.0 as isize) as usize {
            SETTINGS_LIST if hi(wp.0 as isize) == LBN_SELCHANGE as i32 => {
                let index = send(HWND(lp.0 as *mut c_void), LB_GETCURSEL, 0, 0);
                if index >= 0 {
                    setup_page(h, index);
                }
            }
            APPLY => {
                let _ = EnableWindow(GetDlgItem(Some(h), APPLY as i32).unwrap(), false);
            }
            id if id == IDOK.0 as usize || id == IDCANCEL.0 as usize => {
                let _ = DestroyWindow(h);
            }
            700..=729 => {
                if let Ok(apply) = GetDlgItem(Some(h), APPLY as i32) {
                    let _ = EnableWindow(apply, true);
                }
            }
            _ => return DefWindowProcW(h, message, wp, lp),
        },
        WM_CLOSE => {
            let _ = DestroyWindow(h);
        }
        WM_DESTROY => DIALOG.set(HWND::default()),
        _ => return DefWindowProcW(h, message, wp, lp),
    }
    LRESULT(0)
}

fn open_setup(owner: HWND, focus: bool) -> HWND {
    unsafe {
        if !DIALOG.get().is_invalid() {
            if focus {
                let _ = SetForegroundWindow(DIALOG.get());
            }
            return DIALOG.get();
        }
        let style = sty!(WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_CLIPCHILDREN);
        let ex = sty!(WS_EX_DLGMODALFRAME, WS_EX_CONTROLPARENT);
        let (w, h) = outer(style, ex, 720, 576);
        let mut rect = RECT::default();
        let _ = GetWindowRect(owner, &mut rect);
        let dialog = CreateWindowExW(
            WINDOW_EX_STYLE(ex),
            SETUP,
            w!("Setup"),
            WINDOW_STYLE(style),
            rect.left + (rect.right - rect.left - w) / 2,
            rect.top + (rect.bottom - rect.top - h) / 2,
            w,
            h,
            Some(owner),
            None,
            Some(hinst()),
            None,
        )
        .unwrap();
        DIALOG.set(dialog);
        let pages = positioned(
            child(
                dialog,
                "LISTBOX",
                "",
                sty!(WS_TABSTOP, LBS_NOTIFY, LBS_NOINTEGRALHEIGHT),
                WS_EX_CLIENTEDGE.0,
                SETTINGS_LIST,
            ),
            10,
            10,
            164,
            510,
        );
        for name in ["General", "NZB", "Servers"] {
            send(pages, LB_ADDSTRING, 0, wz(name).as_ptr() as isize);
        }
        send(pages, LB_SETCURSEL, 2, 0);
        setup_page(dialog, 2);
        positioned(child(dialog, "STATIC", "", 0x10, 0, 0), 10, 530, 700, 2);
        let ok = positioned(
            child(
                dialog,
                "BUTTON",
                "OK",
                sty!(WS_TABSTOP, BS_DEFPUSHBUTTON),
                0,
                IDOK.0 as usize,
            ),
            454,
            541,
            80,
            26,
        );
        positioned(
            button(dialog, "Cancel", IDCANCEL.0 as usize),
            542,
            541,
            80,
            26,
        );
        let apply = positioned(button(dialog, "Apply", APPLY), 630, 541, 80, 26);
        let _ = EnableWindow(apply, false);
        if focus {
            show(dialog, true);
        } else {
            show_offscreen(dialog);
        }
        if focus {
            let _ = SetFocus(Some(ok));
        }
        dialog
    }
}

fn check_demo(window: HWND) {
    let ui = UI.get();
    assert_eq!(send(ui.lists[0], LVM_GETITEMCOUNT, 0, 0), 9);
    text(ui.filter, "copenhagen");
    assert_eq!(send(ui.lists[0], LVM_GETITEMCOUNT, 0, 0), 1);
    text(ui.filter, "");
    command(window, COLLAPSE);
    assert_eq!(send(ui.lists[0], LVM_GETITEMCOUNT, 0, 0), 4);
    command(window, EXPAND);
    command(window, ADD_DEMO);
    assert_eq!(send(ui.lists[0], LVM_GETITEMCOUNT, 0, 0), 10);
    choose_tab(window, 2);
    assert!(unsafe { IsWindowVisible(ui.lists[2]).as_bool() });
    assert!(!unsafe { IsWindowVisible(ui.connections).as_bool() });
    choose_tab(window, 0);
    command(window, CONNECTIONS);
    assert!(!unsafe { IsWindowVisible(ui.connections).as_bool() });
    command(window, CONNECTIONS);
    assert!(unsafe { IsWindowVisible(ui.connections).as_bool() });
    command(window, COMPACT);
    assert_ne!(
        send(ui.lists[0], LVM_GETEXTENDEDLISTVIEWSTYLE, 0, 0) & LVS_EX_GRIDLINES as isize,
        0
    );
    command(window, PAUSE);
    assert!(PAUSED.get());
    let dialog = open_setup(window, false);
    assert!(unsafe { IsWindowVisible(dialog).as_bool() });
    assert_eq!(open_setup(window, false), dialog);
    let nav = unsafe { GetDlgItem(Some(dialog), SETTINGS_LIST as i32).unwrap() };
    assert_eq!(send(nav, LB_GETCOUNT, 0, 0), 3);
    send(dialog, WM_CLOSE, 0, 0);
    assert!(DIALOG.get().is_invalid());
}

unsafe fn show_offscreen(window: HWND) {
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN) - 4096;
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN) - 4096;
    SetWindowPos(
        window,
        None,
        x,
        y,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW,
    )
    .unwrap();
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    COMPACT_MODE.set(!args.iter().any(|arg| arg == "--roomy"));
    let capture_folder = args
        .windows(2)
        .find(|pair| pair[0] == "--capture")
        .map(|pair| PathBuf::from(&pair[1]));
    let smoke = args.iter().any(|arg| arg == "--smoke");
    unsafe {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        assert!(InitCommonControlsEx(&controls).as_bool());
        init_fonts();
        let classes: [(PCWSTR, WNDPROC); 3] = [
            (MAIN, Some(main_proc)),
            (SETUP, Some(setup_proc)),
            (PANEL, Some(panel_proc)),
        ];
        for (name, proc) in classes {
            let class = WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: hinst(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hIcon: LoadIconW(Some(hinst()), PCWSTR(1 as *const u16)).unwrap(),
                hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
                lpszClassName: name,
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
        }
        let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
        let width = (GetSystemMetrics(SM_CXSCREEN) - s(36)).min(s(1285));
        let height = (GetSystemMetrics(SM_CYSCREEN) - s(96)).min(s(830));
        // An unshown owner keeps background verification out of the taskbar.
        let test_owner = (capture_folder.is_some() || smoke).then(|| {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinst()),
                None,
            )
            .unwrap()
        });
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MAIN,
            w!("NewsQueue — Native Win32 Mockup"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            test_owner,
            None,
            Some(hinst()),
            None,
        )
        .unwrap();
        build(window);
        let _ = SetWindowPos(
            UI.get().tab,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        if capture_folder.is_some() || smoke {
            show_offscreen(window);
        } else {
            let _ = ShowWindow(window, SW_SHOW);
        }
        layout(window);
        if let Some(folder) = capture_folder {
            std::fs::create_dir_all(&folder).unwrap();
            for (compact, name) in [(false, "roomy"), (true, "compact")] {
                set_density(window, compact);
                capture(
                    window,
                    folder.join(format!("queue-{name}.png")).to_str().unwrap(),
                );
            }
            set_density(window, false);
            let dialog = open_setup(window, false);
            capture(dialog, folder.join("setup.png").to_str().unwrap());
            let _ = DestroyWindow(window);
            return;
        }
        if smoke {
            check_demo(window);
            let _ = DestroyWindow(window);
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let dialog = DIALOG.get();
            if !dialog.is_invalid() && IsDialogMessageW(dialog, &message).as_bool() {
                continue;
            }
            if message.message == WM_KEYDOWN {
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let id = match (message.wParam.0 as u16, ctrl) {
                    (0x4F, true) => Some(NZB),
                    (key, false) if key == VK_F1.0 => Some(PAUSE),
                    (key, false) if key == VK_F9.0 => Some(CONNECTIONS),
                    _ => None,
                };
                if let Some(id) = id {
                    command(window, id);
                    continue;
                }
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
