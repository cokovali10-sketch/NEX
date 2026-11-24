//! NEX Studio (native) — a modern, dark desktop editor for the NEX language.
//!
//! Pure Rust + the Win32 API via FFI (no external GUI crates). It edits NEX
//! source in a multiline edit control and compiles **in process** (linking
//! NEX's own Rust compiler, `nex_driver`) on Run, then runs the produced
//! executable.
//!
//! The look is a dark, VS Code-style workbench: an immersive dark title bar,
//! a dark editor with a Consolas face and a line-number gutter, an owner-drawn
//! accent Run button, and a dark status bar. Stack: Rust UI host + NEX compiler
//! (Rust) + C runtime (`nex_rt.c`).

#![windows_subsystem = "windows"]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::fn_to_numeric_cast)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::single_match)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::manual_c_str_literals)]
#![allow(clippy::unnecessary_cast)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::process::Command;

use nex_driver::{CompileOptions, compile_source};

type HWND = isize;
type HINSTANCE = isize;
type HMENU = isize;
type HGDIOBJ = isize;
type HDC = isize;
type HICON = isize;
type HCURSOR = isize;
type LPARAM = isize;
type WPARAM = usize;
type LRESULT = isize;

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
    time: u32,
    pt: POINT,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct DRAWITEMSTRUCT {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    hwnd_item: HWND,
    h_dc: HDC,
    rc_item: RECT,
    item_state: u32,
    item_data: usize,
}

