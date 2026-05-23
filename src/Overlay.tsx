import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  CurrentLyrics,
  CurrentTrack,
  LayoutMode,
  LyricLine,
  OverlayMode,
  Promo,
  Settings,
  TextAlign,
  WordSpan,
} from "./types";
import { fmtMs } from "./types";

const DEFAULT_SETTINGS: Settings = {
  last_mode: "edit",
  anticipate_ms: 0,
  jitter_tolerance_ms: 2000,
  font_family: "Inter",
  font_size_px: 26,
  font_weight: 600,
  text_color: "#ffffff",
  text_color_dim: "rgba(255,255,255,0.45)",
  bg_color: "#000000",
  bg_opacity: 0,
  text_align: "left",
  line_padding_px: 6,
  layout_mode: "three_line",
  show_album_art: true,
  show_translation: false,
  tint_bg_from_album_art: false,
  blur_album_art_background: true,
  window_backdrop: "acrylic",
  auto_contrast: true,
  streamer_enabled: false,
  streamer_port: 38247,
  show_artist_info_panel: true,
  ad_break_promos_enabled: true,
  launch_on_startup: false,
};

export default function Overlay() {
  const [track, setTrack] = useState<CurrentTrack | null>(null);
  const [lyrics, setLyrics] = useState<CurrentLyrics | null>(null);
  // displayIdx is what the DOM renders. It changes only when the active line
  // changes (~once per LRC entry), NOT on every rAF tick.
  const [displayIdx, setDisplayIdx] = useState<number>(-1);
  const [mode, setMode] = useState<OverlayMode>("edit");
  const [hovered, setHovered] = useState(false);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [albumArt, setAlbumArt] = useState<{ title: string; artist: string; data_url: string } | null>(null);
  // Per-word karaoke cursor — only relevant when the active line has
  // .words populated (SimpMusic-sourced tracks). -1 = before-first.
  const [currentWordIdx, setCurrentWordIdx] = useState<number>(-1);
  // Dominant color extracted from the current album art's data URL — fed
  // into the overlay background when tint_bg_from_album_art is on.
  const [tintColor, setTintColor] = useState<{ r: number; g: number; b: number } | null>(null);
  // Measured pixel height of the lyrics column. Drives the side-by-side
  // album art's size so it's exactly as tall as the lyrics next to it
  // (CSS `align-self: stretch + aspect-ratio: 1` was off by a few px in
  // practice — the row height was driven by the image's intrinsic size).
  const [lyricsColEl, setLyricsColEl] = useState<HTMLDivElement | null>(null);
  const [artSize, setArtSize] = useState<number>(80);
  // Inner row element ref — drives the auto-resize of the window's
  // height to match content. No empty space inside the window.
  const [innerRowEl, setInnerRowEl] = useState<HTMLDivElement | null>(null);
  // Raw screen-behind-the-window color sample from contrast.rs's screen-
  // capture worker. Null until the first sample arrives. Carries RGB so
  // we can compute both luminance AND saturation downstream — a tan/gold
  // surface is luminance-light but saturation-tinted, and we want white
  // text there, not the dark text a pure-luminance threshold would pick.
  const [screenColor, setScreenColor] = useState<
    { r: number; g: number; b: number } | null
  >(null);
  // Final boolean: is the surface UNDER the lyric text visually light?
  // Drives the auto-contrast text-color flip. Computed each render from
  // (screenColor, blur-bg, tint color, user bg color, bg opacity),
  // then debounced through hysteresis to avoid flickering when the
  // surface luminance hovers near the 0.5 threshold (dynamic videos
  // behind a transparent overlay are the classic culprit).
  const [surfaceIsLight, setSurfaceIsLight] = useState<boolean | null>(null);
  // Window inner dimensions — drives the scale factor for text + gap +
  // album art sizing. Updates on resize via the listener below.
  const [winSize, setWinSize] = useState<{ w: number; h: number }>({
    w: typeof window !== "undefined" ? window.innerWidth : BASELINE_WINDOW_W_PX,
    h: typeof window !== "undefined" ? window.innerHeight : BASELINE_WINDOW_H_PX,
  });
  // Live lyric-offset nudge (Ctrl+Alt+[ / Ctrl+Alt+]). Session-only;
  // resets on track change so a nudge for one bad LRC doesn't bleed
  // into the next song. Stored in a ref because the rAF closure reads
  // it; the React state is the brief on-screen banner only.
  const nudgeMsRef = useRef<number>(0);
  const [nudgeBanner, setNudgeBanner] = useState<{ value: number; until: number } | null>(null);
  // Auto-update banner state. `available` shows the gold "v X.Y.Z" pill;
  // `installing` shows the spinner state; `ready` prompts to restart.
  const [updateState, setUpdateState] = useState<
    | { phase: "idle" }
    | { phase: "available"; version: string; update: Update }
    | { phase: "downloading"; version: string }
    | { phase: "ready"; version: string }
    | { phase: "error"; message: string }
  >({ phase: "idle" });
  // Coarse re-render trigger for the progress bar / time readout.
  // Position interpolation is computed in render from
  // `track.position_ms + (Date.now() - track.last_update_unix_ms)` while
  // playing; this counter just causes a re-render twice a second so the
  // bar visibly advances between server-pushed `timeline-changed` ticks
  // (which arrive every 2 s and would otherwise leave the bar frozen).
  // The value itself is never read — only the state change triggers
  // re-render — so we don't destructure it.
  const [, setProgressTick] = useState<number>(0);

  // Refs hold the hot-loop data so the rAF closure stays stable across
  // re-renders. Events update these AND the React state.
  const trackRef = useRef<CurrentTrack | null>(null);
  const lyricsRef = useRef<CurrentLyrics | null>(null);
  const indexRef = useRef<number>(-1);
  const wordIdxRef = useRef<number>(-1);
  const settingsRef = useRef<Settings>(DEFAULT_SETTINGS);

  useEffect(() => {
    function interpolatedPositionMs(): number {
      const t = trackRef.current;
      if (!t) return 0;
      if (t.state !== "playing") return t.position_ms;
      const wallElapsed = Date.now() - t.last_update_unix_ms;
      return t.position_ms + Math.max(0, wallElapsed);
    }

    function lookupPositionMs(): number {
      // anticipate_ms = global setting; nudgeMs = per-track live nudge via
      // Ctrl+Alt+[ / Ctrl+Alt+]. POSITIVE nudge means lyrics show LATER,
      // so we SUBTRACT from the lookup position (look further back).
      return interpolatedPositionMs() + settingsRef.current.anticipate_ms - nudgeMsRef.current;
    }

    function snapCursorToCurrentPosition(lines: LyricLine[]): number {
      if (lines.length === 0) return -1;
      const pos = lookupPositionMs();
      let lo = 0;
      let hi = lines.length;
      let found = -1;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (lines[mid].time_ms <= pos) {
          found = mid;
          lo = mid + 1;
        } else {
          hi = mid;
        }
      }
      return found;
    }

    let rafId = 0;
    function tick() {
      const l = lyricsRef.current;
      if (l && l.status === "synced" && l.lines.length > 0) {
        const pos = lookupPositionMs();
        const lines = l.lines;
        let idx = indexRef.current;
        // Advance forward (the usual case during normal playback)
        while (idx + 1 < lines.length && lines[idx + 1].time_ms <= pos) idx++;
        // Rewind backward (user seeked / new track loaded)
        while (idx >= 0 && lines[idx].time_ms > pos) idx--;
        if (idx !== indexRef.current) {
          indexRef.current = idx;
          wordIdxRef.current = -1;
          setDisplayIdx(idx);
          setCurrentWordIdx(-1);
        }

        // Per-word cursor inside the current line. Only when SimpMusic gave
        // us word-level timing; otherwise per-line highlight is the whole story.
        if (idx >= 0) {
          const words = lines[idx].words;
          if (words && words.length > 0) {
            let wIdx = wordIdxRef.current;
            // Forward
            while (wIdx + 1 < words.length && words[wIdx + 1].time_ms <= pos) wIdx++;
            // Backward (seek inside the line)
            while (wIdx >= 0 && words[wIdx].time_ms > pos) wIdx--;
            if (wIdx !== wordIdxRef.current) {
              wordIdxRef.current = wIdx;
              setCurrentWordIdx(wIdx);
            }
          }
        }
      }
      rafId = requestAnimationFrame(tick);
    }

    function applyTrack(
      t: CurrentTrack,
      kind: "track" | "timeline" | "state",
    ) {
      const prev = trackRef.current;
      let next = t;

      // Monotonic clamp: if this is a timeline tick during stable same-track
      // playback and the reported position is slightly BEHIND where our
      // interpolation already is (presumed source-counter staleness, not a
      // real seek), keep advancing from the old anchor. Real seeks crossing
      // the user-tunable jitter tolerance in either direction pass through.
      const jitter = settingsRef.current.jitter_tolerance_ms;
      if (
        prev &&
        kind === "timeline" &&
        prev.title === t.title &&
        prev.artist === t.artist &&
        prev.state === "playing" &&
        t.state === "playing"
      ) {
        const expected =
          prev.position_ms +
          (t.last_update_unix_ms - prev.last_update_unix_ms);
        const drift = expected - t.position_ms;
        if (drift > 0 && drift < jitter) {
          next = { ...t, position_ms: expected };
        }
      }

      trackRef.current = next;
      setTrack(next);

      if (!prev || prev.title !== t.title || prev.artist !== t.artist) {
        indexRef.current = -1;
        setDisplayIdx(-1);
        // Reset the live nudge so a fix for one bad-LRC track doesn't bleed
        // into the next song.
        nudgeMsRef.current = 0;
        setNudgeBanner(null);
      }
    }

    function applyLyrics(l: CurrentLyrics) {
      lyricsRef.current = l;
      setLyrics(l);
      wordIdxRef.current = -1;
      setCurrentWordIdx(-1);
      if (l.status === "synced" && l.lines.length > 0) {
        const idx = snapCursorToCurrentPosition(l.lines);
        indexRef.current = idx;
        setDisplayIdx(idx);
      } else {
        indexRef.current = -1;
        setDisplayIdx(-1);
      }
    }

    function applySettings(s: Settings) {
      settingsRef.current = s;
      setSettings(s);
    }

    const unlisteners: Array<Promise<() => void>> = [
      listen<CurrentTrack>("track-changed", (e) => applyTrack(e.payload, "track")),
      listen<CurrentTrack>("timeline-changed", (e) => applyTrack(e.payload, "timeline")),
      listen<CurrentTrack>("playback-state-changed", (e) => applyTrack(e.payload, "state")),
      listen<CurrentLyrics>("lyrics-state", (e) => applyLyrics(e.payload)),
      listen<CurrentLyrics>("lyrics-loaded", (e) => applyLyrics(e.payload)),
      listen<CurrentLyrics>("lyrics-not-found", (e) => applyLyrics(e.payload)),
      listen<OverlayMode>("mode-changed", (e) => setMode(e.payload)),
      listen<Settings>("settings-changed", (e) => applySettings(e.payload)),
      listen<{ title: string; artist: string; data_url: string }>(
        "album-art-loaded",
        (e) => {
          setAlbumArt(e.payload);
          // Extract the dominant color asynchronously; tint render reads
          // it from state on the next paint. Failure → null = no tint
          // applied (background stays as user-configured).
          extractDominantColor(e.payload.data_url).then(setTintColor);
        },
      ),
      listen<number>("lyric-offset-nudge", (e) => {
        const delta = e.payload;
        const next = nudgeMsRef.current + delta;
        nudgeMsRef.current = next;
        setNudgeBanner({ value: next, until: Date.now() + 1500 });
      }),
      listen<{ luminance: number; r: number; g: number; b: number }>(
        "bg-luminance",
        (e) => {
          // Stash the raw sample. Compositing with blur/tint/user-bg and
          // the hysteresis pass run downstream in a useEffect so the
          // final decision reflects what the user actually sees on the
          // overlay's own surface, not just the screen behind.
          setScreenColor({ r: e.payload.r, g: e.payload.g, b: e.payload.b });
        },
      ),
    ];

    invoke<CurrentTrack>("get_current_track")
      .then((t) => applyTrack(t, "track"))
      .catch(() => {});
    invoke<CurrentLyrics>("get_current_lyrics")
      .then(applyLyrics)
      .catch(() => {});
    // Pull whatever album art the backend has already fetched. Closes the
    // startup race where the backend's emit_full → spawn_art_fetch chain
    // fires `album-art-loaded` BEFORE the listen() above has finished
    // subscribing (Tauri events are fire-and-forget; no replay for late
    // subscribers). Without this, a fresh app launch with music already
    // playing shows lyrics but no artwork until the user switches tracks.
    invoke<{ title: string; artist: string; data_url: string } | null>(
      "get_current_album_art",
    )
      .then((art) => {
        if (art) {
          setAlbumArt(art);
          extractDominantColor(art.data_url).then(setTintColor);
        }
      })
      .catch(() => {});
    invoke<OverlayMode>("get_overlay_mode").then(setMode).catch(() => {});
    invoke<Settings>("get_settings").then(applySettings).catch(() => {});

    rafId = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(rafId);
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  // Track the overlay window's inner dimensions for the scale-with-window
  // text feature. Throttled implicitly by the browser's resize coalescing.
  useEffect(() => {
    const handler = () => setWinSize({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", handler);
    return () => window.removeEventListener("resize", handler);
  }, []);

  // 500ms progress-bar repaint. Cheap because the bar reads
  // `track.position_ms + Date.now()-last_update` inline at render and only
  // the metadata column subtree re-renders meaningfully. Stops the tick
  // when nothing's playing so a paused/closed app doesn't cost a wake
  // every half-second indefinitely.
  useEffect(() => {
    if (!track || track.state !== "playing") return;
    const id = window.setInterval(() => {
      setProgressTick((n) => (n + 1) | 0);
    }, 500);
    return () => window.clearInterval(id);
  }, [track?.state, track?.last_update_unix_ms]);

  // updateStateRef so the tray-event listener (single closure created
  // on mount) can read the latest state without re-subscribing every
  // time the state changes.
  const updateStateRef = useRef(updateState);
  useEffect(() => {
    updateStateRef.current = updateState;
  }, [updateState]);

  // Auto-update check. Runs once on overlay mount + whenever the user
  // picks "Check for updates" / "Install update vX" from the tray menu.
  // Same tray event covers both: if we already have an Update object,
  // install it; otherwise run a fresh check.
  useEffect(() => {
    const runCheck = async () => {
      try {
        const update = await checkForUpdate();
        if (update?.available) {
          setUpdateState({ phase: "available", version: update.version, update });
          invoke("set_update_indicator", { pendingVersion: update.version }).catch(() => {});
        }
      } catch {
        // Silent — don't surface "no endpoint reachable" noise to users.
      }
    };
    runCheck();
    const un = listen("updater-check-requested", () => {
      if (updateStateRef.current.phase === "available") {
        // Trigger install by calling the same handler the banner uses.
        // We re-derive from state so we don't need to pass it in.
        installUpdateInternal();
      } else {
        runCheck();
      }
    });
    return () => {
      un.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Tell the Rust ghost-mode cursor-poll worker whether the banner is
  // currently visible so it can poke a clickable hole in the
  // click-through region when needed.
  useEffect(() => {
    const visible = updateState.phase !== "idle";
    invoke("set_update_banner_visible", { visible }).catch(() => {});
  }, [updateState.phase]);

  async function installUpdateInternal() {
    const cur = updateStateRef.current;
    if (cur.phase !== "available") return;
    const { version, update } = cur;
    if (!update) {
      // Demo path — no real Update object. Simulate the lifecycle so
      // Wes can see the visual. Remove the inner branch when ready.
      setUpdateState({ phase: "downloading", version });
      window.setTimeout(() => {
        setUpdateState({ phase: "ready", version });
        invoke("set_update_indicator", { pendingVersion: null }).catch(() => {});
      }, 1500);
      return;
    }
    setUpdateState({ phase: "downloading", version });
    try {
      await update.downloadAndInstall();
      setUpdateState({ phase: "ready", version });
      invoke("set_update_indicator", { pendingVersion: null }).catch(() => {});
      window.setTimeout(() => {
        relaunch().catch(() => {});
      }, 800);
    } catch (e) {
      setUpdateState({ phase: "error", message: String(e) });
    }
  }
  // Backwards-compat name for the banner's onInstall prop.
  const installUpdate = installUpdateInternal;

  // Sync the album art's size to the lyrics column's measured height.
  // useLayoutEffect for the initial measure (before browser paint, so no
  // flash). ResizeObserver for live updates (font-size slider, line wrap,
  // line-padding slider, layout-mode change).
  useLayoutEffect(() => {
    if (!lyricsColEl) return;
    const h = lyricsColEl.getBoundingClientRect().height;
    if (h > 0) setArtSize(Math.round(h));
  }, [lyricsColEl]);
  useEffect(() => {
    if (!lyricsColEl) return;
    const ro = new ResizeObserver((entries) => {
      const h = entries[0]?.contentRect.height;
      if (h && h > 0) setArtSize(Math.round(h));
    });
    ro.observe(lyricsColEl);
    return () => ro.disconnect();
  }, [lyricsColEl]);

  // Auto-resize the OS window's height to match the inner row's content
  // height + a small padding buffer. Result: no empty vertical space
  // inside the window. User can still drag the right edge to make it
  // wider (which scales text larger, which auto-grows the window
  // height to match). Dragging the bottom edge effectively snaps back
  // to content height on the next observe tick.
  useEffect(() => {
    if (!innerRowEl) return;
    let inFlight = false;
    const apply = async (rowH: number) => {
      if (inFlight) return;
      inFlight = true;
      try {
        const win = getCurrentWindow();
        const padding = 24; // top + bottom container padding (12 + 12)
        const targetH = Math.max(60, Math.round(rowH + padding));
        const cur = await win.outerSize();
        const sf = await win.scaleFactor();
        const curW = cur.width / sf;
        const curH = cur.height / sf;
        if (Math.abs(curH - targetH) > 2) {
          await win.setSize(new LogicalSize(curW, targetH));
        }
      } catch {
        // Window APIs not available — no-op (shouldn't happen in Tauri).
      } finally {
        inFlight = false;
      }
    };
    const ro = new ResizeObserver((entries) => {
      const h = entries[0]?.contentRect.height;
      if (h && h > 0) apply(h);
    });
    ro.observe(innerRowEl);
    // Initial measure for the very first render.
    apply(innerRowEl.getBoundingClientRect().height);
    return () => ro.disconnect();
  }, [innerRowEl]);

  let prev: LyricLine | undefined;
  let cur: LyricLine | undefined;
  let next: LyricLine | undefined;
  if (lyrics && lyrics.status === "synced" && lyrics.lines.length > 0) {
    if (displayIdx >= 0) {
      cur = lyrics.lines[displayIdx];
      prev = displayIdx > 0 ? lyrics.lines[displayIdx - 1] : undefined;
      next =
        displayIdx + 1 < lyrics.lines.length
          ? lyrics.lines[displayIdx + 1]
          : undefined;
    } else {
      // Song is still in the intro — no line has been reached yet. Surface
      // the upcoming first line in the `next` slot so the user can see
      // what's about to start instead of staring at a lonely `♪` and
      // wondering when lyrics begin. `cur` stays undefined so the status
      // line ("♪") renders in the big middle row; `prev` stays empty.
      next = lyrics.lines[0];
    }
  }

  const middleText =
    cur?.text || (lyrics ? statusLine(lyrics, track) : "♪");

  // Translation under the current line — only when the user opted in AND the
  // source actually returned one (currently only NetEase tlyric).
  const translationText: string | undefined =
    settings.show_translation &&
    lyrics?.translation &&
    displayIdx >= 0 &&
    displayIdx < lyrics.translation.length
      ? lyrics.translation[displayIdx]?.text
      : undefined;

  // Effective track for album-art matching. Web-bridge sources (Pandora)
  // overwrite the SMTC title with the bridge-resolved real title at the
  // lyrics-resolver layer, surfaced through `lyrics.track`. The SMTC
  // `track` state still carries the garbage tab title ("Today's Hits
  // Radio - Now Playing on Pandora") because that's what the OS gave us.
  // Matching album-art against `track` alone would never display art for
  // Pandora — we always prefer `lyrics.track` when its title is filled
  // in, falling back to `track` for sources that don't go through a
  // web-bridge override (YouTube / Spotify / iTunes / etc.).
  const artMatchTrack =
    lyrics?.track?.title?.trim()
      ? lyrics.track
      : track;
  // Active ad break? PromoCard is the visual focus — suppress the album-art
  // square AND the blurred-bg layer below so the overlay doesn't visually
  // anchor to whatever song was playing before the ad started. Without this,
  // the prior track's cover art lingers on the left and its dominant-color
  // tint persists in the background through the entire ad break, making it
  // look like "your song is still playing."
  const adActive = lyrics?.status === "ad";

  // Album art only shows for the *currently playing* track — past art lingers
  // in state until the next album-art-loaded event arrives. During an ad
  // break, hide it regardless (see `adActive` above).
  const showArt =
    !adActive &&
    settings.show_album_art &&
    !!albumArt &&
    !!artMatchTrack &&
    albumArt.title === artMatchTrack.title &&
    albumArt.artist === artMatchTrack.artist;

  // Edit mode: drag-region active, dashed border on hover, move cursor.
  // Locked: no drag, no border. Ghost: same; the window is also click-through
  // via set_ignore_cursor_events on the Rust side.
  const isEdit = mode === "edit";
  const dragProps = isEdit ? { "data-tauri-drag-region": true } : {};
  const borderColor = isEdit && hovered
    ? "rgba(212, 175, 55, 0.85)"
    : "transparent";

  const openArtistPanel = settings.show_artist_info_panel && mode !== "ghost"
    ? () => { invoke("open_artist_panel_cmd").catch(() => {}); }
    : undefined;

  // Close any open panel immediately when the user toggles the setting off.
  useEffect(() => {
    if (!settings.show_artist_info_panel) {
      invoke("close_artist_panel_cmd").catch(() => {});
    }
  }, [settings.show_artist_info_panel]);

  const layoutMode: LayoutMode = settings.layout_mode;
  // Auto-contrast override: when the toggle is on AND we have a luminance
  // read for the OVERLAY SURFACE (not the screen behind — see the
  // composited-luminance useEffect below), replace the user's text colors
  // with high-contrast values.
  const autoColorActive = settings.auto_contrast && surfaceIsLight !== null;
  const effectiveTextColor = autoColorActive
    ? (surfaceIsLight ? "#0a0a0a" : "#ffffff")
    : settings.text_color;
  // Solid grays for the autocolor branch — alpha-based dims wash out on
  // bright/colorful album-art backgrounds because the background bleeds
  // through and tints the dim text. Solid values render the same regardless
  // of what's behind.
  const effectiveTextColorDim = autoColorActive
    ? (surfaceIsLight ? "#5a5a5a" : "#c8c8c8")
    : settings.text_color_dim;
  // Scale factor: driven by WIDTH only since height auto-follows content
  // (see the innerRow ResizeObserver above). Drag the window wider →
  // text scales up → content gets taller → window height auto-grows.
  // Drag narrower → text scales down → content shrinks → window height
  // auto-shrinks. No empty vertical space ever.
  const scale = Math.max(0.4, winSize.w / BASELINE_WINDOW_W_PX);
  const baseSettings = autoColorActive
    ? { ...settings, text_color: effectiveTextColor, text_color_dim: effectiveTextColorDim }
    : settings;
  const settingsForRender: Settings = {
    ...baseSettings,
    font_size_px: baseSettings.font_size_px * scale,
    line_padding_px: Math.max(0, Math.round(baseSettings.line_padding_px * scale)),
  };
  // Drop shadow under the text — directional (light source from above)
  // rather than a symmetric halo. Two stacked offsets: a tight sharp one
  // for edge definition + a wider soft one for depth against busy
  // backgrounds. Color flips opposite the text (dark text gets a white
  // shadow on light surfaces, light text gets a black shadow elsewhere).
  const effectiveTextShadow =
    autoColorActive && surfaceIsLight
      ? "0 1px 2px rgba(255,255,255,0.9), 0 3px 10px rgba(255,255,255,0.55)"
      : "0 1px 2px rgba(0,0,0,0.9), 0 3px 10px rgba(0,0,0,0.55)";
  // When tint is on AND we have a color extracted from the current art, blend
  // the user's bg_color with the tint at 50/50 in RGB. Force a minimum 22%
  // opacity so the toggle is visibly doing something even when the user has
  // bg_opacity=0 (the default — fully transparent background). Otherwise the
  // toggle would be a no-op for most users.
  const tintActive =
    settings.tint_bg_from_album_art &&
    !!tintColor &&
    !!artMatchTrack &&
    !!albumArt &&
    albumArt.title === artMatchTrack.title &&
    albumArt.artist === artMatchTrack.artist;
  const effectiveBgColor = tintActive
    ? mixHexWithRgb(settings.bg_color, tintColor!, 0.5)
    : settings.bg_color;
  const effectiveOpacity =
    tintActive && settings.bg_opacity < 22 ? 22 : settings.bg_opacity;
  const bgRgba = colorWithOpacity(effectiveBgColor, effectiveOpacity);

  // Blurred album art behind the lyrics — Apple Music "Now Playing" style.
  // Renders only when there's art for the *current* track AND the user
  // hasn't turned the setting off. The user's bg_color paints on top so
  // the opacity slider still works as a darkening tint.
  const showBlurBg =
    !adActive &&
    settings.blur_album_art_background &&
    !!albumArt &&
    !!artMatchTrack &&
    albumArt.title === artMatchTrack.title &&
    albumArt.artist === artMatchTrack.artist;
  // When the blur is active we drop the container's own bg paint and
  // render bgRgba as a layered absolute div above the blur instead.
  // Otherwise it'd cover the blur entirely (bg paints under flex
  // content but ABOVE absolute children with z-auto in the same
  // stacking context).
  const containerBg = showBlurBg ? "transparent" : bgRgba;

  // Composite luminance of what the user actually sees through the lyric
  // text. Back-to-front: screen-behind → blurred album art (if active) →
  // user bg_color (alpha-blended at effectiveOpacity). Without this
  // composition, auto-contrast was sampling only the screen behind and
  // flipping the lyrics to dark text whenever the desktop happened to
  // be light — a regression for the v0.10.8 blurred-bg case where the
  // user actually sees a DARK surface (blur+brightness(0.62) is dark
  // for >95% of album arts) but the screen sample said "light, use
  // dark text" → unreadable dark-on-dark.
  const surfaceColor = computeSurfaceColor({
    screenColor,
    showBlurBg,
    tintColor,
    bgColor: effectiveBgColor,
    bgOpacityPct: effectiveOpacity,
  });

  // "Lightness score" = luminance × (1 - saturation). White / light gray
  // / pale-cream score high (saturation ≈ 0); tan, gold tint, saturated
  // pastels score lower even at high luminance. Dark text only kicks in
  // for the high-score case; otherwise white text wins (more readable
  // over tinted bright surfaces — Wes called this out on a gold album-
  // art tint where black-on-tan was hard to read).
  //
  // Hysteresis on the score so dynamic backgrounds don't flicker the
  // text color. Threshold 0.60 with a 0.05 band each side.
  const lightScore = surfaceColor ? lightnessScore(surfaceColor) : null;
  useEffect(() => {
    if (lightScore === null) return;
    setSurfaceIsLight((prev) => {
      if (prev === null) return lightScore > 0.60;
      if (prev && lightScore < 0.55) return false;
      if (!prev && lightScore > 0.65) return true;
      return prev;
    });
  }, [lightScore]);

  // Outer frame for all layouts: full window, visual chrome, vertical centering
  // of the inner content. The inner row (3-line / single-line) OR the inner
  // scrolling column (full-page) controls horizontal layout.
  // Hum brand mark. Centered ghost watermark — gets captured no matter how
  // the streamer pulls Hum onto stream (OBS browser source, window capture,
  // display capture). Always visible for now; the eventual Pro tier will
  // toggle this from Settings (free tier keeps the mark, Pro hides it).
  // PNG with luminance-as-alpha so the white logo paints cleanly over any
  // background without a hard-edged box.
  const watermark = (
    <img
      src="/hum-logo.png"
      alt=""
      draggable={false}
      style={{
        position: "absolute",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        height: "85%",
        width: "auto",
        zIndex: 1,
        opacity: 0.18,
        pointerEvents: "none",
        userSelect: "none",
      }}
    />
  );

  const containerStyle: React.CSSProperties = {
    position: "relative",
    height: "100vh",
    width: "100vw",
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    alignItems: layoutMode === "full_page" ? alignToFlex(settings.text_align) : "stretch",
    gap: layoutMode === "full_page" ? settings.line_padding_px : 0,
    padding: "12px 16px",
    boxSizing: "border-box",
    background: containerBg,
    fontFamily: `"${settings.font_family}", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`,
    userSelect: "none",
    cursor: isEdit ? "move" : "default",
    border: `1px dashed ${borderColor}`,
    borderRadius: 8,
    transition: "border-color 160ms ease, background 160ms ease",
    overflow: layoutMode === "full_page" ? "auto" : "hidden",
  };

  // Row layout used by 3-line and single-line: art on the left, lyrics column
  // on the right. The art's height comes from align-self: stretch on the art
  // element, which equals the row's height = lyrics column's natural height.
  //
  // `position: relative` lets this flex row stack ABOVE the absolutely-
  // positioned blurred album-art background layer when that's enabled.
  // Without positioning, CSS paint order puts absolute siblings on top of
  // static flex children regardless of DOM order.
  const innerRowStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: showArt && albumArt ? 14 : 0,
    width: "100%",
    minHeight: 0,
    position: "relative",
  };

  // Vertical stack that holds the update banner (when visible) above the
  // horizontal art+lyrics row. The ResizeObserver-tracked element (which
  // drives the window's auto-resize-to-content) sits here, NOT on
  // `innerRowStyle`, so banner height contributes to the window height.
  // When `UpdateBanner` returns null (idle phase), this collapses to just
  // the row's height — no visual change.
  // `position: relative` keeps this above the blurred album-art background.
  const outerStackStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    alignItems: "stretch",
    gap: 4,
    width: "100%",
    minHeight: 0,
    position: "relative",
  };

  const lyricsColStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    flex: 1,
    minWidth: 0, // allows ellipsis on overflowing lines inside the flex child
    alignItems: alignToFlex(settings.text_align),
    gap: settingsForRender.line_padding_px,
  };

  // Karaoke per-word render kicks in only when the current line came from a
  // source with word-level timing (SimpMusic richSyncLyrics). Falls through
  // to plain text otherwise.
  const nextLineTimeMs: number =
    lyrics?.lines[displayIdx + 1]?.time_ms ??
    track?.duration_ms ??
    (cur?.time_ms ?? 0) + 4000;
  const curKaraoke =
    cur && cur.words && cur.words.length > 0
      ? { words: cur.words, currentWordIdx, nextTimeMs: nextLineTimeMs }
      : undefined;

  if (layoutMode === "single_line") {
    return (
      <div
        {...dragProps}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={containerStyle}
      >
        {showBlurBg ? (
          <BlurredAlbumBg dataUrl={albumArt!.data_url} tintColor={bgRgba} />
        ) : null}
        <div {...dragProps} style={innerRowStyle}>
          {showArt && albumArt ? (
            <AlbumArtSide dataUrl={albumArt.data_url} size={artSize} dragRegion={isEdit} onClick={openArtistPanel} />
          ) : null}
          <div {...dragProps} ref={setLyricsColEl} style={lyricsColStyle}>
            {lyrics?.status === "ad" && settingsForRender.ad_break_promos_enabled ? (
              <PromoCard
                promo={lyrics.promo ?? null}
                textColor={effectiveTextColor}
                textColorDim={effectiveTextColorDim}
                textShadow={effectiveTextShadow}
                scaledFontSize={settingsForRender.font_size_px}
                layoutMode={layoutMode}
                dragRegion={isEdit}
              />
            ) : lyrics?.status === "ad" ? (
              <div style={{ color: effectiveTextColorDim, fontSize: settingsForRender.font_size_px * 0.6, textAlign: "center" }}>
                Ad break
              </div>
            ) : lyrics?.status === "unsupported" ? (
              <UnsupportedBlock
                track={track}
                sourceLabelText={sourceLabel(track?.source_app_id ?? null, null)}
                settings={settingsForRender}
                textShadow={effectiveTextShadow}
                dragRegion={isEdit}
              />
            ) : (
              <>
                <LineRow
                  text={middleText}
                  kind="cur"
                  dragRegion={isEdit}
                  settings={settingsForRender}
                  karaoke={curKaraoke}
                  textShadow={effectiveTextShadow}
                />
                {translationText ? (
                  <TranslationRow text={translationText} settings={settingsForRender} textShadow={effectiveTextShadow} />
                ) : null}
              </>
            )}
          </div>
          {track && lyrics?.status !== "unsupported" ? (
            <MetadataColumn
              track={track}
              textColor={effectiveTextColor}
              textColorDim={effectiveTextColorDim}
              textShadow={effectiveTextShadow}
              source={null}
              alignRight
              dragRegion={isEdit}
              adActive={track.ad_active}
            />
          ) : null}
        </div>
        {watermark}
      </div>
    );
  }

  if (layoutMode === "full_page") {
    const hasLines = lyrics?.status === "synced" && lyrics.lines.length > 0;
    return (
      <div
        {...dragProps}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={containerStyle}
      >
        {showBlurBg ? (
          <BlurredAlbumBg dataUrl={albumArt!.data_url} tintColor={bgRgba} />
        ) : null}
        {showArt && albumArt ? <AlbumArtBadge dataUrl={albumArt.data_url} onClick={openArtistPanel} /> : null}
        {openArtistPanel && (!showArt || !albumArt) && lyrics?.status !== "unsupported" ? (
          <ArtistInfoDot onClick={openArtistPanel} />
        ) : null}
        {lyrics?.status === "ad" && settingsForRender.ad_break_promos_enabled ? (
          <PromoCard
            promo={lyrics.promo ?? null}
            textColor={effectiveTextColor}
            textColorDim={effectiveTextColorDim}
            textShadow={effectiveTextShadow}
            scaledFontSize={settingsForRender.font_size_px}
            layoutMode={layoutMode}
            dragRegion={isEdit}
          />
        ) : lyrics?.status === "ad" ? (
          <div style={{ color: effectiveTextColorDim, fontSize: settingsForRender.font_size_px * 0.6, textAlign: "center" }}>
            Ad break
          </div>
        ) : lyrics?.status === "unsupported" ? (
          <UnsupportedBlock
            track={track}
            sourceLabelText={sourceLabel(track?.source_app_id ?? null, null)}
            settings={settingsForRender}
            textShadow={effectiveTextShadow}
            dragRegion={isEdit}
          />
        ) : hasLines ? (
          lyrics!.lines.map((line, i) => (
            <LineRow
              key={i}
              text={line.text}
              kind={i === displayIdx ? "cur" : "prev"}
              dragRegion={isEdit}
              settings={settingsForRender}
              scrollIntoView={i === displayIdx}
              karaoke={i === displayIdx ? curKaraoke : undefined}
              textShadow={effectiveTextShadow}
            />
          ))
        ) : (
          <LineRow
            text={middleText}
            kind="cur"
            dragRegion={isEdit}
            settings={settingsForRender}
            textShadow={effectiveTextShadow}
          />
        )}
        {watermark}
      </div>
    );
  }

  // Default: three-line scroll, with optional album art on the left.
  return (
    <div
      {...dragProps}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={containerStyle}
    >
      {showBlurBg ? (
        <BlurredAlbumBg dataUrl={albumArt!.data_url} tintColor={bgRgba} />
      ) : null}
      <NudgeBanner banner={nudgeBanner} />
      <div {...dragProps} ref={setInnerRowEl} style={outerStackStyle}>
        <UpdateBanner state={updateState} onInstall={installUpdate} />
        {openArtistPanel && (!showArt || !albumArt) && lyrics?.status !== "unsupported" ? (
          <ArtistInfoDot onClick={openArtistPanel} />
        ) : null}
        <div {...dragProps} style={innerRowStyle}>
          {showArt && albumArt ? (
            <AlbumArtSide dataUrl={albumArt.data_url} size={artSize} dragRegion={isEdit} onClick={openArtistPanel} />
          ) : null}
          <div {...dragProps} ref={setLyricsColEl} style={lyricsColStyle}>
          {lyrics?.status === "ad" && settingsForRender.ad_break_promos_enabled ? (
            <PromoCard
              promo={lyrics.promo ?? null}
              textColor={effectiveTextColor}
              textColorDim={effectiveTextColorDim}
              textShadow={effectiveTextShadow}
              scaledFontSize={settingsForRender.font_size_px}
              layoutMode={layoutMode}
              dragRegion={isEdit}
            />
          ) : lyrics?.status === "ad" ? (
            <div style={{ color: effectiveTextColorDim, fontSize: settingsForRender.font_size_px * 0.6, textAlign: "center" }}>
              Ad break
            </div>
          ) : lyrics?.status === "unsupported" ? (
            <UnsupportedBlock
              track={track}
              sourceLabelText={sourceLabel(track?.source_app_id ?? null, null)}
              settings={settingsForRender}
              textShadow={effectiveTextShadow}
              dragRegion={isEdit}
            />
          ) : (
            <>
              <LineRow text={prev?.text} kind="prev" dragRegion={isEdit} settings={settingsForRender} textShadow={effectiveTextShadow} />
              <LineRow
                text={middleText}
                kind="cur"
                dragRegion={isEdit}
                settings={settingsForRender}
                karaoke={curKaraoke}
                textShadow={effectiveTextShadow}
              />
              {translationText ? (
                <TranslationRow text={translationText} settings={settingsForRender} textShadow={effectiveTextShadow} />
              ) : (
                <LineRow text={next?.text} kind="next" dragRegion={isEdit} settings={settingsForRender} textShadow={effectiveTextShadow} />
              )}
            </>
          )}
          </div>
          {track && lyrics?.status !== "unsupported" ? (
            <MetadataColumn
              track={track}
              textColor={effectiveTextColor}
              textColorDim={effectiveTextColorDim}
              textShadow={effectiveTextShadow}
              source={null}
              alignRight
              dragRegion={isEdit}
              adActive={track.ad_active}
            />
          ) : null}
        </div>
      </div>
      {lyrics?.status === "unsupported" ? null : watermark}
    </div>
  );
}

// Update notice pinned to the top-right of the overlay. Default state is
// just a small gold dot — minimal pixel footprint, doesn't compete with
// the lyrics for attention. Hovering expands it into the full message.
// Click anywhere on the dot or expanded label installs the update and
// relaunches.
//
// Clickable in all three modes (edit / locked / ghost). For ghost mode
// the Rust side runs a cursor-poll worker that toggles
// ignore_cursor_events so this small top-right region receives clicks
// while the rest of the overlay stays click-through.
function UpdateBanner({
  state,
  onInstall,
}: {
  state:
    | { phase: "idle" }
    | { phase: "available"; version: string; update: Update }
    | { phase: "downloading"; version: string }
    | { phase: "ready"; version: string }
    | { phase: "error"; message: string };
  onInstall: () => void;
}) {
  const [hover, setHover] = useState(false);
  if (state.phase === "idle") return null;
  const clickable = state.phase === "available";

  // Sits in normal flow as the first child of `outerStackStyle` in the main
  // render tree — above the art+lyrics row, contributing its own height to
  // the window's auto-resize. `align-self: flex-start` keeps the dot+label
  // hugging the left edge instead of stretching to full width.
  const wrapperStyle: React.CSSProperties = {
    alignSelf: "flex-start",
    display: "flex",
    alignItems: "center",
    gap: 8,
    cursor: clickable ? "pointer" : "default",
    pointerEvents: clickable ? "auto" : "none",
    userSelect: "none",
    padding: "2px 4px",
    borderRadius: 4,
  };

  const dotStyle: React.CSSProperties = {
    display: "inline-block",
    width: 9,
    height: 9,
    borderRadius: "50%",
    flexShrink: 0,
  };

  const labelStyle: React.CSSProperties = {
    fontSize: 11,
    letterSpacing: 0.3,
    color: "rgba(234,234,234,0.92)",
    fontWeight: 500,
    fontVariantNumeric: "tabular-nums",
    textShadow: "0 1px 2px rgba(0,0,0,0.7)",
    overflow: "hidden",
    whiteSpace: "nowrap",
    transition: "opacity 180ms ease, max-width 220ms ease",
  };

  let dotColor = "#d4af37";
  let labelText: React.ReactNode = null;
  switch (state.phase) {
    case "available":
      dotColor = "#d4af37";
      labelText = `New Update Available: v${state.version} — Click to update`;
      break;
    case "downloading":
      dotColor = "#d4af37";
      labelText = `Installing v${state.version}…`;
      break;
    case "ready":
      dotColor = "#7ad07a";
      labelText = `v${state.version} installed — restarting`;
      break;
    case "error":
      dotColor = "#e57373";
      labelText = "Update failed";
      break;
  }

  // Force-show the label for non-"available" states so users see what's
  // happening during install / failure without needing to hover.
  const expanded = hover || state.phase !== "available";

  return (
    <div
      style={wrapperStyle}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={clickable ? onInstall : undefined}
      title={
        clickable
          ? `Install update v${(state as { version: string }).version} and restart`
          : undefined
      }
    >
      <span
        style={{
          ...dotStyle,
          background: dotColor,
          boxShadow: `0 0 6px ${dotColor}80`,
        }}
      />
      <span
        style={{
          ...labelStyle,
          opacity: expanded ? 1 : 0,
          maxWidth: expanded ? 360 : 0,
          color:
            state.phase === "error"
              ? "rgba(229,115,115,0.95)"
              : labelStyle.color,
        }}
      >
        {labelText}
      </span>
    </div>
  );
}

// Fallback "•••" affordance shown top-right when album art is not displayed.
// Mirrors UpdateBanner geometry — 9×9 dot, hover expands label, same anchor.
function ArtistInfoDot({ onClick }: { onClick: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={onClick}
      style={{
        alignSelf: "flex-start",
        display: "flex",
        alignItems: "center",
        gap: 6,
        cursor: "pointer",
        userSelect: "none",
        padding: "2px 4px",
        borderRadius: 4,
      }}
    >
      <span
        style={{
          display: "inline-block",
          width: 9,
          height: 9,
          borderRadius: "50%",
          background: "#d4af37",
          opacity: 0.7,
          boxShadow: "0 0 5px rgba(212,175,55,0.5)",
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: 11,
          letterSpacing: 0.3,
          color: "rgba(234,234,234,0.85)",
          fontWeight: 500,
          overflow: "hidden",
          whiteSpace: "nowrap",
          transition: "opacity 180ms ease, max-width 220ms ease",
          opacity: hover ? 1 : 0,
          maxWidth: hover ? 120 : 0,
        }}
      >
        Artist info
      </span>
    </div>
  );
}

// Promo card rendered in place of the lyric rows during an ad break.
// Three-line / full-page layout: supertitle / [icon + product name + tagline] / CTA.
// In single_line layout it collapses to one inline row.
function PromoCard({
  promo,
  textColor,
  textColorDim,
  textShadow,
  scaledFontSize,
  layoutMode,
  dragRegion,
}: {
  promo: Promo | null | undefined;
  textColor: string;
  textColorDim: string;
  textShadow: string;
  scaledFontSize: number;
  layoutMode: LayoutMode;
  dragRegion: boolean;
}) {
  const accent = promo?.accent_color ?? "#d4af37";
  const cta = promo?.cta_text ?? "Learn more →";
  const productName = promo?.product_name ?? "SYVR Studios";
  const tagline = promo?.tagline ?? "Tools and apps from the makers of Hum.";
  const url = promo?.url ?? "https://syvrstudios.com";
  const iconUrl = promo?.icon_url ?? null;
  const imageUrl = promo?.image_url ?? null;
  const altText = promo?.alt ?? `Sponsored content from ${productName}`;
  // Tracks whether the hero image failed to load (404, blocked, etc.) so
  // we can gracefully degrade to the text-driven layout instead of leaving
  // a broken-image gap in the overlay.
  const [imgFailed, setImgFailed] = useState(false);
  // Reset the failure flag whenever the source URL changes — a new promo
  // rotation shouldn't inherit the prior promo's failure state.
  useEffect(() => {
    setImgFailed(false);
  }, [imageUrl]);

  const handleClick = (e: React.MouseEvent) => {
    if (dragRegion) return;
    e.stopPropagation();
    openUrl(url).catch((err) => {
      console.error("[hum] opener failed to open promo URL:", url, err);
    });
  };
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};

  // Image-driven path: when `image_url` is set AND we're in a layout where
  // the image makes sense (three_line / full_page — single_line's ~26px
  // row height is too short for a hero image to read). Falls back to text
  // when image_url is unset, image load failed, or single_line layout.
  const useImageDriven =
    !!imageUrl && !imgFailed && layoutMode !== "single_line";

  if (useImageDriven) {
    return (
      <div
        {...drag}
        onClick={handleClick}
        className="hum-line-in"
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "stretch",
          justifyContent: "center",
          cursor: dragRegion ? "move" : "pointer",
          width: "100%",
          maxWidth: "92vw",
          overflow: "hidden",
        }}
      >
        <img
          src={imageUrl!}
          alt={altText}
          draggable={false}
          onError={() => setImgFailed(true)}
          style={{
            width: "100%",
            height: "auto",
            maxHeight: "100%",
            // contain so any aspect ratio the user designs at letterboxes
            // gracefully rather than cropping or distorting. Recommend 8:1
            // (1920×240) for edge-to-edge fit in the default overlay.
            objectFit: "contain",
            display: "block",
            pointerEvents: "none",
          }}
        />
      </div>
    );
  }

  if (layoutMode === "single_line") {
    return (
      <div
        {...drag}
        onClick={handleClick}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          cursor: dragRegion ? "move" : "pointer",
          maxWidth: "92vw",
          overflow: "hidden",
        }}
      >
        {iconUrl ? (
          <img
            src={iconUrl}
            alt=""
            draggable={false}
            style={{ width: 28, height: 28, borderRadius: 4, flexShrink: 0, pointerEvents: "none" }}
            onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
          />
        ) : null}
        <span style={{
          fontSize: scaledFontSize,
          color: textColor,
          textShadow,
          fontWeight: 600,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}>
          {productName}
        </span>
        <span style={{ fontSize: scaledFontSize * 0.65, color: textColorDim, textShadow, opacity: 0.85 }}>·</span>
        <span style={{
          fontSize: scaledFontSize * 0.65,
          color: textColorDim,
          textShadow,
          opacity: 0.85,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}>
          {tagline}
        </span>
        <span style={{ fontSize: scaledFontSize * 0.65, color: accent, textShadow, marginLeft: 6, whiteSpace: "nowrap" }}>
          {cta}
        </span>
      </div>
    );
  }

  // three_line + full_page: stacked card.
  return (
    <div
      {...drag}
      onClick={handleClick}
      className="hum-line-in"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 2,
        cursor: dragRegion ? "move" : "pointer",
        maxWidth: "92vw",
        overflow: "hidden",
      }}
    >
      <div style={{
        fontSize: Math.max(9, scaledFontSize * 0.38),
        color: textColorDim,
        textShadow,
        opacity: 0.7,
        letterSpacing: 0.4,
        textTransform: "uppercase",
        whiteSpace: "nowrap",
      }}>
        Brought to you by SYVR Studios
      </div>
      <div style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
      }}>
        {iconUrl ? (
          <img
            src={iconUrl}
            alt=""
            draggable={false}
            style={{ width: 32, height: 32, borderRadius: 4, flexShrink: 0, pointerEvents: "none" }}
            onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
          />
        ) : null}
        <div style={{
          display: "flex",
          flexDirection: "column",
          minWidth: 0,
          flex: 1,
        }}>
          <div style={{
            fontSize: scaledFontSize,
            color: textColor,
            textShadow,
            fontWeight: 600,
            lineHeight: 1.15,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}>
            {productName}
          </div>
          <div style={{
            fontSize: scaledFontSize * 0.55,
            color: textColorDim,
            textShadow,
            opacity: 0.85,
            lineHeight: 1.2,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}>
            {tagline}
          </div>
        </div>
      </div>
      <div style={{
        fontSize: scaledFontSize * 0.6,
        color: accent,
        textShadow,
        marginTop: 2,
        textDecoration: "underline",
        textUnderlineOffset: 2,
        whiteSpace: "nowrap",
      }}>
        {cta}
      </div>
    </div>
  );
}

