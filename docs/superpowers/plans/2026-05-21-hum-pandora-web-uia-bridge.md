# Pandora Web UIA Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Hum surface real lyrics for songs playing on `pandora.com` in Chrome, with zero user setup, while leaving YouTube / Spotify / iTunes / every other source untouched.

**Architecture:** A new `web_bridge` Rust module hosts a trait-based registry of `WebPlayerProbe` implementations. Each probe inspects SMTC's reported title + source-app-id; when one matches (Pandora's signature is `app_id` containing `Chrome` AND title ending with `Now Playing on Pandora`), the probe walks Chrome's UI Automation tree, extracts the now-playing track/artist/album text, and writes it to a shared cache. The lyrics resolver consults this cache before the SMTC snapshot — when fresh bridge data is present it overrides SMTC's garbage title. When no fresh bridge data exists for a known-unreliable source, the resolver returns a new `Unsupported` status so the overlay shows `Pandora web — track info unavailable` instead of lying.

**Tech Stack:**
- Rust 2021, Tauri 2, tokio async (existing).
- `windows` crate 0.58 (already a dep, used by `smtc.rs`).
- `uiautomation` crate (new dep) — high-level safe wrapper around the Win32 UIA COM API. Saves ~500 lines of unsafe COM glue.

Spec reference: [`docs/superpowers/specs/2026-05-21-hum-pandora-web-uia-bridge-design.md`](../specs/2026-05-21-hum-pandora-web-uia-bridge-design.md).

---

## File Structure

**Files this plan creates:**
- `src-tauri/src/web_bridge.rs` — probe trait, `PandoraProbe`, `WebBridgeTrack` struct, polling loop. ~250 lines.

**Files this plan modifies:**
- `src-tauri/Cargo.toml` — add `uiautomation` dep, bump version to `0.10.22`.
- `package.json` — bump version to `0.10.22`.
- `src-tauri/tauri.conf.json` — bump version to `0.10.22`.
- `src-tauri/src/lib.rs` — `mod web_bridge;`, `app.manage(SharedWebBridge)`, `web_bridge::start(...)`.
- `src-tauri/src/lyrics.rs` — add `CachedLyrics::Unsupported`, `Status::Unsupported`, bridge consultation in `start()` main loop, `apply_outcome` mapping.
- `src/Overlay.tsx` — new branch in `statusLine()` for `unsupported` status.
- `docs/CHANGELOG.md` — v0.10.22 entry.

**File boundaries:**
- `web_bridge.rs` owns: probe definitions, the polling loop, UIA glue. Knows nothing about lyrics resolution.
- `lyrics.rs` owns: track resolution. Reads from the shared bridge cache; doesn't know which probe wrote it.
- `Overlay.tsx` owns: rendering. Treats `unsupported` as a status string, no knowledge of the underlying cause.

This isolation means a future SoundCloud probe lands as ~50 lines added to `web_bridge.rs` with zero edits to `lyrics.rs` or the frontend.

---

## Pre-flight

- [ ] **Step 0a: Verify clean working tree**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay && git status`
Expected: `nothing to commit, working tree clean`. If dirty, stash or commit before proceeding.

- [ ] **Step 0b: Verify the test suite is green right now**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay/src-tauri && cargo test --lib 2>&1 | tail -5`
Expected: `test result: ok. 8 passed`.

- [ ] **Step 0c: Verify clippy is clean (the bar this plan upholds)**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay/src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: `Finished` with no error lines. If there are warnings, **stop the plan and fix them first** — they should have been cleared in commit `137222d`.

---

## Task 1: Add the `uiautomation` dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1.1: Find the latest `uiautomation` crate version**

Run: `cargo search uiautomation --limit 1`
Expected: line like `uiautomation = "0.16.1"` (version may differ — use whatever's current).

- [ ] **Step 1.2: Add the dep to `[dependencies]` block in `src-tauri/Cargo.toml`**

Add this line to the `[dependencies]` block (anywhere alphabetical):

```toml
uiautomation = "0.16"
```

(Replace `0.16` with the major.minor returned by the search.)

- [ ] **Step 1.3: Compile**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors. The dep should resolve and any transitive Win32 features it pulls in should slot under the existing `windows` 0.58.

- [ ] **Step 1.4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add uiautomation crate for the Pandora web bridge"
```

---

## Task 2: `web_bridge.rs` skeleton — types, trait, module wiring

**Files:**
- Create: `src-tauri/src/web_bridge.rs`
- Modify: `src-tauri/src/lib.rs:1-20` (module declaration; exact location is wherever the other `mod xxx;` lines live).

- [ ] **Step 2.1: Create the new module file**

Create `src-tauri/src/web_bridge.rs` with these contents:

```rust
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
    let _: &dyn Emitter = app;
}

fn _silence_unused_duration() {
    let _ = Duration::from_secs(1);
}
```

(The two `_silence_unused_*` helpers are scaffolding so this skeleton compiles cleanly under `-D warnings` before Task 6 fills in the loop. They get removed in Task 6.)

- [ ] **Step 2.2: Register the module in `lib.rs`**

Open `src-tauri/src/lib.rs`. Find the existing module declarations near the top (search for `mod smtc;` or `mod lyrics;`). Add `mod web_bridge;` alphabetically alongside them.

For example, if you see:

```rust
mod lyrics;
mod mode;
mod settings;
mod smtc;
mod streamer;
```

Make it:

```rust
mod lyrics;
mod mode;
mod settings;
mod smtc;
mod streamer;
mod web_bridge;
```

- [ ] **Step 2.3: Verify it compiles under strict clippy**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay/src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: `Finished` with no warnings.

- [ ] **Step 2.4: Commit**

```bash
git add src-tauri/src/web_bridge.rs src-tauri/src/lib.rs
git commit -m "web_bridge: skeleton — probe trait, WebBridgeTrack, module hook-up"
```

---

## Task 3: `PandoraProbe::detects` (TDD)

**Files:**
- Modify: `src-tauri/src/web_bridge.rs:60-90` (the `PandoraProbe::detects` body and a new `#[cfg(test)] mod tests` block at the bottom of the file)

- [ ] **Step 3.1: Write the failing tests at the bottom of `web_bridge.rs`**

