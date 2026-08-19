#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    Left,
    Right,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    Other,
}

pub fn adjust_sizing_rect(
    current: NativeRect,
    mut requested: NativeRect,
    edge: ResizeEdge,
) -> NativeRect {
    let current_width = f64::from(current.right - current.left);
    let current_height = f64::from(current.bottom - current.top);
    if current_width <= 0.0 || current_height <= 0.0 {
        return requested;
    }

    let ratio = current_width / current_height;
    let requested_width = f64::from(requested.right - requested.left);
    match edge {
        ResizeEdge::Left | ResizeEdge::Right | ResizeEdge::BottomLeft | ResizeEdge::BottomRight => {
            requested.bottom = requested.top + (requested_width / ratio).round() as i32;
        }
        ResizeEdge::Top | ResizeEdge::Bottom => {
            let requested_height = f64::from(requested.bottom - requested.top);
            requested.right = requested.left + (requested_height * ratio).round() as i32;
        }
        ResizeEdge::TopLeft | ResizeEdge::TopRight => {
            requested.top = requested.bottom - (requested_width / ratio).round() as i32;
        }
        ResizeEdge::Other => {}
    }
    requested
}

pub fn should_handle_sizing(current: NativeRect) -> bool {
    current.right > current.left && current.bottom > current.top
}

#[cfg(windows)]
pub(super) fn install_native(hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
    use windows::Win32::UI::Shell::SetWindowSubclass;

    // SAFETY: The callback has the required system ABI and remains available for the process lifetime.
    let installed = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0) };
    if installed.as_bool() {
        Ok(())
    } else {
        Err("SetWindowSubclass failed".to_string())
    }
}

#[cfg(windows)]
unsafe extern "system" fn subclass_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT};
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, WMSZ_BOTTOM, WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT,
        WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WM_SIZING,
    };

    if msg == WM_SIZING {
        let mut current = RECT::default();
        // SAFETY: current is a valid writable RECT for this call.
        if unsafe { GetWindowRect(hwnd, &mut current) }.is_ok() {
            // SAFETY: WM_SIZING supplies lparam as a valid mutable RECT for the message duration.
            let requested = unsafe { &mut *(lparam.0 as *mut RECT) };
            let edge = match wparam.0 as u32 {
                WMSZ_LEFT => ResizeEdge::Left,
                WMSZ_RIGHT => ResizeEdge::Right,
                WMSZ_BOTTOMLEFT => ResizeEdge::BottomLeft,
                WMSZ_BOTTOMRIGHT => ResizeEdge::BottomRight,
                WMSZ_TOP => ResizeEdge::Top,
                WMSZ_BOTTOM => ResizeEdge::Bottom,
                WMSZ_TOPLEFT => ResizeEdge::TopLeft,
                WMSZ_TOPRIGHT => ResizeEdge::TopRight,
                _ => ResizeEdge::Other,
            };
            let adjusted = adjust_sizing_rect(
                NativeRect {
                    left: current.left,
                    top: current.top,
                    right: current.right,
                    bottom: current.bottom,
                },
                NativeRect {
                    left: requested.left,
                    top: requested.top,
                    right: requested.right,
                    bottom: requested.bottom,
                },
                edge,
            );
            requested.left = adjusted.left;
            requested.top = adjusted.top;
            requested.right = adjusted.right;
            requested.bottom = adjusted.bottom;
            if should_handle_sizing(NativeRect {
                left: current.left,
                top: current.top,
                right: current.right,
                bottom: current.bottom,
            }) {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: Forward all unhandled messages to the system subclass procedure.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: NativeRect = NativeRect {
        left: 10,
        top: 20,
        right: 410,
        bottom: 220,
    };
    const REQUESTED: NativeRect = NativeRect {
        left: 30,
        top: 40,
        right: 330,
        bottom: 340,
    };

    #[test]
    fn width_driven_bottom_edges_adjust_bottom() {
        for edge in [
            ResizeEdge::Left,
            ResizeEdge::Right,
            ResizeEdge::BottomLeft,
            ResizeEdge::BottomRight,
        ] {
            assert_eq!(
                adjust_sizing_rect(CURRENT, REQUESTED, edge),
                NativeRect {
                    bottom: 190,
                    ..REQUESTED
                }
            );
        }
    }

    #[test]
    fn height_driven_edges_adjust_right() {
        for edge in [ResizeEdge::Top, ResizeEdge::Bottom] {
            assert_eq!(
                adjust_sizing_rect(CURRENT, REQUESTED, edge),
                NativeRect {
                    right: 630,
                    ..REQUESTED
                }
            );
        }
    }

    #[test]
    fn top_corner_edges_adjust_top_from_width() {
        for edge in [ResizeEdge::TopLeft, ResizeEdge::TopRight] {
            assert_eq!(
                adjust_sizing_rect(CURRENT, REQUESTED, edge),
                NativeRect {
                    top: 190,
                    ..REQUESTED
                }
            );
        }
    }

    #[test]
    fn current_ratio_is_derived_for_each_adjustment() {
        let current = NativeRect {
            left: -20,
            top: 5,
            right: 280,
            bottom: 205,
        };
        assert_eq!(
            adjust_sizing_rect(current, REQUESTED, ResizeEdge::Right).bottom,
            240
        );
    }

    #[test]
    fn zero_sized_current_rectangles_leave_request_unchanged() {
        let zero_width = NativeRect {
            right: 10,
            ..CURRENT
        };
        let zero_height = NativeRect {
            bottom: 20,
            ..CURRENT
        };
        assert_eq!(
            adjust_sizing_rect(zero_width, REQUESTED, ResizeEdge::Right),
            REQUESTED
        );
        assert_eq!(
            adjust_sizing_rect(zero_height, REQUESTED, ResizeEdge::Right),
            REQUESTED
        );
    }

    #[test]
    fn valid_current_rect_is_handled_even_for_unknown_edge() {
        assert!(should_handle_sizing(CURRENT));
        assert_eq!(
            adjust_sizing_rect(CURRENT, REQUESTED, ResizeEdge::Other),
            REQUESTED
        );
    }

    #[test]
    fn zero_sized_current_rect_is_not_handled() {
        assert!(!should_handle_sizing(NativeRect {
            right: CURRENT.left,
            ..CURRENT
        }));
        assert!(!should_handle_sizing(NativeRect {
            bottom: CURRENT.top,
            ..CURRENT
        }));
    }
}