// Right-side metadata column shown to the right of the lyrics in
// three_line + single_line layouts. Stacks three small read-only widgets:
//   1. Artist · Song · Album text line (top, dim, ellipsis on overflow)
//   2. Interpolated progress bar with `m:ss / m:ss` time readout (middle)
//   3. Source badge — short label of which app the metadata is coming
//      from (bottom, e.g. "Spotify", "Chrome", "Pandora")
// All driven entirely by data already on the snapshot — no Rust changes.
function MetadataColumn({
  track,
  textColor,
  textColorDim,
  textShadow,
  source,
  alignRight,
  dragRegion,
  adActive,
}: {
  track: CurrentTrack;
  textColor: string;
  textColorDim: string;
  textShadow: string;
  // Optional override: when set, prefer this label over source_app_id
  // (e.g. lyrics resolver knows the bridge surfaced "pandora-web" but
  // the OS still says "Chrome.exe"). Falsy → fall back to source_app_id.
  source: string | null;
  alignRight: boolean;
  dragRegion: boolean;
  adActive: boolean;
}) {
  const hasMeta =
    !!(track.title || track.artist || track.album);
  const hasDuration = track.duration_ms > 0;
  if (!hasMeta && !hasDuration) return null;
  const metaParts = [track.artist, track.title, track.album]
    .map((s) => (s || "").trim())
    .filter((s) => s.length > 0);
  const metaText = metaParts.join(" · ");
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};

  return (
    <div
      {...drag}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: alignRight ? "flex-end" : "flex-start",
        justifyContent: "center",
        gap: 4,
        flexShrink: 0,
        // Cap width so a really long Artist · Song · Album doesn't push
        // the lyrics column to nothing. ~38ch is enough for "Artist Name
        // · Song Title · Album Name" without being a wall.
        maxWidth: "38ch",
        minWidth: 0,
        // Self-applied gap so the column always has breathing room from
        // the lyrics column regardless of whether the row's flex `gap`
        // is set (the row's gap is only 14 when album art is showing —
        // 0 otherwise — and would otherwise sit flush against the lyrics).
        marginLeft: 14,
      }}
    >
      {/* Artist line: hidden during ads so it doesn't clash with the promo card.
          Uses a stronger halo shadow than the lyric `textShadow` so it stays
          legible over bright blurred-art backgrounds (the lyric column has
          the lyrics' own larger fonts to help; the metadata is small dim text
          that disappears against busy backgrounds without a halo). */}
      {!adActive && metaText ? (
        <div
          title={metaText}
          style={{
            fontSize: 11,
            letterSpacing: 0.3,
            color: textColorDim,
            textShadow: "0 1px 2px rgba(0,0,0,1), 0 0 6px rgba(0,0,0,0.85), 0 3px 10px rgba(0,0,0,0.55)",
            opacity: 0.85,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            maxWidth: "100%",
            textAlign: alignRight ? "right" : "left",
          }}
        >
          {metaText}
        </div>
      ) : null}
      {hasDuration ? (
        <ProgressBar
          track={track}
          textColor={textColor}
          textColorDim={textColorDim}
        />
      ) : null}
      {adActive ? (
        <AdBreakChip textShadow={textShadow} />
      ) : (
        <SourceBadge
          appId={track.source_app_id}
          overrideLabel={source}
          textColorDim={textColorDim}
          textShadow={textShadow}
        />
      )}
    </div>
  );
}