Append this to `src-tauri/src/web_bridge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// `app_id.contains("Chrome")` matches the real Chrome AUMID
    /// (`"Chrome.exe"` on most installs, `"MSEdge.exe"`-based hybrids on
    /// custom builds — we accept any Chromium-derived app since the UIA
    /// tree shape is identical). `app_id` is empty when SMTC didn't
    /// report a source — be tolerant.
    #[test]
    fn pandora_detects_real_chrome_pandora_session() {
        let p = PandoraProbe;
        assert!(p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Chrome.exe",
        ));
        assert!(p.detects(
            "Some Other Station - Now Playing on Pandora",
            "Google.Chrome",
        ));
    }

    #[test]
    fn pandora_rejects_non_chrome_apps() {
        let p = PandoraProbe;
        // Even if a desktop Pandora app set the title to match, we
        // don't activate the probe for non-Chrome sources — they
        // expose SMTC correctly and don't need DOM scraping.
        assert!(!p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Spotify.exe",
        ));
        assert!(!p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "",
        ));
    }

    #[test]
    fn pandora_rejects_non_pandora_titles_in_chrome() {
        let p = PandoraProbe;
        // YouTube in Chrome — must NOT match.
        assert!(!p.detects(
            "Rick Astley - Never Gonna Give You Up (Official Music Video)",
            "Chrome.exe",
        ));
        // Spotify Web in Chrome — must NOT match.
        assert!(!p.detects(
            "Bohemian Rhapsody · Queen - Spotify",
            "Chrome.exe",
        ));
        // Empty title in Chrome — must NOT match (idle browser tab).
        assert!(!p.detects("", "Chrome.exe"));
    }

    #[test]
    fn pandora_does_not_false_positive_on_word_pandora_elsewhere() {
        let p = PandoraProbe;
        // A YouTube video about Pandora's Box mythology, or a Spotify
        // album called Pandora. Title doesn't END with the canonical
        // Pandora-tab suffix — must NOT match.
        assert!(!p.detects(
            "Pandora's Box - Greek Mythology Explained",
            "Chrome.exe",
        ));
        assert!(!p.detects(
            "Pandora · Aerosmith - Spotify",
            "Chrome.exe",
        ));
    }

    #[test]
    fn any_probe_detects_aggregates_correctly() {
        assert!(any_probe_detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Chrome.exe",
        ));
        assert!(!any_probe_detects(
            "Rick Astley - Never Gonna Give You Up",
            "Chrome.exe",
        ));
    }
}
```

- [ ] **Step 3.2: Run the tests — they should all fail**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay/src-tauri && cargo test --lib web_bridge 2>&1 | tail -20`
Expected: 5 tests, all fail (the stub `detects()` returns `false` regardless, so `pandora_detects_real_chrome_pandora_session` and `any_probe_detects_aggregates_correctly` fail; the other three accidentally pass because they expect `false`).

Actually two of the five fail — the rejection tests pass against the stub. That's fine — the failing-test gate is met by the positive-case tests. **Confirm exactly two failures:**

Expected output excerpt:
```
test web_bridge::tests::pandora_detects_real_chrome_pandora_session ... FAILED
test web_bridge::tests::any_probe_detects_aggregates_correctly ... FAILED
test web_bridge::tests::pandora_rejects_non_chrome_apps ... ok
test web_bridge::tests::pandora_rejects_non_pandora_titles_in_chrome ... ok
test web_bridge::tests::pandora_does_not_false_positive_on_word_pandora_elsewhere ... ok
```

- [ ] **Step 3.3: Implement `PandoraProbe::detects`**

Replace the stub body in `web_bridge.rs` (the `fn detects(...) -> bool { let _ = ...; false }`):

```rust
    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool {
        // Chromium-derived browsers (Chrome, Edge, Brave, Opera) all
        // expose UIA trees identically. Match any AUMID that mentions
        // Chrome — covers the common case and a few derivatives.
        // Reject empty app_id outright (idle session / no source).
        if smtc_app_id.is_empty() || !smtc_app_id.contains("Chrome") {
            return false;
        }
        // Pandora's <title> element is always "{station name} - Now
        // Playing on Pandora". Match the suffix exactly — substring
        // matches would false-positive on song titles containing the
        // word "Pandora" (Aerosmith's "Pandora", Greek mythology, etc.).
        smtc_title.ends_with("Now Playing on Pandora")
    }
```

- [ ] **Step 3.4: Run the tests — all should pass**

Run: `cargo test --lib web_bridge 2>&1 | tail -10`
Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 3.5: Verify clippy still clean**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: `Finished` no warnings.

- [ ] **Step 3.6: Commit**

```bash
git add src-tauri/src/web_bridge.rs
git commit -m "web_bridge: PandoraProbe::detects + table-driven unit tests"
```

---

## Task 4: Chrome window enumeration helper

**Files:**
- Modify: `src-tauri/src/web_bridge.rs` (add helper functions above `PandoraProbe`)

This task introduces the Win32 calls needed to find Chrome windows by process name + title suffix. Pure plumbing — manually verified by a smoke-test print.

- [ ] **Step 4.1: Add Win32 imports + enumeration helper**

Near the top of `web_bridge.rs`, after the existing `use` block, add:

```rust
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
```

Then, above the `struct PandoraProbe;` line, add this helper:

```rust
/// Enumerate top-level Chrome windows whose title matches `predicate`.
/// Returns the `HWND` of each match. Used by probes to find the right
/// Chromium window when multiple tabs / multiple Chrome windows are open.
///
/// Multi-process Chrome: UIA queries against the top-level window handle
/// reach into whichever renderer process is hosting that window's content,
/// so we don't need to chase the per-tab child processes ourselves.
fn find_chrome_windows<F: Fn(&str) -> bool>(predicate: F) -> Vec<HWND> {
    struct Ctx<'a> {
        predicate: &'a dyn Fn(&str) -> bool,
        hits: Vec<HWND>,
    }

    let mut ctx = Ctx {
        predicate: &predicate,
        hits: Vec::new(),
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam was set to a valid &mut Ctx by the EnumWindows
        // caller. The reference outlives the synchronous EnumWindows call.
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1); // skip hidden, keep enumerating
        }

        let title = read_window_title(hwnd);
        if title.is_empty() || !(ctx.predicate)(&title) {
            return BOOL(1);
        }

        let process_name = read_process_name_for_window(hwnd);
        if process_name.eq_ignore_ascii_case("chrome.exe") {
            ctx.hits.push(hwnd);
        }
        BOOL(1)
    }

    let ctx_ptr: *mut Ctx = &mut ctx;
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(ctx_ptr as isize)) };

    ctx.hits
}

