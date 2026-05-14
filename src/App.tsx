import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type CurrentTrack = {
  title: string;
  artist: string;
  album: string;
  duration_ms: number;
  position_ms: number;
  last_update_unix_ms: number;
  state:
    | "unknown"
    | "closed"
    | "opened"
    | "changing"
    | "stopped"
    | "playing"
    | "paused";
  source_app_id: string | null;
};

type LogEntry = {
  ts: number;
  event: "track-changed" | "timeline-changed" | "playback-state-changed";
  payload: CurrentTrack;
};

const MAX_LOG = 80;

function fmtMs(ms: number) {
  if (!ms || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function App() {
  const [track, setTrack] = useState<CurrentTrack | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const append = (
      event: LogEntry["event"],
      payload: CurrentTrack,
    ) => {
      setTrack(payload);
      setLog((prev) =>
        [{ ts: Date.now(), event, payload }, ...prev].slice(0, MAX_LOG),
      );
      // eslint-disable-next-line no-console
      console.log(`[${event}]`, payload);
    };

    const unlisteners: Array<Promise<() => void>> = [
      listen<CurrentTrack>("track-changed", (e) =>
        append("track-changed", e.payload),
      ),
      listen<CurrentTrack>("timeline-changed", (e) =>
        append("timeline-changed", e.payload),
      ),
      listen<CurrentTrack>("playback-state-changed", (e) =>
        append("playback-state-changed", e.payload),
      ),
    ];

    invoke<CurrentTrack>("get_current_track")
      .then((t) => setTrack(t))
      .catch((err) => console.warn("get_current_track failed:", err));

    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  return (
    <main
      style={{
        minHeight: "100vh",
        padding: "24px",
        fontFamily:
          "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
        background: "#0c0c10",
        color: "#e8e8ee",
      }}
    >
      <h1
        style={{
          fontSize: 18,
          fontWeight: 600,
          margin: 0,
          color: "#d4af37",
        }}
      >
        Lyric Overlay — Phase 1: SMTC dev console
      </h1>
      <p style={{ fontSize: 12, color: "#888", margin: "4px 0 16px" }}>
        Play music in any Windows app. Events should stream below.
      </p>

      <section
        style={{
          background: "#15151c",
          border: "1px solid #2a2a36",
          borderRadius: 8,
          padding: 16,
          marginBottom: 16,
        }}
      >
        <div style={{ fontSize: 11, color: "#888", marginBottom: 8 }}>
          CURRENT TRACK
        </div>
        {track ? (
          <>
            <div style={{ fontSize: 16, fontWeight: 600 }}>
              {track.title || "(no title)"}
            </div>
            <div style={{ fontSize: 13, color: "#aaa" }}>
              {track.artist || "(no artist)"}
              {track.album ? ` — ${track.album}` : ""}
            </div>
            <div style={{ fontSize: 12, color: "#888", marginTop: 8 }}>
              <span style={{ color: stateColor(track.state) }}>
                ● {track.state}
              </span>
              {"  "}
              {fmtMs(track.position_ms)} / {fmtMs(track.duration_ms)}
              {track.source_app_id ? `  ·  ${track.source_app_id}` : ""}
            </div>
          </>
        ) : (
          <div style={{ color: "#666" }}>Waiting for an active session…</div>
        )}
      </section>

      <section>
        <div style={{ fontSize: 11, color: "#888", marginBottom: 8 }}>
          EVENT LOG ({log.length})
        </div>
        <div
          ref={logRef}
          style={{
            background: "#0f0f15",
            border: "1px solid #2a2a36",
            borderRadius: 8,
            padding: 12,
            maxHeight: "60vh",
            overflowY: "auto",
            fontSize: 11,
            lineHeight: 1.5,
          }}
        >
          {log.length === 0 ? (
            <div style={{ color: "#555" }}>No events yet.</div>
          ) : (
            log.map((entry, i) => (
              <div
                key={`${entry.ts}-${i}`}
                style={{
                  borderBottom: "1px dashed #1f1f2a",
                  padding: "4px 0",
                }}
              >
                <span style={{ color: "#666" }}>
                  {new Date(entry.ts).toLocaleTimeString()}
                </span>{" "}
                <span style={{ color: eventColor(entry.event) }}>
                  {entry.event}
                </span>{" "}
                <span style={{ color: "#aaa" }}>
                  {entry.payload.title || "(no title)"} —{" "}
                  {entry.payload.artist || "(no artist)"}
                </span>{" "}
                <span style={{ color: stateColor(entry.payload.state) }}>
                  [{entry.payload.state}]
                </span>{" "}
                <span style={{ color: "#666" }}>
                  {fmtMs(entry.payload.position_ms)}/
                  {fmtMs(entry.payload.duration_ms)}
                </span>
              </div>
            ))
          )}
        </div>
      </section>
    </main>
  );
}

function stateColor(s: CurrentTrack["state"]) {
  switch (s) {
    case "playing":
      return "#4ade80";
    case "paused":
      return "#fbbf24";
    case "stopped":
    case "closed":
      return "#888";
    default:
      return "#aaa";
  }
}

function eventColor(e: LogEntry["event"]) {
  switch (e) {
    case "track-changed":
      return "#60a5fa";
    case "timeline-changed":
      return "#a78bfa";
    case "playback-state-changed":
      return "#f472b6";
  }
}