// Interpolates `track.position_ms` against wall time while playing so the
// bar visibly advances between server-pushed `timeline-changed` ticks
// (which arrive every 2 s). Re-renders every 500 ms via the parent's
// progressTick state — no internal timer here.
function ProgressBar({
  track,
  textColor,
  textColorDim,
}: {
  track: CurrentTrack;
  textColor: string;
  textColorDim: string;
}) {
  const duration = Math.max(0, track.duration_ms);
  // Wall-clock interpolation while playing; freeze at the reported
  // position when paused / stopped / etc. so the bar doesn't keep
  // creeping forward after a pause.
  let pos = track.position_ms;
  if (track.state === "playing") {
    pos = track.position_ms + Math.max(0, Date.now() - track.last_update_unix_ms);
  }
  pos = Math.max(0, Math.min(duration || pos, pos));
  const pct = duration > 0 ? Math.min(1, pos / duration) : 0;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "stretch",
        gap: 3,
        width: 160,
        maxWidth: "100%",
      }}
    >
      <div
        style={{
          fontSize: 10,
          color: textColorDim,
          textShadow: "0 1px 2px rgba(0,0,0,1), 0 0 6px rgba(0,0,0,0.85), 0 3px 10px rgba(0,0,0,0.55)",
          fontVariantNumeric: "tabular-nums",
          letterSpacing: 0.4,
          opacity: 0.85,
          display: "flex",
          justifyContent: "space-between",
        }}
      >
        <span>{fmtMs(pos)}</span>
        <span>{fmtMs(duration)}</span>
      </div>
      <div
        style={{
          position: "relative",
          height: 2,
          width: "100%",
          background: "rgba(127,127,127,0.35)",
          borderRadius: 1,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            bottom: 0,
            width: `${(pct * 100).toFixed(2)}%`,
            background: textColor,
            opacity: 0.85,
            transition: "width 480ms linear",
          }}
        />
      </div>
    </div>
  );
}

