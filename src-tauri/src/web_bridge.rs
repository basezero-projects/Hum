#![allow(dead_code)]
//! Web-player bridge — fills in track metadata for browser-based players
//! that don't expose `navigator.mediaSession.metadata` correctly to Windows
//! SMTC. Pandora.com is the motivating case: SMTC gets the browser tab
//! title ("Today's Hits Radio - Now Playing on Pandora") and the Chrome
//! favicon as thumbnail. The real song info lives only in Chrome's DOM.
//!
//! This module owns:
//! - The `WebPlayerProbe` trait — a small interface every supported
//!   no-Media-Session web player implements.
//! - The `PandoraProbe` impl (first concrete probe).
//! - A polling loop that activates only when a probe's `detects()` matches
//!   the current SMTC snapshot. When no probe matches, zero UIA calls
//!   fire — YouTube / Spotify / iTunes are never touched.
//! - A shared cache (`SharedWebBridge`) the lyrics resolver consults
//!   before falling back to the SMTC snapshot.
//!
//! The cache value is a `WebBridgeTrack` with a `last_seen_unix_ms`
//! timestamp. Resolver treats values older than ~5s as stale.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::smtc::SharedSnapshot;

#[derive(Clone, Debug, Serialize, Default)]
pub struct WebBridgeTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Identifier of the probe that wrote this entry, e.g. `"pandora-web"`.
    pub source: String,
    /// Unix epoch ms at the moment of the read. Consumers use this to
    /// decide staleness — typically anything older than 5_000ms is treated
    /// as not-present.
    pub last_seen_unix_ms: i64,
}

pub type SharedWebBridge = Arc<RwLock<Option<WebBridgeTrack>>>;

/// A probe for one specific web player that doesn't expose Media Session
/// metadata correctly. Probes are stateless — every method receives all
/// the inputs it needs.
pub trait WebPlayerProbe: Send + Sync {
    /// Short, stable identifier — used as the `source` field of the
    /// produced `WebBridgeTrack` and in logging.
    fn name(&self) -> &'static str;

    /// Fast gate: does the current SMTC snapshot look like our player?
    /// Must be cheap (string ops only) — runs on every snapshot tick.
    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool;

    /// Walk Chrome's UI Automation tree, extract the now-playing widget
    /// content. Returns `Ok(Some(...))` when a complete-enough read
    /// succeeds, `Ok(None)` when the probe ran but couldn't find the
    /// widget, `Err` for unexpected failures.
    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>>;
}

/// Quick check: does ANY registered probe think the current SMTC snapshot
/// is unreliable? The lyrics resolver uses this to decide whether to
/// surface `Status::Unsupported` when the bridge cache is empty/stale.
pub fn any_probe_detects(smtc_title: &str, smtc_app_id: &str) -> bool {
    PROBES.iter().any(|p| p.detects(smtc_title, smtc_app_id))
}

/// Concrete probe registry. Build-time static — new probes ship as new
/// entries in this slice.
static PROBES: &[&dyn WebPlayerProbe] = &[&PandoraProbe];

// PandoraProbe lives in this same module — see Task 4.
struct PandoraProbe;

impl WebPlayerProbe for PandoraProbe {
    fn name(&self) -> &'static str {
        "pandora-web"
    }

    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool {
        // Filled in in Task 3.
        let _ = (smtc_title, smtc_app_id);
        false
    }

    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        // Filled in in Task 5.
        Ok(None)
    }
}

/// Spawn the bridge worker. The worker watches the SMTC snapshot and,
/// when a probe matches, polls UIA every 2s. Idle (5s tick, zero UIA
/// calls) when no probe matches.
pub fn start(_app: AppHandle, _snapshot: SharedSnapshot, _shared: SharedWebBridge) {
    // Filled in in Task 6.
}

fn _silence_unused_app_emitter(app: &AppHandle) {
    // Keeps the import live until Task 6 wires the emitter. Will be removed.
    // AppHandle implements Emitter; we reference the trait bound here so the
    // import survives until Task 6 uses it for real.
    fn _assert_emitter<T: Emitter<tauri::Wry>>(_: &T) {}
    _assert_emitter(app);
}

fn _silence_unused_duration() {
    let _ = Duration::from_secs(1);
}