fn read_window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // GetWindowTextW returns the number of characters copied, NOT
    // including the null terminator. A return of 0 means either an
    // empty title or an error — either way we treat as empty.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn read_process_name_for_window(hwnd: HWND) -> String {
    let mut pid: u32 = 0;
    let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    let handle = match unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
    } {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    let mut name_buf = [0u16; 260];
    let n = unsafe {
        GetModuleBaseNameW(handle, None, &mut name_buf)
    };
    // Close the handle implicitly via the windows-rs `HANDLE` drop guard
    // … actually OpenProcess returns a raw `HANDLE`, not a guarded one.
    // Close explicitly to avoid leaking handles on every window we
    // enumerate.
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
    if n == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&name_buf[..n as usize])
    }
}
```

- [ ] **Step 4.2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`. If you hit "feature not enabled" errors for the `windows` crate, add the missing features to its `features = [...]` array in `Cargo.toml`. The likely additions:

```toml
"Win32_System_Threading",
"Win32_System_ProcessStatus",
"Win32_UI_WindowsAndMessaging",
```

Check the existing features array first — most of these are probably already pulled in by SMTC's needs.

- [ ] **Step 4.3: Smoke test — log discovered windows on startup**

Open `src-tauri/src/web_bridge.rs`. Replace the `start()` stub with a temporary diagnostic version:

```rust
pub fn start(_app: AppHandle, _snapshot: SharedSnapshot, _shared: SharedWebBridge) {
    tauri::async_runtime::spawn(async {
        // Smoke test — log all Chrome windows whose title ends with
        // "Now Playing on Pandora". Removed in Task 6.
        eprintln!("[web_bridge] startup smoke test running");
        let hwnds = find_chrome_windows(|t| t.ends_with("Now Playing on Pandora"));
        eprintln!("[web_bridge] found {} Pandora-titled Chrome windows", hwnds.len());
        for hwnd in &hwnds {
            eprintln!("[web_bridge]   HWND = {hwnd:?}");
        }
    });
}
```

- [ ] **Step 4.4: Wire `web_bridge::start` from `lib.rs` (temporary, refined in Task 6)**

Open `src-tauri/src/lib.rs`. Find where `smtc::start(...)` is called in the setup hook. Add immediately after it:

```rust
let shared_bridge: web_bridge::SharedWebBridge = std::sync::Arc::new(tokio::sync::RwLock::new(None));
app.manage(shared_bridge.clone());
web_bridge::start(app.handle().clone(), snapshot.clone(), shared_bridge);
```

(Use the same `snapshot` variable that `smtc::start` already references — likely named `snapshot` or `smtc_snapshot`. Match the existing local name.)

- [ ] **Step 4.5: Manual smoke verify**

1. Open Chrome, navigate to pandora.com, start playing any station.
2. Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay && pnpm tauri dev`
3. In Hum's dev console output, expect:
```
[web_bridge] startup smoke test running
[web_bridge] found 1 Pandora-titled Chrome windows
[web_bridge]   HWND = HWND(0x...)
```

If you see `0 Pandora-titled Chrome windows` while Pandora is actively playing, the enumeration is broken — debug before continuing.

- [ ] **Step 4.6: Stop the dev server**

Ctrl-C the `pnpm tauri dev` process.

- [ ] **Step 4.7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/web_bridge.rs src-tauri/src/lib.rs
git commit -m "web_bridge: Chrome window enumeration helper + smoke-test wiring"
```

---

## Task 5: `PandoraProbe::read` — walk the UIA tree

This is the manual-discovery step. We use Microsoft's `inspect.exe` UIA inspector to find the right element selectors for Pandora's now-playing widget. The selectors are then hard-coded into `PandoraProbe::read`.

**Files:**
- Modify: `src-tauri/src/web_bridge.rs` (replace the `read()` stub)

- [ ] **Step 5.1: Locate `inspect.exe`**

Run: `dir /b /s "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\inspect.exe" 2>nul | head -1`
Expected: a path like `C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\inspect.exe`.

If `inspect.exe` isn't installed, install the Windows SDK from https://developer.microsoft.com/en-us/windows/downloads/windows-sdk/ (Tools-only install is enough — under "Debugging Tools for Windows").

- [ ] **Step 5.2: Open Pandora in Chrome, play a track**

Make sure a song is actively playing with track/artist/album visible on the page.

- [ ] **Step 5.3: Launch `inspect.exe` and identify the now-playing widget**

Launch `inspect.exe`. In the top-right tool panel, switch the API mode to "UIA" (not MSAA). Then:

1. Hover the cursor over the **track title** on the Pandora page (in the screenshot example: "Man I Need").
2. Read the right pane. Note these properties:
   - `LocalizedControlType` (e.g. "text", "hyperlink", "button")
   - `Name` (the displayed text — should match the song title)
   - `AutomationId` (may be empty for React-generated nodes)
   - `ClassName`
3. Tab back to the inspect.exe tree view in the left pane — note the **chain of parent elements** from the document root down to the track title. Common pattern:
```
Window: "Today's Hits Radio - Now Playing on Pandora — Google Chrome"
  └ Pane: "Today's Hits Radio - Now Playing on Pandora"  (the document)
      └ ... (chrome UI)
      └ Group with role "main" or aria-labelledby
          └ ... (the now-playing card)
              └ Text or Hyperlink: "Man I Need"           ← track title
              └ Hyperlink: "Olivia Dean"                  ← artist
              └ Hyperlink or Text: "The Art of Loving"   ← album
```
4. Repeat for **artist** and **album** elements. Record their `LocalizedControlType` / `Name` / `ClassName` / `AutomationId`.

- [ ] **Step 5.4: Document the discovered selectors in a comment block**

In `web_bridge.rs`, above the `impl WebPlayerProbe for PandoraProbe` block, add this comment template and fill in the **Discovered** values with what inspect.exe showed you:

```rust
// ─── Pandora UIA selector reference ────────────────────────────────────────
//
// Discovered via inspect.exe against pandora.com Chrome window on 2026-05-21.
// Update when the Pandora redesign breaks selectors.
//
// Track title:
//   LocalizedControlType = "<FILL IN>"
//   ClassName            = "<FILL IN>"
//   AutomationId         = "<FILL IN>"  (may be empty)
//   Tree path            = Document > <FILL IN ANCESTOR CHAIN>
//
// Artist:
//   LocalizedControlType = "<FILL IN>"
//   ClassName            = "<FILL IN>"
//   AutomationId         = "<FILL IN>"  (may be empty)
//
// Album:
//   LocalizedControlType = "<FILL IN>"
//   ClassName            = "<FILL IN>"
//   AutomationId         = "<FILL IN>"  (may be empty)
//
// Selector strategy: anchor on the most stable attribute (AutomationId
// when present, ClassName otherwise). Avoid Name matching since Name
// IS the song title we're trying to extract — circular.
```