// Small "where is this playing from" chip. Maps a Windows SMTC app ID
// (usually a .exe or a UWP package family name) to a short human label.
// Bridge-resolved tracks (Pandora web/desktop) pass an explicit override.
function SourceBadge({
  appId,
  overrideLabel,
  textColorDim,
  textShadow,
}: {
  appId: string | null;
  overrideLabel: string | null;
  textColorDim: string;
  textShadow: string;
}) {
  const label = sourceLabel(appId, overrideLabel);
  if (!label) return null;
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 6px",
        borderRadius: 8,
        fontSize: 9.5,
        letterSpacing: 0.6,
        textTransform: "uppercase",
        color: textColorDim,
        textShadow,
        background: "rgba(127,127,127,0.18)",
        border: "1px solid rgba(127,127,127,0.25)",
        opacity: 0.85,
        whiteSpace: "nowrap",
      }}
    >
      {label}
    </div>
  );
}

function AdBreakChip({ textShadow }: { textShadow: string }) {
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 6px",
        borderRadius: 8,
        fontSize: 9.5,
        letterSpacing: 0.6,
        textTransform: "uppercase",
        color: "rgba(212, 175, 55, 0.95)",
        textShadow,
        background: "rgba(212, 175, 55, 0.12)",
        border: "1px solid rgba(212, 175, 55, 0.5)",
        opacity: 0.95,
        whiteSpace: "nowrap",
      }}
    >
      Ad Break
    </div>
  );
}

