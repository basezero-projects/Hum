import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type {
  CurrentLyrics,
  CurrentTrack,
  LayoutMode,
  LyricLine,
  OverlayMode,
  Settings,
  TextAlign,
  WordSpan,
} from "./types";

const DEFAULT_SETTINGS: Settings = {
  last_mode: "edit",
  anticipate_ms: 500,
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
  auto_contrast: true,
  streamer_enabled: false,
  streamer_port: 38247,
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
  // Background luminance from contrast.rs's screen-capture worker. Null
  // until the first sample arrives. Frontend uses hysteresis around the
  // threshold to avoid color-flicker on dynamic backgrounds (videos).
  const [bgIsLight, setBgIsLight] = useState<boolean | null>(null);
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
          // Hysteresis around 0.5: light → dark requires drop below 0.45,
          // dark → light requires rise above 0.55. Stops flickering when
          // bg sits near the threshold (e.g. mid-gray desktop).
          setBgIsLight((prev) => {
            const lum = e.payload.luminance;
            if (prev === null) return lum > 0.5;
            if (prev && lum < 0.45) return false;
            if (!prev && lum > 0.55) return true;
            return prev;
          });
        },
      ),
    ];

    invoke<CurrentTrack>("get_current_track")
      .then((t) => applyTrack(t, "track"))
      .catch(() => {});
    invoke<CurrentLyrics>("get_current_lyrics")
      .then(applyLyrics)
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

  // ─── TEMPORARY DEMO: force the update banner to appear so Wes can see
  // what it looks like without an actual release endpoint configured.
  // REMOVE BEFORE NEXT COMMIT.
  useEffect(() => {
    const t = window.setTimeout(() => {
      setUpdateState({ phase: "available", version: "0.11.0-demo", update: null as unknown as Update });
      invoke("set_update_indicator", { pendingVersion: "0.11.0-demo" }).catch(() => {});
    }, 800);
    return () => window.clearTimeout(t);
  }, []);
  // ─── END DEMO

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
  if (lyrics && lyrics.status === "synced" && lyrics.lines.length > 0 && displayIdx >= 0) {
    cur = lyrics.lines[displayIdx];
    prev = displayIdx > 0 ? lyrics.lines[displayIdx - 1] : undefined;
    next =
      displayIdx + 1 < lyrics.lines.length
        ? lyrics.lines[displayIdx + 1]
        : undefined;
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

  // Album art only shows for the *currently playing* track — past art lingers
  // in state until the next album-art-loaded event arrives.
  const showArt =
    settings.show_album_art &&
    !!albumArt &&
    !!track &&
    albumArt.title === track.title &&
    albumArt.artist === track.artist;

  // Edit mode: drag-region active, dashed border on hover, move cursor.
  // Locked: no drag, no border. Ghost: same; the window is also click-through
  // via set_ignore_cursor_events on the Rust side.
  const isEdit = mode === "edit";
  const dragProps = isEdit ? { "data-tauri-drag-region": true } : {};
  const borderColor = isEdit && hovered
    ? "rgba(212, 175, 55, 0.85)"
    : "transparent";

  const layoutMode: LayoutMode = settings.layout_mode;
  // Auto-contrast override: when the toggle is on AND we have a luminance
  // read, replace the user's text colors with high-contrast values.
  const autoColorActive = settings.auto_contrast && bgIsLight !== null;
  const effectiveTextColor = autoColorActive
    ? (bgIsLight ? "#0a0a0a" : "#ffffff")
    : settings.text_color;
  const effectiveTextColorDim = autoColorActive
    ? (bgIsLight ? "rgba(0,0,0,0.45)" : "rgba(255,255,255,0.45)")
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
  // Text shadow is the opposite color of the text — black halo over light
  // bg made the dark text invisible. When auto-contrast says the bg is
  // light, we render dark text + WHITE halo. Otherwise default = light
  // text + BLACK halo (works against any dark / mid-tone background).
  const effectiveTextShadow =
    autoColorActive && bgIsLight
      ? "0 2px 6px rgba(255,255,255,0.95), 0 0 14px rgba(255,255,255,0.7)"
      : "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)";
  // When tint is on AND we have a color extracted from the current art, blend
  // the user's bg_color with the tint at 50/50 in RGB. Force a minimum 22%
  // opacity so the toggle is visibly doing something even when the user has
  // bg_opacity=0 (the default — fully transparent background). Otherwise the
  // toggle would be a no-op for most users.
  const tintActive =
    settings.tint_bg_from_album_art &&
    !!tintColor &&
    !!track &&
    !!albumArt &&
    albumArt.title === track.title &&
    albumArt.artist === track.artist;
  const effectiveBgColor = tintActive
    ? mixHexWithRgb(settings.bg_color, tintColor!, 0.5)
    : settings.bg_color;
  const effectiveOpacity =
    tintActive && settings.bg_opacity < 22 ? 22 : settings.bg_opacity;
  const bgRgba = colorWithOpacity(effectiveBgColor, effectiveOpacity);

  // Outer frame for all layouts: full window, visual chrome, vertical centering
  // of the inner content. The inner row (3-line / single-line) OR the inner
  // scrolling column (full-page) controls horizontal layout.
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
    background: bgRgba,
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
  const innerRowStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    gap: showArt && albumArt ? 14 : 0,
    width: "100%",
    minHeight: 0,
  };

  // Vertical stack that holds the update banner (when visible) above the
  // horizontal art+lyrics row. The ResizeObserver-tracked element (which
  // drives the window's auto-resize-to-content) sits here, NOT on
  // `innerRowStyle`, so banner height contributes to the window height.
  // When `UpdateBanner` returns null (idle phase), this collapses to just
  // the row's height — no visual change.
  const outerStackStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    alignItems: "stretch",
    gap: 4,
    width: "100%",
    minHeight: 0,
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
        <div style={innerRowStyle}>
          {showArt && albumArt ? <AlbumArtSide dataUrl={albumArt.data_url} size={artSize} /> : null}
          <div ref={setLyricsColEl} style={lyricsColStyle}>
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
          </div>
        </div>
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
        {showArt && albumArt ? <AlbumArtBadge dataUrl={albumArt.data_url} /> : null}
        {hasLines ? (
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
      <NudgeBanner banner={nudgeBanner} />
      <div ref={setInnerRowEl} style={outerStackStyle}>
        <UpdateBanner state={updateState} onInstall={installUpdate} />
        <div style={innerRowStyle}>
          {showArt && albumArt ? <AlbumArtSide dataUrl={albumArt.data_url} size={artSize} /> : null}
          <div ref={setLyricsColEl} style={lyricsColStyle}>
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
          </div>
        </div>
      </div>
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

function AlbumArtBadge({ dataUrl }: { dataUrl: string }) {
  // Used by full-page layout — small absolute-positioned thumbnail in the
  // corner since side-by-side would conflict with the scrolling column.
  return (
    <img
      src={dataUrl}
      alt=""
      draggable={false}
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
        pointerEvents: "none",
      }}
    />
  );
}

