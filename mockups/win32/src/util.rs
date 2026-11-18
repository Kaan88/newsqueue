#![allow(dead_code)]
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering::Relaxed};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub static DPI: AtomicU32 = AtomicU32::new(96);
static FONT: AtomicIsize = AtomicIsize::new(0);
static BOLD: AtomicIsize = AtomicIsize::new(0);

pub trait U: Copy {
    fn u(self) -> u32;
}
impl U for u32 {
    fn u(self) -> u32 {
        self
    }
}
impl U for i32 {
    fn u(self) -> u32 {
        self as u32
    }
}
impl U for WINDOW_STYLE {
    fn u(self) -> u32 {
        self.0
    }
}
impl U for WINDOW_EX_STYLE {
    fn u(self) -> u32 {
        self.0
    }
}
/// OR together style constants of mixed integer/newtype kinds.
#[macro_export]
macro_rules! sty {
    ($($x:expr),* $(,)?) => { 0u32 $(| $crate::util::U::u($x))* };
}

pub fn s(v: i32) -> i32 {
    ((v as i64 * DPI.load(Relaxed) as i64 + 48) / 96) as i32
}
pub fn wz(t: &str) -> Vec<u16> {
    t.encode_utf16().chain(Some(0)).collect()
}
pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
pub fn hinst() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).unwrap().into() }
}
pub fn font() -> HFONT {
    HFONT(FONT.load(Relaxed) as *mut c_void)
}
pub fn bold() -> HFONT {
    HFONT(BOLD.load(Relaxed) as *mut c_void)
}

/// Segoe UI via the system message font at the current DPI.
pub fn init_fonts() {
    unsafe {
        let dpi = std::env::var("NQ_DPI").ok().and_then(|v| v.parse().ok()).unwrap_or(GetDpiForSystem());
        DPI.store(dpi, Relaxed);
        let mut ncm = NONCLIENTMETRICSW::default();
        ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            0,
            dpi,
        )
        .unwrap();
        let f = CreateFontIndirectW(&ncm.lfMessageFont);
        FONT.store(f.0 as isize, Relaxed);
        let mut lb = ncm.lfMessageFont;
        lb.lfWeight = 700;
        BOLD.store(CreateFontIndirectW(&lb).0 as isize, Relaxed);
    }
}