// Maps SMTC's `source_app_id` (varies wildly: `Spotify.exe`,
// `chrome.exe`, AUMIDs like `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`,
// MS Store package family names for Pandora desktop, etc.) to a short
// display label. Returns null when there's nothing recognizable — the
// badge hides entirely rather than showing a raw path.
function sourceLabel(appId: string | null, override: string | null): string | null {
  // Bridge label wins outright when it's surfaced (e.g. "pandora-web" /
  // "pandora-desktop" — the lyrics resolver knows the bridge identified
  // the song even though the OS reports Chrome.exe).
  if (override) {
    const o = override.toLowerCase();
    if (o.includes("pandora")) return "Pandora";
    if (o.includes("youtube")) return "YouTube";
    if (o.includes("spotify")) return "Spotify";
    if (o.includes("itunes") || o.includes("apple")) return "Apple Music";
  }
  if (!appId) return null;
  const a = appId.toLowerCase();
  if (a.includes("spotify")) return "Spotify";
  if (a.includes("pandora")) return "Pandora";
  if (a.includes("itunes")) return "iTunes";
  // Apple Music app on Windows reports as "AppleInc.AppleMusicWin_..." AUMID.
  if (a.includes("applemusic") || a.includes("apple.music")) return "Apple Music";
  if (a.includes("youtubemusic") || a.includes("youtube music")) return "YouTube Music";
  if (a.includes("tidal")) return "Tidal";
  if (a.includes("amazonmusic") || a.includes("amazon music")) return "Amazon Music";
  if (a.includes("deezer")) return "Deezer";
  if (a.includes("vlc")) return "VLC";
  if (a.includes("foobar")) return "foobar2000";
  if (a.includes("musicbee")) return "MusicBee";
  if (a.includes("winamp")) return "Winamp";
  if (a.includes("wmplayer") || a.includes("windowsmedia")) return "Windows Media";
  if (a.includes("groove")) return "Groove";
  if (a.endsWith("chrome.exe") || a.includes("chrome")) return "Chrome";
  if (a.endsWith("msedge.exe") || a.includes("edge")) return "Edge";
  if (a.endsWith("firefox.exe") || a.includes("firefox")) return "Firefox";
  if (a.endsWith("brave.exe") || a.includes("brave")) return "Brave";
  if (a.includes("opera")) return "Opera";
  if (a.includes("arc")) return "Arc";
  if (a.includes("zen")) return "Zen";
  // Last resort: take the basename, strip .exe, capitalize first char.
  const last = appId.split(/[\\/]/).pop() ?? appId;
  const stripped = last.replace(/\.exe$/i, "").trim();
  if (!stripped) return null;
  // AUMID format usually has `Publisher.AppName_hash!Entry`; pull AppName.
  const aumid = /^[^.]+\.([^_!]+)/.exec(stripped);
  const name = (aumid?.[1] ?? stripped).slice(0, 14);
  if (!name) return null;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

// Brief 1.5s indicator showing the current lyric-offset nudge value when
// the user presses Ctrl+Alt+[ / Ctrl+Alt+]. Auto-fades out via a timer
// so it doesn't sit on top of the lyrics permanently.
function NudgeBanner({ banner }: { banner: { value: number; until: number } | null }) {
  const [, force] = useState(0);
  useEffect(() => {
    if (!banner) return;
    const remaining = banner.until - Date.now();
    if (remaining <= 0) return;
    const t = setTimeout(() => force((n) => n + 1), remaining + 50);
    return () => clearTimeout(t);
  }, [banner]);
  if (!banner || Date.now() > banner.until) return null;
  const sign = banner.value >= 0 ? "+" : "";
  return (
    <div
      style={{
        position: "absolute",
        top: 4,
        right: 8,
        background: "rgba(0,0,0,0.55)",
        color: "#d4af37",
        padding: "2px 8px",
        borderRadius: 6,
        fontSize: 11,
        fontVariantNumeric: "tabular-nums",
        pointerEvents: "none",
        letterSpacing: 0.5,
      }}
    >
      lyric offset {sign}{banner.value} ms
    </div>
  );
}

// Heavily blurred + dimmed copy of the current track's album art, used as
// the overlay's background layer ("Apple Music Now Playing" style). Two
// stacked absolute divs:
//   1. The blurred image — `inset: -48px` extends past the container edges
//      so the blur's soft falloff doesn't bleed in as transparent fringes.
//   2. The user's bg_color (rgba string, may be `"transparent"`) as a tint
//      on top of the blur. Keeps the bg-opacity slider working as a darken /
//      lighten control over the blurred art.
// Container needs `overflow: hidden` (the three layout containers all set
// it) so the oversized inset doesn't leak outside the window chrome.
function BlurredAlbumBg({
  dataUrl,
  tintColor,
}: {
  dataUrl: string;
  tintColor: string;
}) {
  // Outer wrapper pins to the container edges and clips its own overflow.
  // The full-page layout uses `overflow: auto`, so without this wrapper
  // the inner negative-inset blur would create phantom scroll content.
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        overflow: "hidden",
        pointerEvents: "none",
        borderRadius: 8,
      }}
    >
      <div
        style={{
          position: "absolute",
          // Extend past the wrapper so the blur's soft falloff doesn't
          // show as a transparent halo at the edges. 48px > the 40px
          // blur radius below.
          top: -48,
          left: -48,
          right: -48,
          bottom: -48,
          backgroundImage: `url(${dataUrl})`,
          backgroundSize: "cover",
          backgroundPosition: "center",
          filter: "blur(40px) saturate(1.35) brightness(0.62)",
        }}
      />
      {tintColor !== "transparent" ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: tintColor,
          }}
        />
      ) : null}
    </div>
  );
}

