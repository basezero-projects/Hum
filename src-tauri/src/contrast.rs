//! Background-luminance worker for the auto-contrast feature.
//!
//! Periodically (every 2s) samples a small strip of pixels just outside the
//! overlay window via the platform screen sampler, computes average luminance,
//! and emits a `bg-luminance` Tauri event with the value 0..1 plus the
//! averaged RGB. The frontend listens and inverts text color when
//! `settings.auto_contrast` is on (light bg → dark text, dark bg → light).
//!
//! Sampling OUTSIDE the overlay (just below it, falling back to above if
//! that's off-screen) avoids any feedback loop where the overlay's own
//! pixels — including the lyric text glyphs — would skew the read.

use std::time::Duration;

use crate::window_effects::screen_sampler::{
    sample_overlay_background, OverlayBounds, SystemScreenSampler,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

const POLL_INTERVAL_MS: u64 = 2000;
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[contrast] worker starting");
        let sampler = SystemScreenSampler;
        loop {
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;

            // Respect the `auto_contrast` setting: when it's off, skip the
            // desktop capture entirely (no screen grab, no event) instead of
            // capturing every 2s and letting the frontend ignore the result.
            // This is the actual on/off switch for the feature — the screen
            // capture is the expensive (and privacy-relevant) part. Settings
            // may not be managed yet on the very first ticks; default to
            // running until it is (the setting itself defaults on).
            let settings_arc = app
                .try_state::<crate::settings::SharedSettings>()
                .map(|s| s.inner().clone());
            if let Some(settings) = settings_arc {
                if !settings.read().await.auto_contrast {
                    continue;
                }
            }

            let overlay = match app.get_webview_window("overlay") {
                Some(w) => w,
                None => continue,
            };
            // outer_position / outer_size include window decorations. The
            // overlay has decorations: false, so they equal the visible
            // bounds.
            let (pos, size) = match (overlay.outer_position(), overlay.outer_size()) {
                (Ok(p), Ok(s)) => (p, s),
                _ => continue,
            };

            let result = sample_overlay_background(
                &sampler,
                OverlayBounds {
                    x: pos.x,
                    y: pos.y,
                    width: size.width,
                    height: size.height,
                },
            );

            match result {
                Ok(payload) => {
                    let _ = app.emit("bg-luminance", &payload);
                }
                Err(e) => {
                    // Don't spam: log the first failure per session, then go
                    // quiet. AtomicBool rather than `static mut` — this async
                    // task suspends at `sleep().await` and can resume on a
                    // different runtime thread, so unsynchronized access would
                    // be a data race.
                    use std::sync::atomic::{AtomicBool, Ordering};
                    static LOGGED_ONCE: AtomicBool = AtomicBool::new(false);
                    if !LOGGED_ONCE.swap(true, Ordering::Relaxed) {
                        eprintln!("[contrast] sample failed (will keep retrying silently): {e:#}");
                    }
                }
            }
        }
    });
}
