//! NEX Studio (native) — a real desktop editor for the NEX language.
//!
//! Pure Rust + the Win32 API via FFI: no external crates, so it builds reliably
//! on this machine. It edits NEX source in a multiline edit control; on Run it
//! writes the buffer to a temp file, invokes the `nexc` compiler (NEX's own
//! Rust compiler) to build an executable, runs it, and shows stdout/stderr.
//! The runtime it exercises is the C `nex_rt.c`, so the app is Rust driving a
//! Rust + C toolchain — exactly the "our language + Rust + C/C++" stack.

#![windows_subsystem = "windows"]
// Win32 FFI is inherently unsafe and uses raw pointers / global mutable state;
// the standard lints do not apply to this style of code.
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
#![allow(clippy::type_complexity)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::manual_c_str_literals)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;

type HWND = isize;
type HINSTANCE = isize;
type HMENU = isize;
type HBRUSH = isize;
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
struct WNDCLASSA {
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: HINSTANCE,
    h_icon: HICON,
    h_cursor: HCURSOR,
    hbr_background: HBRUSH,
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
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetStockObject(index: i32) -> isize;
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

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_COMMAND: u32 = 0x0111;
const WM_SETFONT: u32 = 0x0030;

const SW_SHOW: i32 = 5;
const ANSI_FIXED_FONT: i32 = 11;
const IDC_ARROW: usize = 32512;

const BTN_RUN: usize = 1;

struct Controls {
    edit: HWND,
    output: HWND,
    run: HWND,
}

static mut CONTROLS: Controls = Controls {
    edit: 0,
    output: 0,
    run: 0,
};

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

/// Writes the code, compiles with `nexc`, runs it, and returns combined output.
fn run_code(code: &str, nexc: &PathBuf) -> String {
    let dir = std::env::temp_dir().join("nex_studio_native");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let source = dir.join("main.nex");
    if let Err(error) = std::fs::write(&source, code) {
        return format!("failed to write source: {error}\n");
    }

    let compile = Command::new(nexc)
        .arg("main.nex")
        .arg("--emit")
        .arg("exe")
        .current_dir(&dir)
        .output();

    let compile = match compile {
        Ok(output) => output,
        Err(error) => return format!("failed to run compiler: {error}\n"),
    };

    let mut out = String::from_utf8_lossy(&compile.stdout).to_string();
    out.push_str(&String::from_utf8_lossy(&compile.stderr));

    let exe = dir
        .join("build")
        .join("debug")
        .join(if cfg!(windows) { "main.exe" } else { "main" });

    if !compile.status.success() || !exe.is_file() {
        return out;
    }

    if let Ok(run) = Command::new(&exe).current_dir(&dir).output() {
        out.push_str(&String::from_utf8_lossy(&run.stdout));
        out.push_str(&String::from_utf8_lossy(&run.stderr));
    } else {
        out.push_str("\nfailed to run executable\n");
    }
    out
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (w & 0xFFFF) as usize;
            let code = (w >> 16) as u32;
            if id == BTN_RUN && code == 0 {
                if let Some(nexc) = locate_nexc() {
                    let code = get_text(CONTROLS.edit);
                    let output = run_code(&code, &nexc);
                    set_text(CONTROLS.output, &output);
                } else {
                    set_text(
                        CONTROLS.output,
                        "compiler `nexc` not found. Build it: cargo build -p nex-driver\n",
                    );
                }
            }
            0
        }
        WM_SIZE => {
            let width = (l & 0xFFFF) as i32;
            let height = ((l >> 16) & 0xFFFF) as i32;
            let run_h = 34;
            let out_h = 180;
            let edit_h = height - run_h - out_h - 24;
            if CONTROLS.edit != 0 {
                MoveWindow(CONTROLS.edit, 8, 8, width - 16, edit_h, 1);
                MoveWindow(CONTROLS.output, 8, 8 + edit_h + 8, width - 16, out_h, 1);
                MoveWindow(CONTROLS.run, 8, 8 + edit_h + 8 + out_h + 6, 120, 28, 1);
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

fn locate_nexc() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("NEXC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe_name = if cfg!(windows) { "nexc.exe" } else { "nexc" };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("../../target/debug").join(exe_name),
        manifest.join("../../target/release").join(exe_name),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
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
    println(\"total: \" + to_string(total))
";

fn main() {
    unsafe {
        let instance = GetModuleHandleA(std::ptr::null());
        let class_name = b"NexStudioWnd\0";

        let cursor = LoadCursorA(instance, IDC_ARROW as *const u8);

        let wc = WNDCLASSA {
            style: 0,
            lpfn_wnd_proc: Some(wnd_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: 0,
            h_cursor: cursor,
            hbr_background: 0,
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
            1000,
            720,
            0,
            0,
            instance,
            std::ptr::null_mut(),
        );
        if hwnd == 0 {
            return;
        }

        let font = GetStockObject(ANSI_FIXED_FONT);

        let class_edit = b"EDIT\0";
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
            8,
            8,
            400,
            300,
            hwnd,
            0,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(edit, WM_SETFONT, font as WPARAM, 1);

        let output = CreateWindowExA(
            0,
            class_edit.as_ptr(),
            to_cstring("Готов к работе. Нажми Run.").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | ES_MULTILINE | ES_READONLY | WS_VSCROLL,
            8,
            320,
            400,
            180,
            hwnd,
            0,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(output, WM_SETFONT, font as WPARAM, 1);

        let class_btn = b"BUTTON\0";
        let run = CreateWindowExA(
            0,
            class_btn.as_ptr(),
            b"Run\0".as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER,
            8,
            510,
            120,
            28,
            hwnd,
            BTN_RUN as HMENU,
            instance,
            std::ptr::null_mut(),
        );
        SendMessageA(run, WM_SETFONT, font as WPARAM, 1);

        CONTROLS = Controls { edit, output, run };

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageA(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
    }
}