pub fn send(h: HWND, m: u32, w: usize, l: isize) -> isize {
    unsafe { SendMessageW(h, m, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

/// Create a child/popup window; x,y,w,h are pixels.
pub fn mkpx(class: &str, text: &str, style: u32, ex: u32, x: i32, y: i32, w: i32, h: i32, parent: Option<HWND>, id: i32) -> HWND {
    let c = wz(class);
    let t = wz(text);
    unsafe {
        let hw = CreateWindowExW(
            WINDOW_EX_STYLE(ex),
            PCWSTR(c.as_ptr()),
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(style),
            x,
            y,
            w,
            h,
            parent,
            Some(HMENU(id as usize as *mut c_void)),
            Some(hinst()),
            None,
        )
        .expect("CreateWindowExW");
        send(hw, WM_SETFONT, font().0 as usize, 1);
        hw
    }
}
/// Same but coordinates in 96-dpi units.
pub fn mk(class: &str, text: &str, style: u32, ex: u32, x: i32, y: i32, w: i32, h: i32, parent: HWND, id: i32) -> HWND {
    mkpx(class, text, style, ex, s(x), s(y), s(w), s(h), Some(parent), id)
}

pub fn lo(v: isize) -> i32 {
    (v & 0xffff) as i32
}
pub fn hi(v: isize) -> i32 {
    ((v >> 16) & 0xffff) as i32
}

pub fn pump(ms: u64) {
    let t0 = std::time::Instant::now();
    unsafe {
        while t0.elapsed().as_millis() < ms as u128 {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}

// ---------------------------------------------------------------- icons ----

pub struct Cv {
    pub dc: HDC,
    pub k: f64,
}
pub const BLUE: u32 = 0x2D6CDF;
pub const GREEN: u32 = 0x2E9B3E;
pub const RED: u32 = 0xD03A2E;
pub const ORANGE: u32 = 0xE08A1E;
pub const GRAY: u32 = 0x5A6270;
pub const YELLOW: u32 = 0xF2C230;
pub const WHITE: u32 = 0xFFFFFF;
fn cr(c: u32) -> COLORREF {
    rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
impl Cv {
    fn p(&self, v: f64) -> i32 {
        (v * self.k).round() as i32
    }
    fn rect(&self, x: f64, y: f64, w: f64, h: f64, col: u32) {
        unsafe {
            let b = CreateSolidBrush(cr(col));
            let r = RECT { left: self.p(x), top: self.p(y), right: self.p(x + w), bottom: self.p(y + h) };
            FillRect(self.dc, &r, b);
            let _ = DeleteObject(b.into());
        }
    }
    fn ell(&self, x: f64, y: f64, w: f64, h: f64, fill: Option<u32>, line: Option<(u32, f64)>) {
        unsafe {
            let b = match fill {
                Some(c) => CreateSolidBrush(cr(c)),
                None => HBRUSH(GetStockObject(NULL_BRUSH).0),
            };
            let pen = match line {
                Some((c, wd)) => CreatePen(PS_SOLID, self.p(wd).max(1), cr(c)),
                None => HPEN(GetStockObject(NULL_PEN).0),
            };
            let ob = SelectObject(self.dc, b.into());
            let op = SelectObject(self.dc, pen.into());
            let _ = Ellipse(self.dc, self.p(x), self.p(y), self.p(x + w), self.p(y + h));
            SelectObject(self.dc, ob);
            SelectObject(self.dc, op);
            let _ = DeleteObject(pen.into());
            if fill.is_some() {
                let _ = DeleteObject(b.into());
            }
        }
    }
    fn poly(&self, pts: &[(f64, f64)], fill: u32) {
        unsafe {
            let v: Vec<POINT> = pts.iter().map(|&(x, y)| POINT { x: self.p(x), y: self.p(y) }).collect();
            let b = CreateSolidBrush(cr(fill));
            let pen = HPEN(GetStockObject(NULL_PEN).0);
            let ob = SelectObject(self.dc, b.into());
            let op = SelectObject(self.dc, pen.into());
            let _ = Polygon(self.dc, &v);
            SelectObject(self.dc, ob);
            SelectObject(self.dc, op);
            let _ = DeleteObject(b.into());
        }
    }
    fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64, w: f64, col: u32) {
        unsafe {
            let pen = CreatePen(PS_SOLID, self.p(w).max(1), cr(col));
            let op = SelectObject(self.dc, pen.into());
            let _ = MoveToEx(self.dc, self.p(x1), self.p(y1), None);
            let _ = LineTo(self.dc, self.p(x2), self.p(y2));
            SelectObject(self.dc, op);
            let _ = DeleteObject(pen.into());
        }
    }
    fn frame(&self, x: f64, y: f64, w: f64, h: f64, lw: f64, col: u32) {
        self.line(x, y, x + w, y, lw, col);
        self.line(x + w, y, x + w, y + h, lw, col);
        self.line(x + w, y + h, x, y + h, lw, col);
        self.line(x, y + h, x, y, lw, col);
    }
    fn arc(&self, x: f64, y: f64, w: f64, h: f64, a: (f64, f64), b: (f64, f64), lw: f64, col: u32) {
        unsafe {
            let pen = CreatePen(PS_SOLID, self.p(lw).max(1), cr(col));
            let op = SelectObject(self.dc, pen.into());
            let _ = Arc(self.dc, self.p(x), self.p(y), self.p(x + w), self.p(y + h), self.p(a.0), self.p(a.1), self.p(b.0), self.p(b.1));
            SelectObject(self.dc, op);
            let _ = DeleteObject(pen.into());
        }
    }
    fn page(&self, col: u32) {
        self.poly(&[(5., 2.), (15., 2.), (19., 6.), (19., 22.), (5., 22.)], WHITE);
        self.line(5., 2., 15., 2., 1.2, col);
        self.line(15., 2., 19., 6., 1.2, col);
        self.line(19., 6., 19., 22., 1.2, col);
        self.line(19., 22., 5., 22., 1.2, col);
        self.line(5., 22., 5., 2., 1.2, col);
    }
    fn arrow_down(&self, col: u32, y0: f64) {
        self.rect(9.5, y0, 5., 8., col);
        self.poly(&[(5., y0 + 8.), (19., y0 + 8.), (12., y0 + 15.)], col);
    }
    fn arrow_up(&self, col: u32, y0: f64) {
        self.rect(9.5, y0 + 7., 5., 8., col);
        self.poly(&[(5., y0 + 7.), (19., y0 + 7.), (12., y0)], col);
    }
}

/// Everything is drawn on a 24-unit grid.
pub fn draw_icon(c: &Cv, name: &str) {
    match name {
        "connect" => {
            c.rect(2., 8., 8., 8., BLUE);
            c.rect(14., 8., 8., 8., BLUE);
            c.rect(9., 11., 6., 2., GRAY);
            c.rect(10., 5., 2., 3., GRAY);
            c.rect(14., 16., 2., 3., GRAY);
        }
        "pause" | "pause_s" => {
            c.rect(5., 4., 5., 16., BLUE);
            c.rect(14., 4., 5., 16., BLUE);
        }
        "pausefor" => {
            c.ell(2., 2., 20., 20., Some(WHITE), Some((ORANGE, 2.0)));
            c.line(12., 12., 12., 6., 2., GRAY);
            c.line(12., 12., 16., 14., 2., GRAY);
        }
        "nzb" => {
            c.page(GRAY);
            c.line(8., 8., 16., 8., 1.5, GRAY);
            c.line(8., 12., 16., 12., 1.5, GRAY);
            c.poly(&[(8., 15.), (16., 15.), (12., 20.)], GREEN);
        }
        "download" => {
            c.arrow_down(GREEN, 2.);
            c.rect(3., 19., 18., 3., GRAY);
        }
        "cleanup" => {
            c.rect(4., 4.5, 16., 2.5, GRAY);
            c.rect(9., 2., 6., 3., GRAY);
            c.poly(&[(6., 8.), (18., 8.), (17., 22.), (7., 22.)], BLUE);
            c.line(10., 11., 10.5, 19., 1.3, WHITE);
            c.line(14., 11., 13.5, 19., 1.3, WHITE);
        }
        "scheduler" => {
            c.rect(3., 4., 18., 18., WHITE);
            c.frame(3., 4., 18., 18., 1.2, GRAY);
            c.rect(3., 4., 18., 5., BLUE);
            for r in 0..2 {
                for q in 0..3 {
                    c.rect(6. + q as f64 * 5., 11.5 + r as f64 * 5., 3., 3., GRAY);
                }
            }
        }
        "shutdown" => {
            c.arc(4., 5., 16., 16., (7.5, 6.5), (16.5, 6.5), 2.4, RED);
            c.line(12., 2., 12., 12., 2.6, RED);
        }
        "setup" => {
            for i in 0..8 {
                let a = i as f64 * std::f64::consts::FRAC_PI_4;
                c.line(12. + 6. * a.cos(), 12. + 6. * a.sin(), 12. + 10. * a.cos(), 12. + 10. * a.sin(), 3.4, GRAY);
            }
            c.ell(4., 4., 16., 16., Some(GRAY), None);
            c.ell(8.5, 8.5, 7., 7., Some(WHITE), None);
        }
        "home" => {
            c.rect(5., 11., 14., 10., 0xE8C98A);
            c.poly(&[(2., 12.), (12., 3.), (22., 12.)], RED);
            c.rect(10., 14., 4., 7., 0x8A5A2B);
        }
        "version" => {
            c.ell(3., 3., 18., 18., Some(BLUE), None);
            c.arrow_up(WHITE, 5.);
        }
        "about" => {
            c.ell(2., 2., 20., 20., Some(BLUE), None);
            c.ell(10.2, 5.2, 3.6, 3.6, Some(WHITE), None);
            c.rect(10.3, 10., 3.4, 8., WHITE);
        }
        "notes" => {
            c.rect(4., 3., 16., 18., YELLOW);
            c.line(7., 8., 17., 8., 1.3, 0x9A7A10);
            c.line(7., 12., 17., 12., 1.3, 0x9A7A10);
            c.line(7., 16., 13., 16., 1.3, 0x9A7A10);
        }
        "stats" => {
            c.rect(3., 12., 5., 9., BLUE);
            c.rect(10., 5., 5., 16., GREEN);
            c.rect(17., 9., 5., 12., ORANGE);
        }
        "tray" => {
            c.rect(2., 15., 20., 6., GRAY);
            c.arrow_down(BLUE, 1.);
            c.rect(4., 17., 16., 2., WHITE);
        }
        "play" => c.poly(&[(6., 3.), (20., 12.), (6., 21.)], GREEN),
        "top" => {
            c.rect(4., 3., 16., 3., GRAY);
            c.arrow_up(BLUE, 7.);
        }
        "up" => c.arrow_up(BLUE, 4.),
        "down" => c.arrow_down(BLUE, 4.),
        "bottom" => {
            c.arrow_down(BLUE, 1.);
            c.rect(4., 18., 16., 3., GRAY);
        }
        "collapse" | "minus" => {
            c.rect(3., 3., 18., 18., WHITE);
            c.frame(3., 3., 18., 18., 1.6, GRAY);
            c.rect(6.5, 10.5, 11., 3., GRAY);
        }
        "expand" | "plus" => {
            c.rect(3., 3., 18., 18., WHITE);
            c.frame(3., 3., 18., 18., 1.6, GRAY);
            c.rect(6.5, 10.5, 11., 3., GRAY);
            c.rect(10.5, 6.5, 3., 11., GRAY);
        }
        "unrar" | "nounrar" => {
            c.rect(3., 7., 18., 14., 0xB5733A);
            c.rect(3., 3., 18., 5., 0x8C5528);
            c.rect(10., 3., 4., 18., YELLOW);
            if name == "nounrar" {
                c.line(3., 3., 21., 21., 3., RED);
            }
        }
        "folder" => {
            c.poly(&[(2., 6.), (9., 6.), (11., 8.), (22., 8.), (22., 20.), (2., 20.)], YELLOW);
            c.rect(2., 10., 20., 10., 0xF7D55E);
        }
        "rename" => {
            c.poly(&[(16., 3.), (21., 8.), (9., 20.), (3., 21.), (4., 15.)], ORANGE);
            c.line(14., 5., 19., 10., 1.5, WHITE);
        }
        "export" => {
            c.page(GRAY);
            c.arrow_up(GREEN, 6.);
        }
        "key" => {
            c.ell(2., 6., 11., 11., None, Some((ORANGE, 3.)));
            c.rect(11., 10.5, 11., 3., ORANGE);
            c.rect(17., 13., 2.5, 4., ORANGE);
        }
        "eta" => {
            c.ell(2., 2., 20., 20., Some(WHITE), Some((BLUE, 2.2)));
            c.line(12., 12., 12., 6., 2., GRAY);
            c.line(12., 12., 17., 12., 2., GRAY);
        }
        "queue" => {
            for i in 0..3 {
                c.rect(3., 4. + i as f64 * 6.5, 3., 3., BLUE);
                c.rect(8., 4. + i as f64 * 6.5, 13., 3., GRAY);
            }
        }
        "shield" => {
            c.poly(&[(12., 2.), (21., 5.), (20., 14.), (12., 22.), (4., 14.), (3., 5.)], GREEN);
            c.line(7.5, 12., 11., 15.5, 2.4, WHITE);
            c.line(11., 15.5, 17., 8., 2.4, WHITE);
        }
        "page" | "file" => c.page(GRAY),
        "rss" => {
            c.rect(2., 2., 20., 20., ORANGE);
            c.ell(5., 15., 4., 4., Some(WHITE), None);
            c.arc(1., 8., 14., 14., (15., 8.), (1., 8.), 2.4, WHITE);
        }
        _ => c.rect(4., 4., 16., 16., GRAY),
    }
}

/// Render an icon with real alpha by drawing twice (black and white backdrop) at 4x
/// and box-filtering down, then add it to an ImageList.
fn icon_dib(name: &str, size: i32) -> HBITMAP {
    unsafe {
        let ss = 4;
        let big = size * ss;
        let hdc = GetDC(None);
        let mut renders: Vec<Vec<u8>> = vec![];
        for bg in [0u8, 255u8] {
            let mem = CreateCompatibleDC(Some(hdc));
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = big;
            bi.bmiHeader.biHeight = -big;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let old = SelectObject(mem, bmp.into());
            std::ptr::write_bytes(bits as *mut u8, bg, (big * big * 4) as usize);
            draw_icon(&Cv { dc: mem, k: big as f64 / 24.0 }, name);
            let _ = GdiFlush();
            let data = std::slice::from_raw_parts(bits as *const u8, (big * big * 4) as usize).to_vec();
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            renders.push(data);
        }
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = size;
        bi.bmiHeader.biHeight = -size;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut c_void = std::ptr::null_mut();
        let out = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let (mut sb, mut sw) = ([0u32; 3], [0u32; 3]);
                for sy in 0..ss {
                    for sx in 0..ss {
                        let i = (((y * ss + sy) * big + x * ss + sx) * 4) as usize;
                        for ch in 0..3 {
                            sb[ch] += renders[0][i + ch] as u32;
                            sw[ch] += renders[1][i + ch] as u32;
                        }
                    }
                }
                let n = (ss * ss) as f64;
                let diff = ((sw[0] + sw[1] + sw[2]) as f64 - (sb[0] + sb[1] + sb[2]) as f64) / (3.0 * n * 255.0);
                let a = (1.0 - diff).clamp(0.0, 1.0);
                let o = ((y * size + x) * 4) as usize;
                if a > 0.003 {
                    for ch in 0..3 {
                        let c = sb[ch] as f64 / n / a;
                        dst[o + ch] = c.clamp(0.0, 255.0) as u8;
                    }
                }
                dst[o + 3] = (a * 255.0).round() as u8;
            }
        }
        ReleaseDC(None, hdc);
        out
    }
}

pub fn imagelist(size: i32, names: &[&str]) -> isize {
    use windows::Win32::UI::Controls::*;
    unsafe {
        let il = ImageList_Create(size, size, ILC_COLOR32, names.len() as i32, 0);
        for n in names {
            let b = icon_dib(n, size);
            ImageList_Add(il, b, None);
            let _ = DeleteObject(b.into());
        }
        il.0 as isize
    }
}

// -------------------------------------------------------------- capture ----

pub fn capture(hwnd: HWND, path: &str) {
    unsafe {
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW);
        pump(300);
        let mut wr = RECT::default();
        GetWindowRect(hwnd, &mut wr).unwrap();
        let (w, h) = (wr.right - wr.left, wr.bottom - wr.top);
        let mut fb = RECT::default();
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut fb as *mut _ as *mut c_void, std::mem::size_of::<RECT>() as u32);
        let hdc = GetDC(None);
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, bmp.into());
        if std::env::var("NQ_SCREEN").is_ok() {
            let _ = BitBlt(mem, 0, 0, w, h, Some(hdc), wr.left, wr.top, SRCCOPY);
        } else {
            let _ = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(2));
        }
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, hdc);
        // crop to the visible frame (drops the invisible resize border)
        let (cx0, cy0, cx1, cy1) = if fb.right > fb.left {
            (fb.left - wr.left, fb.top - wr.top, fb.right - wr.left, fb.bottom - wr.top)
        } else {
            (0, 0, w, h)
        };
        let (cw, ch) = (cx1 - cx0, cy1 - cy0);
        let mut rgba = Vec::with_capacity((cw * ch * 4) as usize);
        for y in cy0..cy1 {
            for x in cx0..cx1 {
                let i = ((y * w + x) * 4) as usize;
                rgba.extend_from_slice(&[buf[i + 2], buf[i + 1], buf[i], 255]);
            }
        }
        let f = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), cw as u32, ch as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
        println!("saved {path} ({cw}x{ch}, dpi {})", DPI.load(Relaxed));
    }
}

/// Outer window size for a wanted client size (96-dpi units), DPI aware.
pub fn outer(style: u32, ex: u32, cw: i32, ch: i32) -> (i32, i32) {
    unsafe {
        let mut r = RECT { left: 0, top: 0, right: s(cw), bottom: s(ch) };
        let _ = AdjustWindowRectExForDpi(&mut r, WINDOW_STYLE(style), false, WINDOW_EX_STYLE(ex), DPI.load(Relaxed));
        (r.right - r.left, r.bottom - r.top)
    }
}
