//! Creates / closes the artist-info peer window on demand.
//! Window label: "artist-info"
//! URL: "artist-panel/index.html" (Vite multi-page entry added in Task 11)

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

/// Open the artist-info panel window.
/// If a window with label "artist-info" already exists, focus it instead.
/// Position: anchored below the "overlay" window (center-aligned horizontally).
/// If less than 500px of screen below the overlay, anchors above instead.
pub async fn open_artist_panel(app: AppHandle) -> Result<()> {
    // If already open, just focus it.
    if let Some(existing) = app.get_webview_window("artist-info") {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // Compute position relative to the overlay window.
    let (x, y) = compute_panel_position(&app)?;

    // URL note: Vite's multi-page setup preserves the input path under
    // `dist/`, so the artist-panel entry ends up at `dist/src/artist-panel/
    // index.html` in production and is served at `/src/artist-panel/
    // index.html` by the dev server. Without the `src/` prefix the webview
    // 404s, Tauri falls back to the root `index.html`, main.tsx's
    // `pickComponent()` sees the unknown `artist-info` window label and
    // defaults to `DevConsole` — which is the "blank black window with no
    // visible content and dev-console title" failure mode you can hit if
    // this URL drifts.
    let window = WebviewWindowBuilder::new(
        &app,
        "artist-info",
        WebviewUrl::App("src/artist-panel/index.html".into()),
    )
    .title("Artist Info")
    .inner_size(360.0, 480.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .resizable(false)
    .skip_taskbar(true)
    .position(x as f64, y as f64)
    .build()?;

    // Mirror the overlay's window_backdrop setting onto this peer window so
    // the visual matches (Mica / Acrylic / Tabbed Mica / None). v0.10.23
    // backdrop machinery in `crate::backdrop` does the DWM call; we just
    // read the current kind from the persisted Settings state.
    #[cfg(windows)]
    {
        let settings_state = app.state::<crate::settings::SharedSettings>();
        let kind = settings_state.inner().blocking_read().window_backdrop;
        if let Ok(raw_hwnd) = window.hwnd() {
            let hwnd = HWND(raw_hwnd.0);
            if let Err(e) = crate::backdrop::apply_backdrop(hwnd, kind) {
                eprintln!("[artist_window] apply_backdrop failed: {e:#}");
            }
        }
    }

    let _ = window.show();

    // Listen for track-changed: auto-close when artist changes.
    // We capture the artist name at open time from get_current_track.
    let app_for_listener = app.clone();
    let open_artist = {
        let snap = app.state::<crate::smtc::SharedSnapshot>();
        let track = snap.read().await;
        crate::lyrics::clean_artist(&track.artist)
    };

    let listener_id = app.listen("track-changed", move |event| {
        // Parse the new track's artist from the event payload.
        if let Ok(track) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let new_artist = track
                .get("artist")
                .and_then(|a| a.as_str())
                .map(crate::lyrics::clean_artist)
                .unwrap_or_default();
            if !new_artist.eq_ignore_ascii_case(&open_artist) {
                if let Some(w) = app_for_listener.get_webview_window("artist-info") {
                    let _ = w.close();
                }
            }
        }
    });

    // Unregister the track-changed listener when the panel window is destroyed
    // so multiple open/close cycles don't accumulate stale listeners.
    let app_for_destroy = app.clone();
    window.on_window_event(move |evt| {
        if matches!(evt, tauri::WindowEvent::Destroyed) {
            app_for_destroy.unlisten(listener_id);
        }
    });

    Ok(())
}

/// Close the artist-info panel window if it is open.
pub fn close_artist_panel(app: &AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window("artist-info") {
        w.close()?;
    }
    Ok(())
}

/// Compute the (x, y) screen position for the artist-info window.
/// Centers horizontally on the overlay; anchors 8px below (or above if near screen bottom).
fn compute_panel_position(app: &AppHandle) -> Result<(i32, i32)> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or_else(|| anyhow!("overlay window not found"))?;

    let pos = overlay.outer_position()?;
    let size = overlay.outer_size()?;

    // Panel is 360px wide. Center it on the overlay.
    let center_x = pos.x + (size.width as i32) / 2 - 180;

    // 480px tall panel. Check if it fits below.
    let below_y = pos.y + size.height as i32 + 8;

    // Get monitor height to decide above/below.
    let monitor_height = overlay
        .current_monitor()?
        .map(|m| m.size().height as i32)
        .unwrap_or(1080);

    let y = if below_y + 480 <= monitor_height {
        below_y
    } else {
        pos.y - 488
    };

    // Clamp to screen top.
    let y = y.max(0);
    // Clamp x to screen left (rough guard — no right-side clamp needed for most setups).
    let x = center_x.max(0);

    Ok((x, y))
}

// ── Tauri commands ─────────────────────────────────────────────────────────

/// Allowed URL hosts for ticket / artist links. Defends against cache-poisoning
/// with malformed or malicious URLs.
///
/// Exact-host entries match literally. The special `.go.impact.com` entry is
/// matched as a suffix to cover Impact tracking subdomains (e.g.
/// `abc.go.impact.com`) without opening a broad TLD wildcard.
const TICKET_URL_WHITELIST: &[&str] = &[
    "ticketmaster.com",
    "www.ticketmaster.com",
    "ticketmaster.ca",
    "www.ticketmaster.ca",
    "ticketmaster.co.uk",
    "www.ticketmaster.co.uk",
    "ticketmaster.de",
    "www.ticketmaster.de",
    // Impact tracking subdomain space — matched via ends_with below.
    ".go.impact.com",
    "seatgeek.com",
    "www.seatgeek.com",
    "axs.com",
    "www.axs.com",
    "livenation.com",
    "www.livenation.com",
    "last.fm",
    "www.last.fm",
    "theaudiodb.com",
    "www.theaudiodb.com",
    "musicbrainz.org",
    "www.musicbrainz.org",
];

#[tauri::command]
pub fn open_ticket_url(url: String) -> Result<(), String> {
    // Parse and whitelist-check the host. Also enforce https scheme as
    // defense-in-depth against non-browser protocol handler abuse.
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("URL scheme '{}' is not https", parsed.scheme()));
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let allowed = TICKET_URL_WHITELIST.iter().any(|entry| {
        if let Some(suffix) = entry.strip_prefix('.') {
            // Subdomain wildcard entry: match if host equals the bare domain
            // (exact) OR has it as a suffix (any subdomain). E.g. `.go.impact.com`
            // matches `go.impact.com` and `abc.go.impact.com`.
            host == suffix || host.ends_with(entry)
        } else {
            host == *entry
        }
    });
    if !allowed {
        return Err(format!("URL host '{host}' is not on the ticket link whitelist"));
    }
    opener::open(&url).map_err(|e| format!("open_ticket_url failed: {e}"))
}

#[tauri::command]
pub async fn open_artist_panel_cmd(app: AppHandle) -> Result<(), String> {
    open_artist_panel(app).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_artist_panel_cmd(app: AppHandle) -> Result<(), String> {
    close_artist_panel(&app).map_err(|e| e.to_string())
}
