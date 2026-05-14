//! iTunes COM bridge.
//!
//! Classic iTunes for Windows doesn't expose itself to SMTC, so we bridge it
//! via its COM automation interface (`iTunes.Application`). We spawn a tiny
//! PowerShell child process at startup that connects to iTunes via COM and
//! prints one JSON line per second to stdout. We parse those lines and emit
//! the same Tauri events the SMTC reader does.
//!
//! SMTC has priority — iTunes events are suppressed whenever SMTC has an
//! active session (e.g. when Spotify is playing, ignore iTunes).

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::smtc::{CurrentTrack, PlaybackState, SharedSnapshot};

const SCRIPT: &str = include_str!("../scripts/itunes_poll.ps1");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Deserialize, Default)]
struct Line {
    #[serde(default)]
    present: bool,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    position_ms: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

pub fn start(app: AppHandle, snapshot: SharedSnapshot, smtc_playing: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app, snapshot, smtc_playing).await {
            eprintln!("[itunes] worker exited: {e:#}");
        }
    });
}

async fn run(
    app: AppHandle,
    snapshot: SharedSnapshot,
    smtc_playing: Arc<AtomicBool>,
) -> Result<()> {
    // Stage the script to a UNIQUE temp file per process. The previous fixed
    // path (%TEMP%\lyric-overlay-itunes-poll.ps1) was writable by any process
    // running as the same user, opening a TOCTOU window between fs::write and
    // PowerShell's -File read. NamedTempFile's random suffix closes that, and
    // the guard auto-deletes on drop.
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .prefix("lyric-overlay-itunes-")
        .suffix(".ps1")
        .tempfile()
        .context("create temp script file")?;
    tmp.as_file_mut()
        .write_all(SCRIPT.as_bytes())
        .context("write itunes poll script")?;
    tmp.as_file_mut().flush().context("flush itunes poll script")?;
    let script_path = tmp.path().to_path_buf();

    // kill_on_drop ensures the PowerShell child exits when this future drops
    // for any reason (cancellation, error return). Without it, the previous
    // BUGS.md "orphan PowerShell on hot-reload" was guaranteed.
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .spawn()
        .context("spawn powershell for iTunes poll")?;
    // _tmp_guard keeps the temp script alive while the child reads it. When
    // run() returns, the guard drops, Tokio kills the child (kill_on_drop),
    // and the script is removed from disk.
    let _tmp_guard = tmp;

    let stdout = child.stdout.take().context("no stdout from powershell")?;
    let mut lines = BufReader::new(stdout).lines();

    let mut last_emitted_title: Option<String> = None;
    let mut last_emitted_state: PlaybackState = PlaybackState::Unknown;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line.context("read itunes stdout")? {
                    Some(l) => l,
                    None => {
                        eprintln!("[itunes] poller exited (stdout closed)");
                        break;
                    }
                };
                if line.trim().is_empty() { continue; }

                let parsed: Line = match serde_json::from_str(&line) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("[itunes] bad json: {e} :: {line}");
                        continue;
                    }
                };

                if let Some(err) = &parsed.error {
                    eprintln!("[itunes] poller reported error: {err}");
                }

                // SMTC has priority while it's actively playing. Otherwise
                // iTunes wins (e.g. Chrome's stale-paused SMTC session shouldn't
                // mute iTunes).
                if smtc_playing.load(Ordering::Relaxed) {
                    continue;
                }

                if !parsed.present {
                    // iTunes isn't running. If we previously had iTunes data,
                    // clear the snapshot and notify.
                    if last_emitted_title.is_some() {
                        let mut snap = snapshot.write().await;
                        *snap = CurrentTrack::default();
                        let _ = app.emit("track-changed", &*snap);
                        let _ = app.emit("playback-state-changed", &*snap);
                        last_emitted_title = None;
                        last_emitted_state = PlaybackState::Unknown;
                    }
                    continue;
                }

                let state = parse_state(parsed.state.as_deref());
                let title = parsed.title.unwrap_or_default();
                let artist = parsed.artist.unwrap_or_default();
                let album = parsed.album.unwrap_or_default();
                let duration_ms = parsed.duration_ms.unwrap_or(0);
                let position_ms = parsed.position_ms.unwrap_or(0);
                let now_ms = unix_ms_now();

                let track_changed = last_emitted_title.as_deref() != Some(title.as_str());
                let state_changed = last_emitted_state != state;

                {
                    let mut snap = snapshot.write().await;
                    snap.title = title.clone();
                    snap.artist = artist;
                    snap.album = album;
                    snap.duration_ms = duration_ms;
                    snap.position_ms = position_ms;
                    snap.last_update_unix_ms = now_ms;
                    snap.state = state;
                    snap.source_app_id = Some("iTunes (COM)".into());
                }

                let snap_ro = snapshot.read().await;
                if track_changed {
                    let _ = app.emit("track-changed", &*snap_ro);
                }
                let _ = app.emit("timeline-changed", &*snap_ro);
                if state_changed {
                    let _ = app.emit("playback-state-changed", &*snap_ro);
                }

                last_emitted_title = Some(title);
                last_emitted_state = state;
            }
            status = child.wait() => {
                eprintln!("[itunes] powershell child exited: {status:?}");
                break;
            }
        }
    }

    Ok(())
}

fn parse_state(s: Option<&str>) -> PlaybackState {
    match s {
        Some("playing") => PlaybackState::Playing,
        Some("paused") => PlaybackState::Paused,
        Some("stopped") => PlaybackState::Stopped,
        _ => PlaybackState::Unknown,
    }
}

fn unix_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