- [ ] **Step 5.5: Implement `read()` based on the discovered selectors**

Replace the stub `read()` body in `PandoraProbe`:

```rust
    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        use uiautomation::UIAutomation;

        let automation = UIAutomation::new()
            .map_err(|e| anyhow::anyhow!("UIAutomation::new failed: {e:?}"))?;

        // Find Chrome windows whose title is Pandora's.
        let hwnds = find_chrome_windows(|t| t.ends_with("Now Playing on Pandora"));
        if hwnds.is_empty() {
            return Ok(None);
        }

        // Try each matching window — first one that yields a clean read wins.
        for hwnd in hwnds {
            // SAFETY: HWND is a raw pointer-sized handle. The
            // uiautomation crate accepts it as a window handle.
            let root = match automation.element_from_handle(hwnd.into()) {
                Ok(elem) => elem,
                Err(_) => continue,
            };

            // Build matchers for the three elements. The exact builder
            // calls depend on the inspect.exe findings above. EXAMPLE
            // pattern (adjust per actual selectors discovered):
            let title_elem = match automation
                .create_matcher()
                .from(root.clone())
                .classname("<FILL IN — track title ClassName>")
                .timeout(500)
                .find_first()
            {
                Ok(e) => e,
                Err(_) => continue,
            };
            let title = title_elem.get_name().unwrap_or_default();

            let artist_elem = automation
                .create_matcher()
                .from(root.clone())
                .classname("<FILL IN — artist ClassName>")
                .timeout(500)
                .find_first();
            let artist = artist_elem
                .map(|e| e.get_name().unwrap_or_default())
                .unwrap_or_default();

            let album_elem = automation
                .create_matcher()
                .from(root.clone())
                .classname("<FILL IN — album ClassName>")
                .timeout(500)
                .find_first();
            let album = album_elem
                .map(|e| e.get_name().unwrap_or_default())
                .unwrap_or_default();

            if title.trim().is_empty() {
                // Title couldn't be read — try next window or bail.
                continue;
            }

            let now_unix_ms = chrono::Utc::now().timestamp_millis();
            return Ok(Some(WebBridgeTrack {
                title: title.trim().to_string(),
                artist: artist.trim().to_string(),
                album: album.trim().to_string(),
                source: self.name().to_string(),
                last_seen_unix_ms: now_unix_ms,
            }));
        }

        Ok(None)
    }
```

Notes:
- `chrono` may or may not already be a Hum dep. If `cargo check` complains, replace the timestamp line with `std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)`.
- The exact matcher API depends on the `uiautomation` crate version. Refer to https://docs.rs/uiautomation/latest/uiautomation/ if `create_matcher().classname()` isn't the right chain — common alternatives are `name()`, `localized_control_type()`, `automation_id()`.

- [ ] **Step 5.6: Fill in the `<FILL IN>` placeholders with the ClassName / AutomationId values you noted in Step 5.4**

- [ ] **Step 5.7: Manual verify — read smoke test**

Add a temporary smoke-test call to `web_bridge::start()` (above the existing `find_chrome_windows` log):

```rust
pub fn start(_app: AppHandle, _snapshot: SharedSnapshot, _shared: SharedWebBridge) {
    tauri::async_runtime::spawn(async {
        // Wait 5s so Chrome is fully loaded before the first probe.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        eprintln!("[web_bridge] running Pandora read smoke test");
        match PandoraProbe.read() {
            Ok(Some(track)) => eprintln!(
                "[web_bridge] PandoraProbe SUCCESS: title={:?} artist={:?} album={:?}",
                track.title, track.artist, track.album
            ),
            Ok(None) => eprintln!("[web_bridge] PandoraProbe returned None (no window or no element)"),
            Err(e) => eprintln!("[web_bridge] PandoraProbe error: {e:#}"),
        }
    });
}
```

Run: `pnpm tauri dev`
With Pandora playing in Chrome, expect after ~5s:
```
[web_bridge] PandoraProbe SUCCESS: title="Man I Need" artist="Olivia Dean" album="The Art of Loving"
```

If `Ok(None)` or `Err`: re-run `inspect.exe`, re-verify the selectors. The probe MUST work end-to-end against a live Pandora session before continuing.

- [ ] **Step 5.8: Stop the dev server, commit**

```bash
git add src-tauri/src/web_bridge.rs
git commit -m "web_bridge: PandoraProbe::read — UIA tree walk for Chrome Pandora tab"
```

---

## Task 6: Polling loop with idle/active branches + event emission

**Files:**
- Modify: `src-tauri/src/web_bridge.rs` (`start()` body)

Replaces the smoke-test versions with the real polling loop.

- [ ] **Step 6.1: Replace `web_bridge::start` with the real implementation**

In `web_bridge.rs`, replace the `pub fn start(...)` body and remove the `_silence_unused_*` helpers:

```rust
pub fn start(app: AppHandle, snapshot: SharedSnapshot, shared: SharedWebBridge) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[web_bridge] worker starting");
        let mut last_emitted_title = String::new();

        loop {
            let (title, app_id) = {
                let snap = snapshot.read().await;
                let id = snap.source_app_id.clone().unwrap_or_default();
                (snap.title.clone(), id)
            };

            let active_probe: Option<&'static dyn WebPlayerProbe> = PROBES
                .iter()
                .find(|p| p.detects(&title, &app_id))
                .copied();

            match active_probe {
                Some(probe) => {
                    let name = probe.name();
                    let read_result = tokio::task::spawn_blocking(move || probe.read())
                        .await;
                    match read_result {
                        Ok(Ok(Some(track))) => {
                            let new_title = track.title.clone();
                            {
                                let mut w = shared.write().await;
                                *w = Some(track);
                            }
                            if new_title != last_emitted_title {
                                eprintln!(
                                    "[web_bridge] probe={name} read title={new_title:?}, emitting web-bridge-updated"
                                );
                                last_emitted_title = new_title;
                                // Dedicated event so SMTC's `track-changed`
                                // semantics (payload = full CurrentTrack)
                                // stay clean. lyrics::start subscribes to
                                // both events; web-bridge-updated is just
                                // a wake signal with `()` payload.
                                let _ = app.emit("web-bridge-updated", ());
                            }
                        }
                        Ok(Ok(None)) => {
                            // Probe ran, found nothing — leave existing cache
                            // alone. Resolver staleness check (5s) handles
                            // expiration if subsequent reads also fail.
                        }
                        Ok(Err(e)) => {
                            eprintln!("[web_bridge] probe={name} read error: {e:#}");
                        }
                        Err(join_err) => {
                            eprintln!("[web_bridge] probe={name} spawn_blocking failed: {join_err:#}");
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                None => {
                    // No probe matches the current SMTC snapshot. Idle —
                    // zero UIA calls fire. Wake periodically to re-check.
                    if !last_emitted_title.is_empty() {
                        // Just transitioned out of an active probe; clear the
                        // stale cache so the resolver doesn't keep using
                        // last-known Pandora data after the user switched
                        // tabs to YouTube.
                        *shared.write().await = None;
                        last_emitted_title.clear();
                        eprintln!("[web_bridge] no probe matches, cache cleared");
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });
}
```

