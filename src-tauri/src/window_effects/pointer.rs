#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePoint {
    pub x: i32,
    pub y: i32,
}

pub const BANNER_WIDTH: i32 = 360;
pub const BANNER_HEIGHT: i32 = 48;

pub trait PointerLocator {
    fn pointer_position(&self) -> Result<NativePoint, String>;
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemPointerLocator;

#[cfg(windows)]
impl PointerLocator for SystemPointerLocator {
    fn pointer_position(&self) -> Result<NativePoint, String> {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut point = POINT::default();
        // SAFETY: GetCursorPos writes to the POINT owned by this stack frame.
        unsafe { GetCursorPos(&mut point) }
            .map_err(|error| error.to_string())
            .map(|()| NativePoint {
                x: point.x,
                y: point.y,
            })
    }
}

pub fn is_pointer_in_banner(
    visible: bool,
    overlay_top_left: NativePoint,
    pointer: NativePoint,
) -> bool {
    visible
        && pointer.x >= overlay_top_left.x
        && pointer.x < overlay_top_left.x + BANNER_WIDTH
        && pointer.y >= overlay_top_left.y
        && pointer.y < overlay_top_left.y + BANNER_HEIGHT
}

pub fn should_ignore_cursor_events(
    locator: &dyn PointerLocator,
    visible: bool,
    overlay_top_left: NativePoint,
) -> bool {
    if !visible {
        return true;
    }
    let in_banner = locator
        .pointer_position()
        .map(|pointer| is_pointer_in_banner(visible, overlay_top_left, pointer))
        .unwrap_or(false);
    !in_banner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_hit_test_obeys_visibility_and_half_open_boundaries() {
        let origin = NativePoint { x: -50, y: 100 };
        assert!(!is_pointer_in_banner(
            false,
            origin,
            NativePoint { x: -50, y: 100 }
        ));
        assert!(is_pointer_in_banner(
            true,
            origin,
            NativePoint { x: -50, y: 100 }
        ));
        assert!(is_pointer_in_banner(
            true,
            origin,
            NativePoint { x: 309, y: 147 }
        ));
        assert!(!is_pointer_in_banner(
            true,
            origin,
            NativePoint { x: 310, y: 147 }
        ));
        assert!(!is_pointer_in_banner(
            true,
            origin,
            NativePoint { x: 309, y: 148 }
        ));
    }

    struct FakeLocator(Result<NativePoint, String>);

    impl PointerLocator for FakeLocator {
        fn pointer_position(&self) -> Result<NativePoint, String> {
            self.0.clone()
        }
    }

    #[test]
    fn failed_pointer_lookup_restores_click_through() {
        let locator = FakeLocator(Err("lookup failed".to_string()));
        assert!(should_ignore_cursor_events(
            &locator,
            true,
            NativePoint { x: 10, y: 20 }
        ));
    }

    #[test]
    fn pointer_decision_maps_banner_hit_to_setter_argument() {
        let origin = NativePoint { x: 10, y: 20 };
        let inside = FakeLocator(Ok(NativePoint { x: 10, y: 20 }));
        let outside = FakeLocator(Ok(NativePoint { x: 370, y: 20 }));
        assert!(!should_ignore_cursor_events(&inside, true, origin));
        assert!(should_ignore_cursor_events(&outside, true, origin));
        assert!(should_ignore_cursor_events(&inside, false, origin));
    }

    struct PanicLocator;

    impl PointerLocator for PanicLocator {
        fn pointer_position(&self) -> Result<NativePoint, String> {
            panic!("hidden banner must not query the pointer")
        }
    }

    #[test]
    fn hidden_banner_restores_click_through_without_pointer_query() {
        assert!(should_ignore_cursor_events(
            &PanicLocator,
            false,
            NativePoint { x: 10, y: 20 }
        ));
    }
}