// Side-by-side album art used by 3-line and single-line layouts. The size
// prop comes from a ResizeObserver on the lyrics column in the parent — this
// is exact, not approximate, because pure CSS (align-self:stretch +
// aspect-ratio:1) was off by a handful of px when the image's intrinsic
// dimensions interacted with flex's hypothetical-size pass.
function AlbumArtSide({ dataUrl, size }: { dataUrl: string; size: number }) {
  // Floor at 40 so a tiny font doesn't shrink the art to a sliver.
  const px = Math.max(40, size);
  return (
    <div
      style={{
        width: px,
        height: px,
        flexShrink: 0,
        position: "relative",
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
        textShadow: textShadow ?? "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
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
        textShadow: textShadow ?? "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
        opacity: text ? 1 : 0.2,
        // Disable color-transition on the container while karaoke is active
        // so it doesn't fight the per-word transitions.
        transition: useKaraoke
          ? "opacity 220ms ease"
          : "opacity 220ms ease, color 220ms ease",
        lineHeight: 1.2,
        maxWidth: "92vw",
        letterSpacing: isCur ? 0.2 : 0,
        ...wrapStyle,
      }}
    >
      {useKaraoke
        ? karaoke!.words.map((w, i) => {
            const idx = karaoke!.currentWordIdx;
            const isPast = idx > i;
            const isCurrent = idx === i;
            const lit = isPast || isCurrent;
            const dur = wordDurationMs(karaoke!.words, i, karaoke!.nextTimeMs);
            return (
              <span
                key={i}
                style={{
                  color: lit ? settings.text_color : settings.text_color_dim,
                  transition: isCurrent ? `color ${dur}ms linear` : "none",
                }}
              >
                {w.text}
              </span>
            );
          })
        : text || "♪"}
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

function statusLine(l: CurrentLyrics, t: CurrentTrack | null): string {
  switch (l.status) {
    case "fetching":
      return t?.title ? `♪ fetching — ${t.title}` : "♪ fetching…";
    case "not_found":
      return t?.title
        ? `♪ no lyrics for ${t.title}`
        : "♪ no lyrics on LRCLib";
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