- [ ] **Step 6.2: Remove the now-unused `_silence_unused_*` helpers from Task 2**

Delete the two `fn _silence_unused_*` functions added in Task 2.

- [ ] **Step 6.3: Verify compile + clippy clean**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: `Finished` no warnings.

- [ ] **Step 6.4: Manual integration smoke**

Run: `pnpm tauri dev`
With Pandora playing in Chrome, watch for:
```
[web_bridge] worker starting
[web_bridge] probe=pandora-web read title="Man I Need", emitting track-changed
```
Switch to a different Chrome tab playing YouTube. Within ~5s, expect:
```
[web_bridge] no probe matches, cache cleared
```
Switch back to Pandora. Within ~5s, the probe wakes up again and emits a new title.

- [ ] **Step 6.5: Stop dev server, commit**

```bash
git add src-tauri/src/web_bridge.rs
git commit -m "web_bridge: polling loop with active/idle branches + track-changed emit"
```

---

## Task 7: `Unsupported` status — Rust side

**Files:**
- Modify: `src-tauri/src/lyrics.rs` — `CachedLyrics` enum, `Status` enum, `to_cached` / `to_cached_ref` / `apply_outcome` switch arms, possibly `write_store`.

- [ ] **Step 7.1: Add `Unsupported` to `CachedLyrics`**

Open `src-tauri/src/lyrics.rs`. Find:

```rust
pub enum CachedLyrics {
    NotFound,
    Instrumental,
    Plain {
        text: String,
    },
    Synced { ... },
}
```

Add a new variant between `NotFound` and `Instrumental`:

```rust
pub enum CachedLyrics {
    NotFound,
    /// The source publishes audio but doesn't expose track metadata in any
    /// form Hum can read (e.g. Pandora web with no UIA selector match).
    /// Renders as a clear "source-specific reason" message rather than
    /// the generic "no lyrics for <garbage tab title>" output.
    Unsupported,
    Instrumental,
    Plain { ... },
    Synced { ... },
}
```

- [ ] **Step 7.2: Add `Unsupported` to `Status`**

In the same file, find:

```rust
pub enum Status {
    #[default]
    Idle,
    Fetching,
    Synced,
    Plain,
    Instrumental,
    NotFound,
    Error,
}
```

Add `Unsupported` between `NotFound` and `Error`:

```rust
pub enum Status {
    #[default]
    Idle,
    Fetching,
    Synced,
    Plain,
    Instrumental,
    NotFound,
    Unsupported,
    Error,
}
```

- [ ] **Step 7.3: Update `to_cached` and `to_cached_ref`**

Find both functions (line ~823 `to_cached_ref` and ~842 `to_cached`). They both end with `CachedLyrics::NotFound`. No changes needed in these — they construct from `LrcRecord`, which never produces `Unsupported`. Confirm the compiler is happy.

- [ ] **Step 7.4: Update `apply_outcome`'s match**

Find `apply_outcome` around line 357. It has a `match out.cached { ... }` that handles every variant. Add an arm for `Unsupported`:

```rust
        CachedLyrics::Unsupported => {
            s.status = Status::Unsupported;
            s.line_count = 0;
            s.plain = None;
            s.lines = vec![];
            s.translation = None;
            let _ = app.emit("lyrics-not-found", &*s);
        }
```

Place it right above the `CachedLyrics::NotFound` arm so it's read by humans as the "almost-NotFound" sibling.

- [ ] **Step 7.5: Verify `write_store` still skips `Unsupported`**

Find `write_store` (around line 1530-1560). It already skips `CachedLyrics::NotFound`. Modify the skip clause:

```rust
    // Don't persist NotFound or Unsupported — these are session-local
    // states that should re-evaluate on every replay. Caching them would
    // mask both Hum's own improvements (resolver tweaks between releases)
    // and improvements in the upstream source (Pandora finally adding
    // Media Session metadata).
    if matches!(cached, CachedLyrics::NotFound | CachedLyrics::Unsupported) {
        return;
    }
```

- [ ] **Step 7.6: Verify `read_store` discards `Unsupported`**

Find `read_store` (near `write_store`). It already has logic to discard `NotFound`. Extend it to discard `Unsupported` too:

```rust
    if matches!(cached, CachedLyrics::NotFound | CachedLyrics::Unsupported) {
        // Don't return stale Unsupported either — see write_store comment.
        let _ = store.delete(key); // best-effort cleanup of any pre-v0.10.22 disk entries
        return None;
    }
```

(If `read_store`'s existing branch doesn't `delete`, just match the existing pattern — discarding on read is enough; the cleanup is optional.)