#[repr(C)]
struct WNDCLASSA {
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: HINSTANCE,
    h_icon: HICON,
    h_cursor: HCURSOR,
    hbr_background: HGDIOBJ,
    lpsz_menu_name: *const u8,
    lpsz_class_name: *const u8,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassA(classes: *const WNDCLASSA) -> u16;
    fn DefWindowProcA(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn CreateWindowExA(
        ex_style: u32,
        class: *const u8,
        title: *const u8,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HMENU,
        instance: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    fn GetMessageA(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG);
    fn DispatchMessageA(msg: *const MSG);
    fn PostQuitMessage(code: i32);
    fn ShowWindow(hwnd: HWND, cmd: i32);
    fn UpdateWindow(hwnd: HWND);
    fn MoveWindow(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, repaint: i32) -> i32;
    fn SetWindowTextA(hwnd: HWND, text: *const u8) -> i32;
    fn GetWindowTextA(hwnd: HWND, buf: *mut u8, max: i32) -> i32;
    fn GetWindowTextLengthA(hwnd: HWND) -> i32;
    fn SendMessageA(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn GetModuleHandleA(name: *const u8) -> HINSTANCE;
    fn LoadCursorA(instance: HINSTANCE, name: *const u8) -> HCURSOR;
    fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: i32) -> i32;
    fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetStockObject(index: i32) -> HGDIOBJ;
    fn CreateSolidBrush(color: u32) -> HGDIOBJ;
    fn CreateFontA(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_family: u32,
        face: *const u8,
    ) -> HGDIOBJ;
    fn SetTextColor(dc: HDC, color: u32) -> u32;
    fn SetBkColor(dc: HDC, color: u32) -> u32;
    fn SetBkMode(dc: HDC, mode: i32) -> i32;
    fn FillRect(dc: HDC, rect: *const RECT, brush: HGDIOBJ) -> i32;
    fn RoundRect(dc: HDC, l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> i32;
    fn DrawTextA(dc: HDC, text: *const u8, count: i32, rect: *const RECT, flags: u32) -> i32;
    fn SelectObject(dc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    fn DeleteObject(obj: HGDIOBJ) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const c_void, size: u32) -> i32;
}

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const WS_CHILD: u32 = 0x40000000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_VSCROLL: u32 = 0x00200000;
const WS_HSCROLL: u32 = 0x00100000;
const WS_BORDER: u32 = 0x00800000;

const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const ES_WANTRETURN: u32 = 0x1000;
const ES_LEFT: u32 = 0x0000;
const ES_CENTER: u32 = 0x0001;

const BS_OWNERDRAW: u32 = 0x0000000B;

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_COMMAND: u32 = 0x0111;
const WM_SETFONT: u32 = 0x0030;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_DRAWITEM: u32 = 0x002B;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_MOUSEMOVE: u32 = 0x0200;

const EM_LINEINDEX: u32 = 0x00BB;
const EM_LINEFROMCHAR: u32 = 0x00C9;
const EN_CHANGE: u32 = 0x0300;

const SW_SHOW: i32 = 5;
const IDC_ARROW: usize = 32512;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const TRANSPARENT: i32 = 1;
const DT_CENTER: u32 = 0x0001;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const FW_NORMAL: i32 = 400;
const DEFAULT_CHARSET: u32 = 1;
const OUT_DEFAULT_PRECIS: u32 = 0;
const CLIP_DEFAULT_PRECIS: u32 = 0;
const DEFAULT_QUALITY: u32 = 0;
const DEFAULT_PITCH: u32 = 0;

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

const COL_BG: u32 = 0x00_1E_1E_1E; // placeholder, computed in rgb()
const _UNUSED: () = ();

const BTN_RUN: usize = 1;
const EDIT_ID: usize = 0;
const STATUS_ID: usize = 2;
const GUTTER_ID: usize = 3;
const SIDEBAR_ID: usize = 4;
const TAB_ID: usize = 5;

struct Controls {
    edit: HWND,
    output: HWND,
    run: HWND,
    status: HWND,
    gutter: HWND,
    sidebar: HWND,
    tab: HWND,
}

static mut CONTROLS: Controls = Controls {
    edit: 0,
    output: 0,
    run: 0,
    status: 0,
    gutter: 0,
    sidebar: 0,
    tab: 0,
};

static mut BG_BRUSH: HGDIOBJ = 0;
static mut EDIT_BRUSH: HGDIOBJ = 0;
static mut TOOL_BRUSH: HGDIOBJ = 0;
static mut SIDEBAR_BRUSH: HGDIOBJ = 0;
static mut RUN_RECT: RECT = RECT {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
};
static mut RUN_HOVER: bool = false;
static mut FONT_MONO: HGDIOBJ = 0;
static mut FONT_SMALL: HGDIOBJ = 0;

fn to_cstring(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let c = to_cstring(text);
    SetWindowTextA(hwnd, c.as_ptr());
}

unsafe fn get_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthA(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u8; (len as usize) + 1];
    let read = GetWindowTextA(hwnd, buf.as_mut_ptr(), (len + 1) as i32);
    buf.truncate(read.max(0) as usize);
    String::from_utf8_lossy(&buf).to_string()
}

fn line_of(source: &str, offset: usize) -> usize {
    source
        .as_bytes()
        .iter()
        .take(offset.min(source.len()))
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

/// Compiles the code in process and runs the produced executable.
fn run_code(code: &str) -> String {
    let dir = std::env::temp_dir().join("nex_studio_native");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    if let Err(error) = std::fs::write(dir.join("main.nex"), code) {
        return format!("failed to write source: {error}\n");
    }
    let options = CompileOptions {
        build_dir: Some(dir.clone()),
        ..CompileOptions::default()
    };
    let result = match compile_source("main.nex", code.to_string(), options) {
        Ok(result) => result,
        Err(error) => return format!("compile error: {error}\n"),
    };

    let mut out = String::new();
    for diagnostic in &result.diagnostics {
        let span_start = diagnostic.span.map(|span| span.start).unwrap_or(0);
        let line = line_of(code, span_start);
        let label = if diagnostic.is_error() {
            "error"
        } else {
            "warning"
        };
        out.push_str(&format!(
            "{label} [{}] line {line}: {}\n",
            diagnostic.code, diagnostic.message
        ));
    }
    if result.has_errors() {
        return out;
    }
    let Some(artifacts) = result.artifacts else {
        out.push_str("no artifacts produced\n");
        return out;
    };
    let Some(exe) = artifacts.executable else {
        out.push_str("no executable produced\n");
        return out;
    };
    match Command::new(&exe).current_dir(&dir).output() {
        Ok(run) => {
            out.push_str(&String::from_utf8_lossy(&run.stdout));
            let err = String::from_utf8_lossy(&run.stderr);
            if !err.is_empty() {
                out.push_str(&err);
            }
        }
        Err(error) => out.push_str(&format!("\nfailed to run executable: {error}\n")),
    }
    out
}

unsafe fn update_gutter() {
    let edit = CONTROLS.edit;
    if edit == 0 {
        return;
    }
    let text = get_text(edit);
    let lines = text.split('\n').count();
    let mut out = String::new();
    for n in 1..=lines {
        out.push_str(&format!("{n:>4}\n"));
    }
    set_text(CONTROLS.gutter, &out);
}

unsafe fn update_caret() {
    let edit = CONTROLS.edit;
    if edit == 0 {
        return;
    }
    let caret = SendMessageA(edit, EM_LINEINDEX, (-1isize) as WPARAM, 0) as i32;
    let caret = caret.max(0);
    let line = SendMessageA(edit, EM_LINEFROMCHAR, caret as WPARAM, 0) as i32;
    let line_start = SendMessageA(edit, EM_LINEINDEX, line as WPARAM, 0) as i32;
    let col = caret - line_start;
    set_text(
        CONTROLS.status,
        &format!("Ln {}, Col {}    UTF-8    NEX", line + 1, col + 1),
    );
    update_gutter();
}

unsafe fn draw_run_button(draw: &DRAWITEMSTRUCT) {
    let dc = draw.h_dc;
    let rc = RECT {
        left: draw.rc_item.left,
        top: draw.rc_item.top,
        right: draw.rc_item.right,
        bottom: draw.rc_item.bottom,
    };
    let bg = if draw.item_state & 0x1 != 0 || RUN_HOVER {
        rgb(122, 174, 88)
    } else {
        rgb(106, 154, 76)
    };
    let brush = CreateSolidBrush(bg);
    FillRect(dc, &rc, brush);
    DeleteObject(brush);

    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, rgb(255, 255, 255));
    SelectObject(dc, FONT_SMALL);
    let text = b"\xe2\x96\xb6 Run\0";
    DrawTextA(
        dc,
        text.as_ptr(),
        -1,
        &rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
            let dc = w as HDC;
            let ctrl = l as HWND;
            if ctrl == CONTROLS.sidebar {
                SetTextColor(dc, rgb(169, 183, 198));
                SetBkColor(dc, rgb(49, 51, 53));
                SIDEBAR_BRUSH as LRESULT
            } else if ctrl == CONTROLS.gutter {
                SetTextColor(dc, rgb(133, 133, 133));
                SetBkColor(dc, rgb(43, 43, 43));
                EDIT_BRUSH as LRESULT
            } else {
                SetTextColor(dc, rgb(169, 183, 198));
                SetBkColor(dc, rgb(43, 43, 43));
                EDIT_BRUSH as LRESULT
            }
        }
        WM_CTLCOLORSTATIC => {
            let dc = w as HDC;
            let ctrl = l as HWND;
            if ctrl == CONTROLS.tab {
                // Active tab: editor background, brighter text.
                SetTextColor(dc, rgb(169, 183, 198));
                SetBkColor(dc, rgb(43, 43, 43));
                EDIT_BRUSH as LRESULT
            } else if ctrl == CONTROLS.status {
                // Status bar: toolbar grey, light text.
                SetTextColor(dc, rgb(169, 183, 198));
                SetBkColor(dc, rgb(60, 63, 65));
                TOOL_BRUSH as LRESULT
            } else {
                // Sidebar (tree): sidebar grey, dim text.
                SetTextColor(dc, rgb(169, 183, 198));
                SetBkColor(dc, rgb(49, 51, 53));
                SIDEBAR_BRUSH as LRESULT
            }
        }
        WM_ERASEBKGND => {
            let dc = w as HDC;
            let mut rc = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetClientRect(hwnd, &mut rc);
            FillRect(dc, &rc, BG_BRUSH);
            1
        }
        WM_DRAWITEM => {
            let draw = &*(l as *const DRAWITEMSTRUCT);
            if draw.ctl_id == BTN_RUN as u32 {
                draw_run_button(draw);
            }
            1
        }
        WM_MOUSEMOVE => {
            let x = (l & 0xFFFF) as i16 as i32;
            let y = ((l >> 16) & 0xFFFF) as i16 as i32;
            let rc = RUN_RECT;
            let inside = x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom;
            if inside != RUN_HOVER {
                RUN_HOVER = inside;
                InvalidateRect(CONTROLS.run, std::ptr::null(), 0);
            }
            DefWindowProcA(hwnd, msg, w, l)
        }
        WM_COMMAND => {
            let id = (w & 0xFFFF) as usize;
            let code = (w >> 16) as u32;
            if id == BTN_RUN && code == 0 {
                let code = get_text(CONTROLS.edit);
                let output = run_code(&code);
                set_text(CONTROLS.output, &output);
            } else if id == EDIT_ID && code == EN_CHANGE {
                update_caret();
            }
            0
        }
        WM_SIZE => {
            let width = (l & 0xFFFF) as i32;
            let height = ((l >> 16) & 0xFFFF) as i32;
            let toolbar_h = 44;
            let status_h = 26;
            let tab_h = 30;
            let out_h = 170;
            let gutter_w = 50;
            let sidebar_w = 190;
            if CONTROLS.edit != 0 {
                let main_x = sidebar_w;
                let main_w = width - main_x;
                // Toolbar strip across the top.
                // Sidebar with the project tree (full height under the toolbar).
                MoveWindow(
                    CONTROLS.sidebar,
                    0,
                    toolbar_h,
                    sidebar_w,
                    height - toolbar_h - status_h,
                    1,
                );
                // Tab header above the editor.
                MoveWindow(CONTROLS.tab, main_x, toolbar_h, main_w - 180, tab_h, 1);
                // Run button in the toolbar.
                let run_w = 110;
                MoveWindow(CONTROLS.run, width - run_w - 14, 8, run_w, 28, 1);
                RUN_RECT = RECT {
                    left: width - run_w - 14,
                    top: 8,
                    right: width - 14,
                    bottom: 36,
                };
                let edit_y = toolbar_h + tab_h;
                let edit_h = height - edit_y - status_h - out_h - 12;
                // Gutter + editor.
                MoveWindow(CONTROLS.gutter, main_x + 4, edit_y, gutter_w, edit_h, 1);
                MoveWindow(
                    CONTROLS.edit,
                    main_x + 4 + gutter_w + 4,
                    edit_y,
                    main_w - gutter_w - 16,
                    edit_h,
                    1,
                );
                // Console panel below the editor.
                MoveWindow(
                    CONTROLS.output,
                    main_x + 4,
                    edit_y + edit_h + 6,
                    main_w - 8,
                    out_h,
                    1,
                );
                // Status bar across the bottom.
                MoveWindow(CONTROLS.status, 0, height - status_h, width, status_h, 1);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcA(hwnd, msg, w, l),
    }
}

const EXAMPLE: &str = "struct Item:
    name: string
    price: i64

fn main():
    var cart: [Item] = []
    push(cart, Item(\"apple\", 10))
    push(cart, Item(\"bread\", 25))
    push(cart, Item(\"milk\", 18))
    var total = 0
    for it in cart:
        println(it.name + \": \" + to_string(it.price))
        total = total + it.price
    println(f\"items: {len(cart)}, total: {total}\")
";

fn main() {
    unsafe {
        let instance = GetModuleHandleA(std::ptr::null());
        let class_name = b"NexStudioWnd\0";
        let cursor = LoadCursorA(instance, IDC_ARROW as *const u8);

        BG_BRUSH = CreateSolidBrush(rgb(43, 43, 43));
        EDIT_BRUSH = CreateSolidBrush(rgb(43, 43, 43));
        TOOL_BRUSH = CreateSolidBrush(rgb(60, 63, 65));
        SIDEBAR_BRUSH = CreateSolidBrush(rgb(49, 51, 53));
        FONT_MONO = CreateFontA(
            -16,
            0,
            0,
            0,
            FW_NORMAL,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            DEFAULT_PITCH,
            b"Consolas\0".as_ptr(),
        );
        FONT_SMALL = CreateFontA(
            -15,
            0,
            0,
            0,
            FW_NORMAL,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            DEFAULT_PITCH,
            b"Segoe UI\0".as_ptr(),
        );

        let wc = WNDCLASSA {
            style: 0,
            lpfn_wnd_proc: Some(wnd_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: 0,
            h_cursor: cursor,
            hbr_background: BG_BRUSH,
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
        };
        RegisterClassA(&wc);

        let title = b"NEX Studio\0";
        let hwnd = CreateWindowExA(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            120,
            1100,
            740,
            0,
            0,
            instance,
            std::ptr::null_mut(),
        );
        if hwnd == 0 {
            return;
        }

        // Dark immersive title bar (Windows 10/11).
        let dark: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const c_void,
            4,
        );

        let class_edit = b"EDIT\0";

        // Sidebar: project tree (read-only, dark).
        let sidebar = CreateWindowExA(
            0,
            class_edit.as_ptr(),
            to_cstring("▾ NEX\n\n  main.nex\n  libs\n    mathx.nex\n    strx.nex\n    examples\n      demo.nex\n").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_MULTILINE | ES_READONLY | WS_VSCROLL,
            0, 44, 190, 300,
            hwnd,
            SIDEBAR_ID as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(sidebar, WM_SETFONT, FONT_SMALL as WPARAM, 1);

        // Tab header (active file).
        let class_status = b"STATIC\0";
        let tab = CreateWindowExA(
            0,
            class_status.as_ptr(),
            to_cstring("● main.nex").as_ptr(),
            WS_CHILD | WS_VISIBLE | ES_LEFT,
            190,
            44,
            400,
            30,
            hwnd,
            TAB_ID as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(tab, WM_SETFONT, FONT_SMALL as WPARAM, 1);

        let gutter = CreateWindowExA(
            0,
            class_edit.as_ptr(),
            b"   1\0".as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_MULTILINE | ES_READONLY | ES_CENTER,
            8,
            46,
            56,
            300,
            hwnd,
            GUTTER_ID as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(gutter, WM_SETFONT, FONT_MONO as WPARAM, 1);

        let edit = CreateWindowExA(
            0,
            class_edit.as_ptr(),
            to_cstring(EXAMPLE).as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_BORDER
                | ES_MULTILINE
                | ES_AUTOVSCROLL
                | ES_WANTRETURN
                | WS_VSCROLL
                | WS_HSCROLL,
            68,
            46,
            400,
            300,
            hwnd,
            EDIT_ID as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(edit, WM_SETFONT, FONT_MONO as WPARAM, 1);

        let output = CreateWindowExA(
            0,
            class_edit.as_ptr(),
            to_cstring("Ready. Press Run to compile and execute.").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_MULTILINE | ES_READONLY | WS_VSCROLL,
            8,
            360,
            400,
            180,
            hwnd,
            0,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(output, WM_SETFONT, FONT_MONO as WPARAM, 1);

        let class_btn = b"BUTTON\0";
        let run = CreateWindowExA(
            0,
            class_btn.as_ptr(),
            b"Run\0".as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
            900,
            8,
            110,
            30,
            hwnd,
            BTN_RUN as HMENU,
            instance,
            std::ptr::null_mut(),
        );

        let status = CreateWindowExA(
            0,
            class_status.as_ptr(),
            to_cstring("Ln 1, Col 1").as_ptr(),
            WS_CHILD | WS_VISIBLE | ES_LEFT,
            12,
            8,
            400,
            24,
            hwnd,
            STATUS_ID as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(status, WM_SETFONT, FONT_SMALL as WPARAM, 1);

        CONTROLS = Controls {
            edit,
            output,
            run,
            status,
            gutter,
            sidebar,
            tab,
        };

        // Dark status bar strip: colour it by handling WM_CTLCOLORSTATIC (status is a STATIC).
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        update_caret();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageA(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
    }
}
