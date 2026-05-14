import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CurrentLyrics, CurrentTrack, LyricLine } from "./types";

export default function Overlay() {
  const [track, setTrack] = useState<CurrentTrack | null>(null);
  const [lyrics, setLyrics] = useState<CurrentLyrics | null>(null);
  // displayIdx is what the DOM renders. It changes only when the active line
  // changes (~once per LRC entry), NOT on every rAF tick.
  const [displayIdx, setDisplayIdx] = useState<number>(-1);

  // Refs hold the hot-loop data so the rAF closure stays stable across
  // re-renders. Events update these AND the React state.
  const trackRef = useRef<CurrentTrack | null>(null);
  const lyricsRef = useRef<CurrentLyrics | null>(null);
  const indexRef = useRef<number>(-1);

  useEffect(() => {
    function interpolatedPositionMs(): number {
      const t = trackRef.current;
      if (!t) return 0;
      if (t.state !== "playing") return t.position_ms;
      const wallElapsed = Date.now() - t.last_update_unix_ms;
      return t.position_ms + Math.max(0, wallElapsed);
    }

    function snapCursorToCurrentPosition(lines: LyricLine[]): number {
      if (lines.length === 0) return -1;
      const pos = interpolatedPositionMs();
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
        const pos = interpolatedPositionMs();
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

    function applyTrack(t: CurrentTrack) {
      const prev = trackRef.current;
      trackRef.current = t;
      setTrack(t);
      // On title/artist change, clear stale cursor while we wait for new lyrics
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

    const unlisteners: Array<Promise<() => void>> = [
      listen<CurrentTrack>("track-changed", (e) => applyTrack(e.payload)),
      listen<CurrentTrack>("timeline-changed", (e) => applyTrack(e.payload)),
      listen<CurrentTrack>("playback-state-changed", (e) => applyTrack(e.payload)),
      listen<CurrentLyrics>("lyrics-state", (e) => applyLyrics(e.payload)),
      listen<CurrentLyrics>("lyrics-loaded", (e) => applyLyrics(e.payload)),
      listen<CurrentLyrics>("lyrics-not-found", (e) => applyLyrics(e.payload)),
    ];

    invoke<CurrentTrack>("get_current_track")
      .then(applyTrack)
      .catch(() => {});
    invoke<CurrentLyrics>("get_current_lyrics")
      .then(applyLyrics)
      .catch(() => {});

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

  return (
    <div
      data-tauri-drag-region
      style={{
        height: "100vh",
        width: "100vw",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        alignItems: "center",
        gap: 6,
        padding: "12px 28px",
        boxSizing: "border-box",
        background: "transparent",
        fontFamily:
          '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
        userSelect: "none",
        cursor: "move",
      }}
    >
      <LineRow text={prev?.text} kind="prev" />
      <LineRow text={middleText} kind="cur" />
      <LineRow text={next?.text} kind="next" />
    </div>
  );
}

function LineRow({
  text,
  kind,
}: {
  text: string | undefined;
  kind: "prev" | "cur" | "next";
}) {
  const isCur = kind === "cur";
  return (
    <div
      data-tauri-drag-region
      style={{
        fontSize: isCur ? 28 : 16,
        fontWeight: isCur ? 600 : 400,
        color: isCur ? "#ffffff" : "rgba(255,255,255,0.45)",
        textAlign: "center",
        textShadow:
          "0 2px 6px rgba(0,0,0,0.95), 0 0 14px rgba(0,0,0,0.65)",
        opacity: text ? 1 : 0.2,
        transition: "opacity 220ms ease, color 220ms ease",
        lineHeight: 1.25,
        maxWidth: "92vw",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
        letterSpacing: isCur ? 0.2 : 0,
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