- [ ] **Step 7.7: Verify compile**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`. If a `match` somewhere in the codebase is non-exhaustive after adding the variant, the compiler will name it — add the new arm with the same shape as the `NotFound` arm.

- [ ] **Step 7.8: Run tests**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all 13 tests pass (8 existing + 5 web_bridge).

- [ ] **Step 7.9: Commit**

```bash
git add src-tauri/src/lyrics.rs
git commit -m "lyrics: add Unsupported variant for sources we can't decode"
```

---

## Task 8: Resolver bridge consultation + Unsupported emission

**Files:**
- Modify: `src-tauri/src/lyrics.rs` — `start()` main loop body.

- [ ] **Step 8.1: Update `lyrics::start()` signature to accept the shared bridge**

Open `src-tauri/src/lyrics.rs`. Find the existing `pub fn start(app: AppHandle, shared: SharedLyrics, snapshot: SharedSnapshot)`. Add a `web_bridge: web_bridge::SharedWebBridge` parameter:

```rust
pub fn start(
    app: AppHandle,
    shared: SharedLyrics,
    snapshot: SharedSnapshot,
    web_bridge: crate::web_bridge::SharedWebBridge,
) {
```

- [ ] **Step 8.2: Add a `use` for the bridge module**

Near the top of `lyrics.rs`, add:

```rust
use crate::web_bridge;
```

(If `crate::web_bridge::SharedWebBridge` is already referenced inline, no separate import needed.)

- [ ] **Step 8.3: Subscribe `lyrics::start` to the new `web-bridge-updated` event**

In `lyrics::start`, right next to the existing `app.listen_any("track-changed", ...)` registration, add a second listener that wakes the same channel:

```rust
    let tx_track = tx.clone();
    app.listen_any("track-changed", move |_event| {
        let _ = tx_track.send(());
    });

    // Bridge probes (web_bridge.rs) emit web-bridge-updated when they
    // read a new track from Chrome's UIA tree. Wake the resolver loop
    // through the same channel — the bridge-cache consultation below
    // picks up the fresh values.
    let tx_bridge = tx.clone();
    app.listen_any("web-bridge-updated", move |_event| {
        let _ = tx_bridge.send(());
    });
```

- [ ] **Step 8.4: Replace the main loop body with the bridge-aware version**

The current main loop body (lines 137-183 of the existing file) is:

```rust
while rx.recv().await.is_some() {
    let snap = { snapshot.read().await.clone() };
    if snap.title.trim().is_empty() {
        continue;
    }
    // (long comment about empty-artist handling)
    let key = cache_key(&snap.artist, &snap.title, snap.duration_ms);
    if key == last_key {
        continue;
    }
    last_key = key.clone();

    let track = TrackEcho {
        title: snap.title.clone(),
        artist: snap.artist.clone(),
        album: snap.album.clone(),
        duration_ms: snap.duration_ms,
    };

    // Mark fetching. (long comment about errors reset)
    {
        let mut s = shared.write().await;
        *s = CurrentLyrics {
            track_key: key.clone(),
            status: Status::Fetching,
            source: None,
            line_count: 0,
            lines: vec![],
            plain: None,
            translation: None,
            errors: vec![],
            track: track.clone(),
        };
        emit_state(&app, &s);
    }

    let outcome = resolve_lyrics(&app, &client, &mem, &track, &key).await;
    apply_outcome(&app, &shared, &key, &track, outcome).await;
}
```

Replace the entire loop body with this:

```rust
while rx.recv().await.is_some() {
    let snap = { snapshot.read().await.clone() };

    // Consult the web-player bridge. If a probe wrote real track info
    // within the staleness window (5s), use that. Otherwise fall back
    // to SMTC's snapshot. Pandora.com is the motivating case — SMTC
    // sees only the browser tab title; the bridge fills in the real
    // song via UIA.
    let bridge_track = {
        let b = web_bridge.read().await;
        b.clone()
    };
    let now_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let bridge_fresh = bridge_track
        .as_ref()
        .is_some_and(|t| now_unix_ms - t.last_seen_unix_ms < 5_000 && !t.title.trim().is_empty());

    let (effective_title, effective_artist, effective_album) = if bridge_fresh {
        let t = bridge_track.as_ref().unwrap();
        (t.title.clone(), t.artist.clone(), t.album.clone())
    } else {
        (snap.title.clone(), snap.artist.clone(), snap.album.clone())
    };

    if effective_title.trim().is_empty() {
        continue;
    }

    // If SMTC's title matches a known-unreliable-source probe AND we
    // don't have fresh bridge data, surface Unsupported instead of
    // running the resolver against the garbage SMTC title. Pandora
    // web with the UIA probe broken / not-yet-read is the motivating
    // case — we'd otherwise look up the browser tab title as if it
    // were a song and get noise back.
    let unreliable_no_bridge = !bridge_fresh
        && web_bridge::any_probe_detects(
            &snap.title,
            snap.source_app_id.as_deref().unwrap_or(""),
        );

    let key = cache_key(&effective_artist, &effective_title, snap.duration_ms);
    if key == last_key {
        continue;
    }
    last_key = key.clone();

    let track = TrackEcho {
        title: effective_title.clone(),
        artist: effective_artist.clone(),
        album: effective_album.clone(),
        duration_ms: snap.duration_ms,
    };

    if unreliable_no_bridge {
        // Short-circuit: emit Unsupported, do NOT hit any network source.
        // The resolver's normal LRCLib / SimpMusic / NetEase chain would
        // burn an HTTP round trip on a non-song query and return NotFound
        // anyway. Skipping it saves the round trip and renders the
        // honest "Pandora web — track info unavailable" message.
        apply_outcome(
            &app,
            &shared,
            &key,
            &track,
            Outcome {
                cached: CachedLyrics::Unsupported,
                source: "unsupported-source".into(),
                persist: false,
                errors: Vec::new(),
            },
        )
        .await;
        continue;
    }

    // Mark fetching. The `errors: vec![]` reset prevents stale errors
    // from a previous track's resolution from leaking into the dev
    // console while this one is still in flight.
    {
        let mut s = shared.write().await;
        *s = CurrentLyrics {
            track_key: key.clone(),
            status: Status::Fetching,
            source: None,
            line_count: 0,
            lines: vec![],
            plain: None,
            translation: None,
            errors: vec![],
            track: track.clone(),
        };
        emit_state(&app, &s);
    }

    let outcome = resolve_lyrics(&app, &client, &mem, &track, &key).await;
    apply_outcome(&app, &shared, &key, &track, outcome).await;
}
```

The end of the loop (mark-fetching + resolve_lyrics + apply_outcome) is preserved verbatim — only the bridge-consultation and Unsupported-short-circuit are new, wedged between the snapshot read and the mark-fetching block.

- [ ] **Step 8.5: Update the `start()` callsite in `lib.rs`**

Open `src-tauri/src/lib.rs`. Find the existing `lyrics::start(app.handle().clone(), shared_lyrics.clone(), snapshot.clone());` call. Update to pass the shared bridge:

```rust
lyrics::start(
    app.handle().clone(),
    shared_lyrics.clone(),
    snapshot.clone(),
    shared_bridge.clone(),
);
```

- [ ] **Step 8.6: Verify compile**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished`.

- [ ] **Step 8.7: Verify clippy clean**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: `Finished` no warnings.

- [ ] **Step 8.8: Run tests**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: all 13 tests still pass.

- [ ] **Step 8.9: Commit**

```bash
git add src-tauri/src/lyrics.rs src-tauri/src/lib.rs
git commit -m "lyrics: consult web_bridge cache before SMTC; emit Unsupported for unreliable sources without bridge data"
```

---

## Task 9: Frontend rendering for `unsupported`

**Files:**
- Modify: `src/Overlay.tsx` — `statusLine` function

- [ ] **Step 9.1: Add the `unsupported` branch to `statusLine`**

Open `src/Overlay.tsx`. Find the `statusLine` function (around line 1250). Add a new case for `"unsupported"`:

```typescript
function statusLine(l: CurrentLyrics, t: CurrentTrack | null): string {
  switch (l.status) {
    case "fetching":
      return t?.title ? `♪ fetching — ${t.title}` : "♪ fetching…";
    case "not_found":
      return t?.title
        ? `♪ no lyrics for ${t.title}`
        : "♪ no lyrics on LRCLib";
    case "unsupported":
      // Source publishes audio but no metadata Hum can decode — Pandora
      // web is the motivating case. Show a clear reason rather than
      // pretending we just couldn't find lyrics for "Now Playing on
      // Pandora" (which is the browser tab title, not a song).
      if (l.source === "unsupported-source" && t?.title?.endsWith("Now Playing on Pandora")) {
        return "♪ Pandora web — track info unavailable";
      }
      return "♪ track info unavailable for this source";
    case "instrumental":
      return "♪ instrumental";
    case "plain":
      return "♪ unsynced lyrics (no per-line timing)";
    case "error":
      return "♪ error fetching lyrics";
    case "idle":
      return t?.title ? `♪ ${t.title}` : "♪";
    default:
      return "♪";
  }
}
```

- [ ] **Step 9.2: Check the dev console rendering too**

Open `src/DevConsole.tsx`. Find the existing `lyrics.status === "not_found"` branch (around line 195). Add a parallel `unsupported` branch in the same conditional chain — render it the same way as `not_found` for the dev console (a status row showing "Unsupported (Pandora web)" or similar). If the dev console only uses `not_found` for a generic "no lyrics" panel, copy that panel with adjusted text.

The exact edit depends on the dev console structure — read the surrounding 30 lines and add an `unsupported` branch alongside the `not_found` branch, with display text `"Unsupported — track info unavailable from this source"`.

- [ ] **Step 9.3: Update the TypeScript Status type**

Search for the TS Status type definition. It's likely in Overlay.tsx or a types file:

Run: `grep -rn "type Status\|status: \"" src/`

Find the union type, e.g.:

```typescript
type Status =
  | "idle"
  | "fetching"
  | "synced"
  | "plain"
  | "instrumental"
  | "not_found"
  | "error";
```

Add `"unsupported"`:

```typescript
type Status =
  | "idle"
  | "fetching"
  | "synced"
  | "plain"
  | "instrumental"
  | "not_found"
  | "unsupported"
  | "error";
```

- [ ] **Step 9.4: Typecheck**

Run: `cd D:/Work/App_Projects/All_Projects/lyric-overlay && pnpm typecheck 2>&1 | tail -5`
Expected: no errors.

- [ ] **Step 9.5: Commit**

```bash
git add src/Overlay.tsx src/DevConsole.tsx
git commit -m "overlay: render 'unsupported' status with source-specific message"
```

---

## Task 10: Version bump + changelog + final integration test

**Files:**
- Modify: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` — version `0.10.21` → `0.10.22`.
- Modify: `docs/CHANGELOG.md` — new entry above `[0.10.21]`.

- [ ] **Step 10.1: Bump versions**

Edit each of these to change `0.10.21` to `0.10.22`:
- `package.json` line 3: `"version": "0.10.22",`
- `src-tauri/Cargo.toml` line 3: `version = "0.10.22"`
- `src-tauri/tauri.conf.json` line 4: `"version": "0.10.22",`

- [ ] **Step 10.2: Add changelog entry**

Open `docs/CHANGELOG.md`. Add this block immediately above the `## [0.10.21] - 2026-05-21` heading:

```markdown
## [0.10.22] - 2026-05-21

### Added
- **Pandora.com web player now works.** Songs playing on pandora.com in Chrome surface real lyrics, the same as Spotify Web / YouTube / iTunes already did. Previously the overlay showed `♪ no lyrics for Today's Hits Radio - Now Playing on Pandora` because Pandora's website doesn't call `navigator.mediaSession.metadata` — SMTC fell back to the browser tab title (a station name, not a song) and the resolver had nothing real to look up. Hum now reads Pandora's now-playing widget directly from Chrome's accessibility tree via Windows UI Automation (the same API screen readers use); Chromium enables its UIA tree on demand with no user prompt, no extension, no flag. The real track title / artist / album feed into the standard cleaner + LRCLib / SimpMusic / NetEase resolver path, so any song Pandora plays gets the same lyric coverage as a song from any other source. Polls every 2 seconds while a Pandora tab is the active SMTC source; idle (zero CPU, zero UIA calls) when the user is on YouTube / Spotify / iTunes / anything else. Trait-based extension point under the hood means future no-Media-Session web players (SoundCloud, Bandcamp, etc.) land as one-file additions.
- **New "Unsupported" overlay status for sources Hum can't decode.** When Hum sees SMTC reporting an unreliable source (Pandora web with the UIA probe unavailable, or any future case where a known-broken source publishes audio without track metadata) and has no fresh bridge data, the overlay now shows `♪ Pandora web — track info unavailable` instead of the misleading `♪ no lyrics for [station name]`. The honest message replaces the "lookup failure" framing — users know it's a source-side limitation, not a missing-song problem. Like NotFound (since v0.10.15), Unsupported is never persisted to disk and never cached in memory, so a Hum upgrade or a Pandora-side fix immediately propagates without stale verdicts.

### Architecture / files
- **New `src-tauri/src/web_bridge.rs`** — `WebPlayerProbe` trait + `PandoraProbe` impl + polling loop. `PandoraProbe::detects` is a pure string match (Chromium AUMID + Pandora `<title>` suffix); `PandoraProbe::read` walks the UIA tree of the matching Chrome window via the `uiautomation` crate, extracting the track / artist / album text from Pandora's now-playing widget. The loop spawns at startup; idles at 5s ticks with zero UIA calls when no probe matches, polls every 2s when a probe is active.
- **`src-tauri/src/lyrics.rs`** — new `CachedLyrics::Unsupported` and `Status::Unsupported` enum variants. `lyrics::start`'s main loop consults the shared web-bridge cache before falling back to the SMTC snapshot; when the bridge is fresh, its title / artist / album override SMTC for the duration of that resolution. When SMTC's title matches a known-unreliable source AND no fresh bridge data exists, the loop short-circuits to Unsupported without any network calls. `write_store` and `read_store` skip Unsupported the same way they skip NotFound — never persisted, never memory-cached.
- **`src-tauri/src/lib.rs`** — new `SharedWebBridge` managed state. `web_bridge::start` spawns alongside `smtc::start` at boot. `lyrics::start` receives the shared bridge as a new parameter.
- **`src/Overlay.tsx`** — new `unsupported` branch in `statusLine`. Renders `♪ Pandora web — track info unavailable` for Pandora-specific source matches and a generic `♪ track info unavailable for this source` for future unsupported sources.
- **`Cargo.toml`** — new `uiautomation` crate dependency. Wraps the Win32 `IUIAutomation` COM interface into a safe Rust API; saves ~500 lines of unsafe COM glue.

### Diagnostic notes
- Pandora UIA selectors (track title / artist / album element ClassName + AutomationId) are hard-coded in `web_bridge.rs` based on the Pandora DOM as of 2026-05-21. If Pandora ships a redesign that breaks the selectors, the probe returns `None` and the resolver falls through to Unsupported — overlay shows the honest status rather than wrong lyrics. Updating the selectors after a Pandora redesign is a single-file edit; the discovery procedure (run `inspect.exe`, hover the elements, copy their stable attributes) is documented at the top of the `Pandora UIA selector reference` comment block in `web_bridge.rs`.
- Performance: a Pandora UIA read takes 50-300ms on a typical Chrome session. With 2s polling cadence, that's well under 1% CPU averaged over a minute. Idle state (no probe active) uses essentially zero CPU — the loop is a 5s sleep that checks the SMTC snapshot pointer and goes back to sleep.
- YouTube / Spotify Web / iTunes desktop / Apple Music desktop / SoundCloud (today) / Bandcamp (today) all expose Media Session metadata correctly and never enter the probe path. SMTC title-pattern matching is precise (`ends_with("Now Playing on Pandora")` not `contains("pandora")`) to avoid false-positives on song titles that mention Pandora.
```

- [ ] **Step 10.3: Compile + test + lint all together**

```bash
cd D:/Work/App_Projects/All_Projects/lyric-overlay
pnpm typecheck && cd src-tauri && cargo check && cargo test --lib && cargo clippy --all-targets -- -D warnings
```

Expected: typecheck clean, cargo check finished, 13 tests pass, clippy clean.

- [ ] **Step 10.4: Full integration verify against all four primary sources**

Start the app:

```bash
pnpm tauri dev
```

Test each source — confirm Hum shows real lyrics (or appropriate status) for each:

1. **Pandora.com in Chrome** — start a station. Expect: real lyrics appear within 2-4 seconds of each track change. Console logs `[web_bridge] probe=pandora-web read title=...`.
2. **YouTube in same Chrome window (new tab)** — play any music video. Expect: real lyrics. No `[web_bridge]` activity for this tab (probe doesn't match YouTube's title).
3. **Spotify Web in Chrome** — play a song. Expect: real lyrics, no `[web_bridge]` involvement.
4. **iTunes desktop** (if installed) — play a song. Expect: real lyrics, no `[web_bridge]` involvement (non-Chrome AUMID).
5. **Test the Unsupported path:** in `web_bridge.rs`, temporarily change `PandoraProbe::read` to return `Ok(None)`. Restart Hum. With Pandora playing, expect overlay shows `♪ Pandora web — track info unavailable`, NOT `♪ no lyrics for ...`. Revert the temporary change before committing.
6. **Tab-switch test:** With Pandora playing in one tab, switch to another tab playing YouTube. Overlay should switch to YouTube's song lyrics within one SMTC `MediaChanged` event (typically 1-2 seconds). Console logs `[web_bridge] no probe matches, cache cleared`.

If any of 1-6 fails, do NOT commit Task 10 yet — diagnose and fix the root cause. Possible failure modes:

- 1 fails (Pandora doesn't get lyrics) → `inspect.exe` against the live Pandora page, re-verify the selectors used in `PandoraProbe::read`.
- 2-4 fail (regression on existing sources) → check that `PandoraProbe::detects` correctly returns `false` for them (`cargo test --lib web_bridge` should catch this).
- 5 fails (Unsupported doesn't render) → check the resolver short-circuit in `lyrics.rs` and the frontend `statusLine` case.
- 6 fails (stale Pandora lyrics) → check the cache-clear-on-probe-deactivation logic in `web_bridge::start`.

- [ ] **Step 10.5: Stop the dev server**

- [ ] **Step 10.6: Commit the version + changelog**

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "$(cat <<'EOF'
v0.10.22: Pandora web bridge via Windows UI Automation

Pandora.com in Chrome now surfaces real lyrics. Pandora's web app doesn't
call navigator.mediaSession.metadata, so SMTC falls back to the browser
tab title — useless for lyric lookup. v0.10.22 reads Pandora's now-playing
widget directly from Chrome's accessibility tree via the same Windows UIA
API screen readers use. Chromium enables its accessibility tree on demand
when an external process queries; no user prompt, no extension, no flag.

Architecture is trait-based so future no-Media-Session web players
(SoundCloud, Bandcamp, NPR, etc.) land as one-file probe additions.

Ships with graceful failure: a new CachedLyrics::Unsupported variant
surfaces "Pandora web — track info unavailable" when UIA reads fail
(Pandora redesign, Chrome closed, etc.), rather than the misleading
"no lyrics for [station name]" output.

YouTube / Spotify Web / iTunes / any other source is unaffected —
their SMTC paths don't match any probe and never enter the bridge code.

Full walkthrough: docs/CHANGELOG.md.
EOF
)"
```

- [ ] **Step 10.7: Final state check**

Run: `git log --oneline -5 && cargo test --lib 2>&1 | tail -3 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected:
- 5 most recent commits visible, the top one being the v0.10.22 commit.
- `13 passed; 0 failed`.
- Clippy `Finished` no warnings.

Do NOT push — per Hum's policy in `CLAUDE.md`, Tauri desktop apps only push when Wes explicitly asks.

---

## Done. Out of scope for this plan (queued elsewhere):

- Mica/acrylic backdrop (`docs/superpowers/specs/2026-05-21-hum-mica-acrylic-backdrop-design.md` — spec ready, next slice after v0.10.22).
- Album-art UIA scrape (replace Chrome favicon with real Pandora art).
- Hover-expand panel with artist bio + tour dates + ticket affiliate links (queued as session task #8).
- SoundCloud / Bandcamp / other probes — one-file additions to `web_bridge.rs` when Wes hits them.