function AlbumArtBadge({ dataUrl, onClick }: { dataUrl: string; onClick?: () => void }) {
  const [hover, setHover] = useState(false);
  const isClickable = !!onClick;
  // Used by full-page layout — small absolute-positioned thumbnail in the
  // corner since side-by-side would conflict with the scrolling column.
  return (
    <img
      src={dataUrl}
      alt=""
      draggable={false}
      onClick={(e) => { if (isClickable) { e.stopPropagation(); onClick(); } }}
      onMouseEnter={() => isClickable && setHover(true)}
      onMouseLeave={() => isClickable && setHover(false)}
      style={{
        position: "absolute",
        top: 8,
        left: 8,
        width: 40,
        height: 40,
        borderRadius: 4,
        objectFit: "cover",
        boxShadow: "0 2px 8px rgba(0,0,0,0.6)",
        opacity: 0.9,
        pointerEvents: isClickable ? "auto" : "none",
        cursor: isClickable ? "pointer" : "default",
        outline: isClickable && hover ? "1.5px solid rgba(212,175,55,0.5)" : "none",
        outlineOffset: 2,
      }}
    />
  );
}

// Side-by-side album art used by 3-line and single-line layouts. The size
// prop comes from a ResizeObserver on the lyrics column in the parent — this
// is exact, not approximate, because pure CSS (align-self:stretch +
// aspect-ratio:1) was off by a handful of px when the image's intrinsic
// dimensions interacted with flex's hypothetical-size pass.
//
// `dragRegion` makes the album-art square a Tauri window drag handle when
// edit mode is on, matching the rest of the overlay's chrome — the user
// should be able to grab the window from anywhere inside it, not only
// the lyric text.
function AlbumArtSide({
  dataUrl,
  size,
  dragRegion,
  onClick,
}: {
  dataUrl: string;
  size: number;
  dragRegion: boolean;
  onClick?: () => void;
}) {
  const [hover, setHover] = useState(false);
  // Floor at 40 so a tiny font doesn't shrink the art to a sliver.
  const px = Math.max(40, size);
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};
  const isClickable = !!onClick;

  return (
    <div
      {...drag}
      onClick={(e) => {
        if (isClickable) {
          e.stopPropagation();
          onClick();
        }
      }}
      onMouseEnter={() => isClickable && setHover(true)}
      onMouseLeave={() => isClickable && setHover(false)}
      style={{
        width: px,
        height: px,
        flexShrink: 0,
        position: "relative",
        cursor: isClickable ? "pointer" : "default",
        outline: isClickable && hover ? "1.5px solid rgba(212,175,55,0.5)" : "none",
        outlineOffset: 2,
        borderRadius: 6,
      }}
    >
      <img
        src={dataUrl}
        alt=""
        draggable={false}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "cover",
          borderRadius: 6,
          boxShadow: "0 2px 8px rgba(0,0,0,0.6)",
          display: "block",
          pointerEvents: "none",
        }}
      />
    </div>
  );
}

