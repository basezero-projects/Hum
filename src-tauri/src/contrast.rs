//! Background-luminance worker for the auto-contrast feature.
//!
//! Periodically (every 2s) samples a small strip of pixels just outside the
//! overlay window via `xcap` desktop capture, computes average luminance,
//! and emits a `bg-luminance` Tauri event with the value 0..1 plus the
//! averaged RGB. The frontend listens and inverts text color when
//! `settings.auto_contrast` is on (light bg → dark text, dark bg → light).
//!
//! Sampling OUTSIDE the overlay (just below it, falling back to above if
//! that's off-screen) avoids any feedback loop where the overlay's own
//! pixels — including the lyric text glyphs — would skew the read.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;
use xcap::Monitor;

#[derive(Clone, Serialize, Debug)]
pub struct BgLuminance {
    /// 0.0 = pure black, 1.0 = pure white. Computed via the standard
    /// `0.299 R + 0.587 G + 0.114 B` luma weighting.
    pub luminance: f32,
    /// Averaged RGB of the sampled patch, mainly for debugging / future
    /// tinting features. Frontend currently only uses `luminance`.
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const POLL_INTERVAL_MS: u64 = 2000;
const SAMPLE_HEIGHT: u32 = 30;
const SAMPLE_WIDTH_CAP: u32 = 240;
const SAMPLE_GAP_PX: i32 = 20;

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[contrast] worker starting");
        loop {
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;

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

            // Sample a horizontal strip centered on the overlay's x-axis,
            // just below the window. Try above if below is off-screen.
            let sample_w = (size.width).min(SAMPLE_WIDTH_CAP);
            let sample_h = SAMPLE_HEIGHT;
            let sample_x = pos.x + ((size.width as i32) - (sample_w as i32)) / 2;
            let below_y = pos.y + (size.height as i32) + SAMPLE_GAP_PX;
            let above_y = pos.y - SAMPLE_GAP_PX - (sample_h as i32);

            let result = sample_at(sample_x, below_y, sample_w, sample_h)
                .or_else(|_| sample_at(sample_x, above_y, sample_w, sample_h));

            match result {
                Ok(payload) => {
                    let _ = app.emit("bg-luminance", &payload);
                }
                Err(e) => {
                    // Don't spam: log first failure per session, then go quiet.
                    static mut LOGGED_ONCE: bool = false;
                    // SAFETY: single tauri-async task accesses this; no races.
                    unsafe {
                        if !LOGGED_ONCE {
                            eprintln!("[contrast] sample failed (will keep retrying silently): {e:#}");
                            LOGGED_ONCE = true;
                        }
                    }
                }
            }
        }
    });
}

fn sample_at(x: i32, y: i32, w: u32, h: u32) -> Result<BgLuminance> {
    let monitors = Monitor::all().context("Monitor::all")?;

    // Find the monitor containing (x + w/2, y + h/2). xcap returns each
    // monitor's position in virtual-screen coords on Windows.
    let cx = x + (w as i32) / 2;
    let cy = y + (h as i32) / 2;
    let monitor = monitors
        .iter()
        .find(|m| {
            let mx = m.x().unwrap_or(0);
            let my = m.y().unwrap_or(0);
            let mw = m.width().unwrap_or(0) as i32;
            let mh = m.height().unwrap_or(0) as i32;
            cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
        })
        .context("no monitor contains sample center")?;

    let img = monitor.capture_image().context("capture_image")?;
    let mx = monitor.x().unwrap_or(0);
    let my = monitor.y().unwrap_or(0);

    // Convert sample rect to monitor-local coords + clamp to image bounds.
    let local_x = (x - mx).max(0) as u32;
    let local_y = (y - my).max(0) as u32;
    if local_x >= img.width() || local_y >= img.height() {
        anyhow::bail!("sample origin outside image");
    }
    let crop_w = w.min(img.width() - local_x);
    let crop_h = h.min(img.height() - local_y);
    if crop_w == 0 || crop_h == 0 {
        anyhow::bail!("zero-sized crop");
    }

    // Average RGB across the crop.
    let mut sr: u64 = 0;
    let mut sg: u64 = 0;
    let mut sb: u64 = 0;
    let mut n: u64 = 0;
    for py in local_y..(local_y + crop_h) {
        for px in local_x..(local_x + crop_w) {
            let p = img.get_pixel(px, py);
            sr += p[0] as u64;
            sg += p[1] as u64;
            sb += p[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        anyhow::bail!("no pixels sampled");
    }
    let r = (sr / n) as u8;
    let g = (sg / n) as u8;
    let b = (sb / n) as u8;
    let luminance =
        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
    Ok(BgLuminance { luminance, r, g, b })
}
