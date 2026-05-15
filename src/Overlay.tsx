import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  auto_contrast: false,
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
  // Background luminance from contrast.rs's screen-capture worker. Null
  // until the first sample arrives. Frontend uses hysteresis around the
  // threshold to avoid color-flicker on dynamic backgrounds (videos).
  const [bgIsLight, setBgIsLight] = useState<boolean | null>(null);

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
      return interpolatedPositionMs() + settingsRef.current.anticipate_ms;
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
  const settingsForRender: Settings = autoColorActive
    ? { ...settings, text_color: effectiveTextColor, text_color_dim: effectiveTextColorDim }
    : settings;
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

  const lyricsColStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    flex: 1,
    minWidth: 0, // allows ellipsis on overflowing lines inside the flex child
    alignItems: alignToFlex(settings.text_align),
    gap: pxToVh(settings.line_padding_px),
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
            />
            {translationText ? (
              <TranslationRow text={translationText} settings={settingsForRender} />
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
            />
          ))
        ) : (
          <LineRow
            text={middleText}
            kind="cur"
            dragRegion={isEdit}
            settings={settingsForRender}
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
      <div style={innerRowStyle}>
        {showArt && albumArt ? <AlbumArtSide dataUrl={albumArt.data_url} size={artSize} /> : null}
        <div ref={setLyricsColEl} style={lyricsColStyle}>
          <LineRow text={prev?.text} kind="prev" dragRegion={isEdit} settings={settingsForRender} />
          <LineRow
            text={middleText}
            kind="cur"
            dragRegion={isEdit}
            settings={settingsForRender}
            karaoke={curKaraoke}
          />
          {translationText ? (
            <TranslationRow text={translationText} settings={settingsForRender} />
          ) : (
            <LineRow text={next?.text} kind="next" dragRegion={isEdit} settings={settingsForRender} />
          )}
        </div>
      </div>
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

function TranslationRow({ text, settings }: { text: string; settings: Settings }) {
  return (
    <div
      style={{
        fontSize: pxToVh(Math.max(11, settings.font_size_px * 0.55)),
        fontWeight: 400,
        color: settings.text_color_dim,
        textAlign: settings.text_align,
        textShadow: "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
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
}: {
  text: string | undefined;
  kind: "prev" | "cur" | "next";
  dragRegion: boolean;
  settings: Settings;
  scrollIntoView?: boolean;
  karaoke?: { words: WordSpan[]; currentWordIdx: number; nextTimeMs: number };
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
  // Font sizes use vh (viewport-height-relative) so the text scales when
  // the user drags the overlay window smaller / bigger in edit mode. The
  // slider value is the literal pixel size at the baseline 200px window
  // height; smaller window scales text down proportionally.
  const sizePx = isCur ? settings.font_size_px : Math.max(12, settings.font_size_px * 0.6);
  return (
    <div
      ref={ref}
      {...drag}
      style={{
        fontSize: pxToVh(sizePx),
        fontWeight: isCur ? settings.font_weight : 400,
        // When karaoke is on, the per-word spans own their color. The container
        // color still matters for any leftover non-span text (none in practice).
        color: isCur ? settings.text_color : settings.text_color_dim,
        textAlign: settings.text_align,
        textShadow:
          "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
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
// the lyric text to scale WITH the window so dragging it smaller shrinks
// the whole composition (text, gaps, art via the ResizeObserver chain)
// instead of just cropping. The slider value in Settings is calibrated to
// this baseline height — at 200px window height, font_size_px renders as
// the literal pixel value the user picked. Window 100px tall → text is
// half. Window 400px tall → text is double. Linear in viewport height.
const BASELINE_WINDOW_HEIGHT_PX = 200;
function pxToVh(px: number): string {
  return `${((px / BASELINE_WINDOW_HEIGHT_PX) * 100).toFixed(3)}vh`;
}

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