function TranslationRow({
  text,
  settings,
  textShadow,
}: {
  text: string;
  settings: Settings;
  textShadow?: string;
}) {
  return (
    <div
      style={{
        fontSize: Math.max(8, settings.font_size_px * 0.55),
        fontWeight: 400,
        color: settings.text_color_dim,
        textAlign: settings.text_align,
        textShadow: textShadow ?? "0 1px 2px rgba(0,0,0,0.9), 0 3px 10px rgba(0,0,0,0.55)",
        opacity: 0.85,
        lineHeight: 1.2,
        maxWidth: "92vw",
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
        fontStyle: "italic",
      }}
    >
      {text}
    </div>
  );
}

// Replaces the prev/cur/next lyric stack when status === "unsupported".
// The user is doing something we can't lyric (watching Netflix, on
// Pandora-web, browsing). Instead of three empty ♪ placeholders we show:
//   • a centered "<lead> <highlight>" line where the service portion is
//     painted in its brand color (Netflix red, Twitch purple, etc.)
//   • a smaller "X min remaining" line below if the source exposed a
//     duration (Netflix in Chrome does — the metadata column already
//     shows MM:SS / MM:SS; this is the at-a-glance version).
// The component re-renders on `progressTick` state changes (passed via
// `tickKey` so React invalidates) so the remaining-minutes count updates
// without a per-component timer.
function UnsupportedBlock({
  track,
  sourceLabelText,
  settings,
  textShadow,
  dragRegion,
}: {
  track: CurrentTrack | null;
  sourceLabelText: string | null;
  settings: Settings;
  textShadow: string;
  dragRegion: boolean;
}) {
  const title = (track?.title ?? "").trim();
  const isPandoraTab = title.endsWith("Now Playing on Pandora");
  const isVideoService = !!title && KNOWN_VIDEO_SERVICES.test(title);
  const titleIsJustSource =
    !!sourceLabelText && title.toLowerCase() === sourceLabelText.toLowerCase();

  // Decide the "lead text + highlight word" split that matches the
  // statusLine() string but keeps the service name separable so we can
  // brand-color just that token.
  let lead = "";
  let highlight = "";
  if (isPandoraTab) {
    lead = "Hum's tuned in —";
    highlight = "Pandora";
  } else if (isVideoService) {
    lead = "Watching";
    highlight = title;
  } else if (title && !titleIsJustSource) {
    lead = "";
    highlight = title;
  } else if (sourceLabelText) {
    lead = "Hum's tuned in —";
    highlight = sourceLabelText;
  } else {
    lead = "Hum's tuned in";
    highlight = "";
  }

  const color = serviceBrandColor(highlight) ?? settings.text_color;

  // Remaining time. Wall-clock interpolation while playing so the count
  // updates between server pushes.
  let remaining: string | null = null;
  if (track && track.duration_ms > 0) {
    let pos = track.position_ms;
    if (track.state === "playing") {
      pos = track.position_ms + Math.max(0, Date.now() - track.last_update_unix_ms);
    }
    const remainingMs = Math.max(0, track.duration_ms - pos);
    const min = Math.round(remainingMs / 60000);
    if (min >= 1) {
      remaining = `${min} min remaining`;
    } else if (remainingMs >= 5000) {
      remaining = `less than a minute remaining`;
    }
  }

  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};

  // Visual hierarchy: small dim "Watching" cap above + GIANT brand
  // headline + clear remaining-time line below. Sizes scale with the
  // user's font_size_px so a user-shrunk overlay shrinks gracefully.
  const headlineFontSize = Math.max(settings.font_size_px * 1.6, 32);
  const captionFontSize = Math.max(headlineFontSize * 0.28, 11);
  const sublineFontSize = Math.max(headlineFontSize * 0.4, 14);
  // Soft halo behind the headline using the service brand color — adds
  // a colored breath to the otherwise gray plate without taking over.
  // Falls back to a neutral dark glow when there's no brand color.
  const haloColor = serviceBrandColor(highlight);
  const halo = haloColor
    ? `0 0 64px ${haloColor}55, 0 0 22px ${haloColor}77`
    : undefined;

  return (
    <div
      {...drag}
      style={{
        alignSelf: "stretch",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 4,
        position: "relative",
        width: "100%",
        maxWidth: "100%",
        // Soft service-colored glow behind the block.
        textShadow: halo,
      }}
    >
      {lead ? (
        <div
          style={{
            fontSize: captionFontSize,
            fontWeight: 600,
            color: settings.text_color_dim,
            textShadow,
            letterSpacing: 2,
            textTransform: "uppercase",
            opacity: 0.7,
            lineHeight: 1.2,
            textAlign: "center",
          }}
        >
          {lead.replace(/[—\s]+$/, "")}
        </div>
      ) : null}
      {highlight ? (
        <div
          style={{
            fontSize: headlineFontSize,
            fontWeight: 800,
            color,
            // Layered shadow: solid dark contact + the service-color halo.
            textShadow: halo
              ? `0 2px 4px rgba(0,0,0,0.95), 0 0 28px ${haloColor}88, 0 0 60px ${haloColor}55`
              : textShadow,
            letterSpacing: -0.5,
            lineHeight: 1.05,
            transition: "color 220ms ease",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            maxWidth: "100%",
            textAlign: "center",
          }}
        >
          {highlight}
        </div>
      ) : null}
      {remaining ? (
        <div
          style={{
            fontSize: sublineFontSize,
            fontWeight: 500,
            color: settings.text_color,
            textShadow: "0 1px 2px rgba(0,0,0,0.95), 0 0 8px rgba(0,0,0,0.6)",
            letterSpacing: 0.3,
            opacity: 0.85,
            marginTop: 6,
            textAlign: "center",
          }}
        >
          {remaining}
        </div>
      ) : null}
    </div>
  );
}

function LineRow({
  text,
  kind,
  dragRegion,
  settings,
  scrollIntoView,
  karaoke,
  textShadow,
}: {
  text: string | undefined;
  kind: "prev" | "cur" | "next";
  dragRegion: boolean;
  settings: Settings;
  scrollIntoView?: boolean;
  karaoke?: { words: WordSpan[]; currentWordIdx: number; nextTimeMs: number };
  textShadow?: string;
}) {
  const isCur = kind === "cur";
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (scrollIntoView && ref.current) {
      ref.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [scrollIntoView]);
  // Single-line for ALL rows (prev / cur / next). Wrapping the cur row to 2
  // lines made long-line songs readable but caused the lyrics column height
  // — and therefore the side-by-side album art height — to jitter every
  // time a long line came up. Long lines now ellipsis-truncate instead.
  // Trade-off: catches truncated for line-stability + constant art size.
  const wrapStyle: React.CSSProperties = {
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
  };
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};
  const useKaraoke = isCur && !!karaoke && !!text;
  // settings.font_size_px is already pre-scaled by the window-resize
  // factor in Overlay (settingsForRender). Just apply the prev/next dim
  // shrink ratio on top.
  const sizePx = isCur ? settings.font_size_px : Math.max(8, settings.font_size_px * 0.6);
  return (
    <div
      ref={ref}
      {...drag}
      style={{
        fontSize: sizePx,
        fontWeight: isCur ? settings.font_weight : 400,
        // When karaoke is on, the per-word spans own their color. The container
        // color still matters for any leftover non-span text (none in practice).
        color: isCur ? settings.text_color : settings.text_color_dim,
        textAlign: settings.text_align,
        textShadow: textShadow ?? "0 1px 2px rgba(0,0,0,0.9), 0 3px 10px rgba(0,0,0,0.55)",
        opacity: text ? 1 : 0.2,
        // Disable color-transition on the container while karaoke is active
        // so it doesn't fight the per-word transitions.
        transition: useKaraoke
          ? "opacity 220ms ease"
          : "opacity 220ms ease, color 220ms ease",
        lineHeight: 1.2,
        maxWidth: "92vw",
        letterSpacing: isCur ? 0.2 : 0,
        // Positioned so this row paints above the absolutely-positioned
        // blurred album-art background (full-page layout renders each
        // LineRow as a direct flex child of the container).
        position: "relative",
        ...wrapStyle,
      }}
    >
      {/* Wrapper span is keyed by the rendered line text so that whenever a
          line advances (prev/cur/next all update), the wrapper remounts
          and the `hum-line-in` CSS animation fires — a brief lift-from-
          below + fade-in. inline-block is required because the animation
          uses `transform`, which doesn't apply to plain inline boxes. */}
      <span
        key={text ?? ""}
        className="hum-line-in"
        style={{ display: "inline-block" }}
      >
        {useKaraoke
          ? karaoke!.words.map((w, i) => {
              const idx = karaoke!.currentWordIdx;
              const isPast = idx > i;
              const isCurrent = idx === i;
              const dur = wordDurationMs(karaoke!.words, i, karaoke!.nextTimeMs);
              // Karaoke wipe: each word is filled with a two-stop gradient
              // (lit on the left half, dim on the right half) clipped to
              // the text glyphs via background-clip: text. background-
              // position slides the gradient under the glyphs:
              //   past    → 0% 0%   (lit half covers the word)
              //   current → animates 100% → 0% (left-to-right sweep)
              //   future  → 100% 0% (dim half covers the word)
              // Smooth fill instead of the old abrupt dim→lit step.
              const bgPos = isPast || isCurrent ? "0% 0%" : "100% 0%";
              return (
                <span
                  key={i}
                  style={{
                    background: `linear-gradient(to right, ${settings.text_color} 0%, ${settings.text_color} 50%, ${settings.text_color_dim} 50%, ${settings.text_color_dim} 100%)`,
                    backgroundSize: "200% 100%",
                    backgroundPosition: bgPos,
                    backgroundClip: "text",
                    WebkitBackgroundClip: "text",
                    color: "transparent",
                    WebkitTextFillColor: "transparent",
                    transition: isCurrent ? `background-position ${dur}ms linear` : "none",
                  }}
                >
                  {w.text}
                </span>
              );
            })
          : text || "♪"}
      </span>
    </div>
  );
}

// A word's "duration" = time until the next word starts (or until the next
// LINE starts for the last word). Floored at 80ms so the color transition
// stays visible on tightly-packed words.
function wordDurationMs(words: WordSpan[], idx: number, lineEndMs: number): number {
  const w = words[idx];
  if (!w) return 500;
  const nextStart = idx + 1 < words.length ? words[idx + 1].time_ms : lineEndMs;
  return Math.max(80, nextStart - w.time_ms);
}

