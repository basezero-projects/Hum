import { useEffect, useRef, useState } from "react";
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
  text_align: "center",
  line_padding_px: 6,
  layout_mode: "three_line",
  show_album_art: true,
  show_translation: false,
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

  // Refs hold the hot-loop data so the rAF closure stays stable across
  // re-renders. Events update these AND the React state.
  const trackRef = useRef<CurrentTrack | null>(null);
  const lyricsRef = useRef<CurrentLyrics | null>(null);
  const indexRef = useRef<number>(-1);
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
          setDisplayIdx(idx);
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
        (e) => setAlbumArt(e.payload),
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
  const bgRgba = colorWithOpacity(settings.bg_color, settings.bg_opacity);

  const containerStyle: React.CSSProperties = {
    position: "relative",
    height: "100vh",
    width: "100vw",
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    alignItems: alignToFlex(settings.text_align),
    gap: settings.line_padding_px,
    padding: "12px 28px",
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

  if (layoutMode === "single_line") {
    return (
      <div
        {...dragProps}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={containerStyle}
      >
        {showArt && albumArt ? <AlbumArtBadge dataUrl={albumArt.data_url} /> : null}
        <LineRow
          text={middleText}
          kind="cur"
          dragRegion={isEdit}
          settings={settings}
        />
        {translationText ? (
          <TranslationRow text={translationText} settings={settings} />
        ) : null}
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
              settings={settings}
              scrollIntoView={i === displayIdx}
            />
          ))
        ) : (
          <LineRow
            text={middleText}
            kind="cur"
            dragRegion={isEdit}
            settings={settings}
          />
        )}
      </div>
    );
  }

  // Default: three-line scroll
  return (
    <div
      {...dragProps}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={containerStyle}
    >
      {showArt && albumArt ? <AlbumArtBadge dataUrl={albumArt.data_url} /> : null}
      <LineRow text={prev?.text} kind="prev" dragRegion={isEdit} settings={settings} />
      <LineRow text={middleText} kind="cur" dragRegion={isEdit} settings={settings} />
      {translationText ? (
        <TranslationRow text={translationText} settings={settings} />
      ) : (
        <LineRow text={next?.text} kind="next" dragRegion={isEdit} settings={settings} />
      )}
    </div>
  );
}

function AlbumArtBadge({ dataUrl }: { dataUrl: string }) {
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

function TranslationRow({ text, settings }: { text: string; settings: Settings }) {
  return (
    <div
      style={{
        fontSize: Math.max(11, settings.font_size_px * 0.55),
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
}: {
  text: string | undefined;
  kind: "prev" | "cur" | "next";
  dragRegion: boolean;
  settings: Settings;
  scrollIntoView?: boolean;
}) {
  const isCur = kind === "cur";
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (scrollIntoView && ref.current) {
      ref.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [scrollIntoView]);
  const wrapStyle: React.CSSProperties = isCur
    ? {
        whiteSpace: "normal",
        display: "-webkit-box",
        WebkitLineClamp: 2,
        WebkitBoxOrient: "vertical",
        overflow: "hidden",
        textOverflow: "ellipsis",
      }
    : {
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
      };
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};
  return (
    <div
      ref={ref}
      {...drag}
      style={{
        fontSize: isCur ? settings.font_size_px : Math.max(12, settings.font_size_px * 0.6),
        fontWeight: isCur ? settings.font_weight : 400,
        color: isCur ? settings.text_color : settings.text_color_dim,
        textAlign: settings.text_align,
        textShadow:
          "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
        opacity: text ? 1 : 0.2,
        transition: "opacity 220ms ease, color 220ms ease",
        lineHeight: 1.2,
        maxWidth: "92vw",
        letterSpacing: isCur ? 0.2 : 0,
        ...wrapStyle,
      }}
    >
      {text || "♪"}
    </div>
  );
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

// Convert hex (#rrggbb) + opacity-percent to rgba(...) string. Falls back to
// transparent for invalid input so a typo can't break rendering.
function colorWithOpacity(hex: string, opacityPct: number): string {
  const a = Math.max(0, Math.min(1, opacityPct / 100));
  if (a === 0) return "transparent";
  const m = /^#([0-9a-fA-F]{6})$/.exec(hex);
  if (!m) return "transparent";
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  return `rgba(${r}, ${g}, ${b}, ${a.toFixed(3)})`;
}
