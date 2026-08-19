use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, WMSZ_BOTTOM, WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP,
    WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WM_SIZING,
};

pub fn set_aspect(_ratio: f64) {}

pub fn install(hwnd: HWND) {
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0) };
    if !ok.as_bool() {
        eprintln!("[aspect_lock] SetWindowSubclass failed");
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    if msg == WM_SIZING {
        let mut cur = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut cur) }.is_ok() {
            let cw = (cur.right - cur.left) as f64;
            let ch = (cur.bottom - cur.top) as f64;
            if ch > 0.0 && cw > 0.0 {
                let ratio = cw / ch;
                let rect = unsafe { &mut *(lparam.0 as *mut RECT) };
                let w = f64::from(rect.right - rect.left);
                match wparam.0 as u32 {
                    WMSZ_LEFT | WMSZ_RIGHT | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT => {
                        rect.bottom = rect.top + (w / ratio).round() as i32;
                    }
                    WMSZ_TOP | WMSZ_BOTTOM => {
                        let h = f64::from(rect.bottom - rect.top);
                        rect.right = rect.left + (h * ratio).round() as i32;
                    }
                    WMSZ_TOPLEFT | WMSZ_TOPRIGHT => {
                        rect.top = rect.bottom - (w / ratio).round() as i32;
                    }
                    _ => {}
                }
                return LRESULT(1);
            }
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