// Tab titles equal to one of these mean the user is on the service but
// the media session didn't expose the actual show/video title (usually
// DRM or service policy). Frame these as "Watching X" rather than
// rendering "Netflix" as if it were a song name.
const KNOWN_VIDEO_SERVICES = /^(netflix|youtube|twitch|hulu|disney\+|prime video|amazon prime|hbo|max|peacock|apple tv|paramount\+|crunchyroll)$/i;

// Brand colors for the unsupported-state "Watching X" / "Hum's tuned in
// — X" lines. Keyed by lowercase service name. Returns null for unknown
// services; the caller falls back to the user's text color.
function serviceBrandColor(name: string | null | undefined): string | null {
  if (!name) return null;
  const k = name.toLowerCase();
  if (k === "netflix") return "#e50914";
  if (k === "youtube") return "#ff0000";
  if (k === "twitch") return "#9146ff";
  if (k === "hulu") return "#1ce783";
  if (k === "disney+") return "#1f80e0";
  if (k === "prime video" || k === "amazon prime") return "#00a8e1";
  if (k === "hbo" || k === "max") return "#6e2da3";
  if (k === "peacock") return "#fa6400";
  if (k === "apple tv") return "#e8e8e8";
  if (k === "paramount+") return "#0064ff";
  if (k === "crunchyroll") return "#f47521";
  if (k === "pandora") return "#3668ff";
  if (k === "spotify") return "#1ed760";
  if (k === "youtube music") return "#ff0000";
  if (k === "apple music") return "#fa5760";
  if (k === "tidal") return "#000000";
  return null;
}

function statusLine(l: CurrentLyrics, t: CurrentTrack | null): string {
  switch (l.status) {
    case "fetching":
      return t?.title ? `♪ fetching — ${t.title}` : "♪ fetching…";
    case "not_found":
      return t?.title
        ? `♪ no lyrics for ${t.title}`
        : "♪ no lyrics on LRCLib";
    case "unsupported": {
      // Source publishes audio but no metadata Hum can decode (Pandora web)
      // OR exposes a page/app title that isn't a song (Netflix, YouTube,
      // Chrome on a random site). Surface what we CAN see instead of
      // dead-ending with "track info unavailable for this source".
      const title = (t?.title ?? "").trim();
      const src = sourceLabel(t?.source_app_id ?? null, null);
      // Pandora web's tab title is the literal string "Now Playing on
      // Pandora", not a song — show a clear source line instead.
      // No ♪ prefix on these — they're not about music. The cur line in
      // the unsupported case describes what the user is doing (watching
      // a video, on a non-music site), not "there's a song with no
      // lyrics available." Music notes belong on the music-status
      // branches (fetching, no_lyrics, instrumental, plain).
      if (title.endsWith("Now Playing on Pandora")) {
        return "Hum's tuned in — Pandora";
      }
      // Known video services where the tab title equals the service name —
      // Chrome's media session doesn't expose the show name (DRM/policy),
      // so we get title = "Netflix" instead of title = "Stranger Things".
      // Frame as "Watching X" so it doesn't read as a song.
      if (title && KNOWN_VIDEO_SERVICES.test(title)) {
        return `Watching ${title}`;
      }
      // Generic case: show the title if it isn't just the source/site name,
      // otherwise frame by source.
      const titleIsJustSource =
        !!src && title.toLowerCase() === src.toLowerCase();
      if (title && !titleIsJustSource) {
        return title;
      }
      if (src) return `Hum's tuned in — ${src}`;
      return "Hum's tuned in";
    }
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

function alignToFlex(a: TextAlign): React.CSSProperties["alignItems"] {
  if (a === "left") return "flex-start";
  if (a === "right") return "flex-end";
  return "center";
}

// The overlay window is resizable via the edit-mode drag corners. We want
// the lyric text + gaps + album art to scale WITH the window — both width
// AND height — so dragging it smaller shrinks the whole composition
// instead of just cropping. The slider value in Settings is the literal
// pixel size at the baseline 720×200 window. Smaller window scales down,
// larger scales up; whichever dimension is the tighter constraint wins
// (so text never overflows when only the width is dragged narrower).
const BASELINE_WINDOW_W_PX = 1100;
const BASELINE_WINDOW_H_PX = 200;

// Convert hex (#rrggbb) + opacity-percent to rgba(...) string. Also accepts
// rgb(r, g, b) input from mixHexWithRgb's output. Falls back to transparent
// for invalid input so a typo can't break rendering.
function colorWithOpacity(color: string, opacityPct: number): string {
  const a = Math.max(0, Math.min(1, opacityPct / 100));
  if (a === 0) return "transparent";
  const hex = /^#([0-9a-fA-F]{6})$/.exec(color);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return `rgba(${(n >> 16) & 0xff}, ${(n >> 8) & 0xff}, ${n & 0xff}, ${a.toFixed(3)})`;
  }
  const rgb = /^rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$/.exec(color);
  if (rgb) {
    return `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${a.toFixed(3)})`;
  }
  return "transparent";
}

// Linear-interpolate a hex color (#rrggbb) toward an RGB color. t=0 → all hex,
// t=1 → all rgb. Returns rgb(r,g,b). Used for tinting the user's bg_color
// toward the album-art dominant color.
// Perceived luminance (0..1) of the surface the user sees through the
// lyric text, composited back-to-front from:
//   1. Screen behind the overlay window (when no own bg is present).
//   2. Blurred album-art layer — dimmed by brightness(0.62) in CSS, so we
//      derive its luminance from the extracted dominant tint scaled by
//      the same multiplier.
//   3. User bg_color alpha-blended over the above at bg_opacity/100.
//
// Returns null when there's NO signal at all (no screen sample yet, no
// blur, no opacity) — auto-contrast then stays neutral.
function computeSurfaceColor(args: {
  screenColor: { r: number; g: number; b: number } | null;
  showBlurBg: boolean;
  tintColor: { r: number; g: number; b: number } | null;
  bgColor: string;
  bgOpacityPct: number;
}): { r: number; g: number; b: number } | null {
  const { screenColor, showBlurBg, tintColor, bgColor, bgOpacityPct } = args;

  // Layer 1: what's BEHIND the overlay (or what the blurred art replaces).
  let bg: { r: number; g: number; b: number } | null = null;
  if (showBlurBg && tintColor) {
    // Blurred bg layer. CSS filter brightness(0.62) is applied in CSS,
    // so dim the dominant tint by the same multiplier.
    bg = {
      r: Math.round(tintColor.r * 0.62),
      g: Math.round(tintColor.g * 0.62),
      b: Math.round(tintColor.b * 0.62),
    };
  } else if (screenColor !== null) {
    bg = screenColor;
  }

  // Layer 2: user bg_color painted on top at bg_opacity. Alpha-blends
  // toward the user color and away from whatever was behind.
  if (bgOpacityPct > 0) {
    const userColor = hexToRgb(bgColor);
    if (userColor !== null) {
      const alpha = Math.min(1, Math.max(0, bgOpacityPct / 100));
      bg = bg !== null
        ? {
            r: Math.round(userColor.r * alpha + bg.r * (1 - alpha)),
            g: Math.round(userColor.g * alpha + bg.g * (1 - alpha)),
            b: Math.round(userColor.b * alpha + bg.b * (1 - alpha)),
          }
        : userColor;
    }
  }

  return bg;
}

// Per-component perceived-luminance weighting (Rec. 601). 0..1.
function rgbLuminance(c: { r: number; g: number; b: number }): number {
  return (0.299 * c.r + 0.587 * c.g + 0.114 * c.b) / 255;
}

// HSV saturation: (max - min) / max. Returns 0 for pure black/gray (where
// max == min or max == 0). White is 0; tan/gold/saturated colors trend
// toward 1.
function rgbSaturationHsv(c: { r: number; g: number; b: number }): number {
  const max = Math.max(c.r, c.g, c.b);
  if (max === 0) return 0;
  const min = Math.min(c.r, c.g, c.b);
  return (max - min) / max;
}

// Single score combining luminance and lack-of-saturation. Drives the
// "use dark text" decision. Pure white → 1.0; light gray → ~0.8; pale
// cream → ~0.85; tan / gold tint → ~0.4; saturated colors → < 0.3.
function lightnessScore(c: { r: number; g: number; b: number }): number {
  return rgbLuminance(c) * (1 - rgbSaturationHsv(c));
}

// Parse a #rrggbb hex into RGB components. Returns null for malformed
// input so callers can distinguish "no signal" from "looks black."
function hexToRgb(
  hex: string,
): { r: number; g: number; b: number } | null {
  const m = /^#([0-9a-fA-F]{6})$/.exec(hex);
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return {
    r: (n >> 16) & 0xff,
    g: (n >> 8) & 0xff,
    b: n & 0xff,
  };
}

function mixHexWithRgb(
  hex: string,
  rgb: { r: number; g: number; b: number },
  t: number,
): string {
  const m = /^#([0-9a-fA-F]{6})$/.exec(hex);
  if (!m) return `rgb(${rgb.r}, ${rgb.g}, ${rgb.b})`;
  const n = parseInt(m[1], 16);
  const ar = (n >> 16) & 0xff;
  const ag = (n >> 8) & 0xff;
  const ab = n & 0xff;
  const k = Math.max(0, Math.min(1, t));
  const r = Math.round(ar * (1 - k) + rgb.r * k);
  const g = Math.round(ag * (1 - k) + rgb.g * k);
  const b = Math.round(ab * (1 - k) + rgb.b * k);
  return `rgb(${r}, ${g}, ${b})`;
}

// Sample the dominant color from an album-art data URL by drawing it to a
// tiny offscreen canvas and averaging the visible pixels. Skips near-black
// pixels (album art often has dark borders) and near-transparent pixels so
// the average leans toward the real artwork color rather than the bars.
// Returns null on any failure (cors, image decode, no usable pixels).
function extractDominantColor(
  dataUrl: string,
): Promise<{ r: number; g: number; b: number } | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      try {
        const size = 32;
        const canvas = document.createElement("canvas");
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext("2d");
        if (!ctx) return resolve(null);
        ctx.drawImage(img, 0, 0, size, size);
        const data = ctx.getImageData(0, 0, size, size).data;
        let sr = 0, sg = 0, sb = 0, n = 0;
        for (let i = 0; i < data.length; i += 4) {
          if (data[i + 3] < 128) continue;
          const lum = data[i] + data[i + 1] + data[i + 2];
          if (lum < 30) continue;
          if (lum > 720) continue; // very near-white, often a single-color BG
          sr += data[i];
          sg += data[i + 1];
          sb += data[i + 2];
          n++;
        }
        if (n === 0) return resolve(null);
        resolve({
          r: Math.round(sr / n),
          g: Math.round(sg / n),
          b: Math.round(sb / n),
        });
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = dataUrl;
  });
}
