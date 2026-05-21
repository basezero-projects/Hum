import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { fmtMs } from "./types";
import type { CurrentLyrics, CurrentTrack, LyricsStatus } from "./types";

type LogEntry = {
  ts: number;
  event:
    | "track-changed"
    | "timeline-changed"
    | "playback-state-changed"
    | "lyrics-state"
    | "lyrics-loaded"
    | "lyrics-not-found";
  summary: string;
  color: string;
};

const MAX_LOG = 80;

export default function DevConsole() {
  const [track, setTrack] = useState<CurrentTrack | null>(null);
  const [lyrics, setLyrics] = useState<CurrentLyrics | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const pushLog = (entry: Omit<LogEntry, "ts">) => {
      setLog((prev) =>
        [{ ts: Date.now(), ...entry }, ...prev].slice(0, MAX_LOG),
      );
    };

    const onTrack = (event: LogEntry["event"], payload: CurrentTrack) => {
      setTrack(payload);
      pushLog({
        event,
        color: trackEventColor(event),
        summary: `${payload.title || "(no title)"} — ${
          payload.artist || "(no artist)"
        } [${payload.state}] ${fmtMs(payload.position_ms)}/${fmtMs(
          payload.duration_ms,
        )}`,
      });
      // eslint-disable-next-line no-console
      console.log(`[${event}]`, payload);
    };

    const onLyrics = (event: LogEntry["event"], payload: CurrentLyrics) => {
      setLyrics(payload);
      const head = payload.lines[0]?.text ?? payload.plain?.split("\n")[0] ?? "";
      pushLog({
        event,
        color: lyricsEventColor(event),
        summary: `${payload.status} (${payload.source ?? "—"}) lines=${
          payload.line_count
        } ${head ? `· ${head.slice(0, 50)}` : ""}`,
      });
      // eslint-disable-next-line no-console
      console.log(`[${event}]`, payload);
    };

    const unlisteners: Array<Promise<() => void>> = [
      listen<CurrentTrack>("track-changed", (e) =>
        onTrack("track-changed", e.payload),
      ),
      listen<CurrentTrack>("timeline-changed", (e) =>
        onTrack("timeline-changed", e.payload),
      ),
      listen<CurrentTrack>("playback-state-changed", (e) =>
        onTrack("playback-state-changed", e.payload),
      ),
      listen<CurrentLyrics>("lyrics-state", (e) =>
        onLyrics("lyrics-state", e.payload),
      ),
      listen<CurrentLyrics>("lyrics-loaded", (e) =>
        onLyrics("lyrics-loaded", e.payload),
      ),
      listen<CurrentLyrics>("lyrics-not-found", (e) =>
        onLyrics("lyrics-not-found", e.payload),
      ),
    ];

    invoke<CurrentTrack>("get_current_track")
      .then((t) => setTrack(t))
      .catch((err) => console.warn("get_current_track failed:", err));

    invoke<CurrentLyrics>("get_current_lyrics")
      .then((l) => setLyrics(l))
      .catch((err) => console.warn("get_current_lyrics failed:", err));

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
        Hum — SMTC + iTunes + LRCLib dev console
      </h1>
      <p style={{ fontSize: 12, color: "#888", margin: "4px 0 16px" }}>
        Play music in any Windows app. Lyrics fetch automatically on track change.
      </p>

      <section
        style={{
          background: "#15151c",
          border: "1px solid #2a2a36",
          borderRadius: 8,
          padding: 16,
          marginBottom: 12,
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

      <section
        style={{
          background: "#15151c",
          border: "1px solid #2a2a36",
          borderRadius: 8,
          padding: 16,
          marginBottom: 16,
        }}
      >
        <div
          style={{
            fontSize: 11,
            color: "#888",
            marginBottom: 8,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>LYRICS</span>
          {lyrics && (
            <span style={{ color: lyricsStatusColor(lyrics.status) }}>
              {lyrics.status}
              {lyrics.source ? `  ·  ${lyrics.source}` : ""}
              {lyrics.line_count ? `  ·  ${lyrics.line_count} lines` : ""}
            </span>
          )}
        </div>
        {lyrics === null ? (
          <div style={{ color: "#666" }}>No lyric request yet.</div>
        ) : lyrics.status === "fetching" ? (
          <div style={{ color: "#aaa" }}>
            Fetching for{" "}
            <span style={{ color: "#fff" }}>
              {lyrics.track.title || "(no title)"}
            </span>
            …
          </div>
        ) : lyrics.status === "not_found" ? (
          <div>
            <div style={{ color: "#888", marginBottom: lyrics.errors?.length ? 8 : 0 }}>
              No lyrics found for{" "}
              <span style={{ color: "#fff" }}>
                {lyrics.track.title || "(no title)"} —{" "}
                {lyrics.track.artist || "(no artist)"}
              </span>
            </div>
            {lyrics.errors && lyrics.errors.length > 0 && (
              <div
                style={{
                  fontSize: 11,
                  fontFamily:
                    "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                  color: "#fbbf24",
                  background: "#1a1610",
                  border: "1px solid #3a2f1a",
                  borderRadius: 4,
                  padding: "8px 10px",
                  whiteSpace: "pre-wrap",
                  lineHeight: 1.5,
                }}
                title="Authoritative miss from at least one source; these are peer-source errors that didn't change the outcome."
              >
                {lyrics.errors.join("\n")}
              </div>
            )}
          </div>
        ) : lyrics.status === "unsupported" ? (
          <div>
            <div style={{ color: "#888", marginBottom: 0 }}>
              Track info unavailable from this source —{" "}
              <span style={{ color: "#fff" }}>
                {lyrics.track.title || "(no title)"}
              </span>{" "}
              cannot be decoded by Hum.
            </div>
          </div>
        ) : lyrics.status === "error" ? (
          <div>
            <div style={{ color: "#f87171", marginBottom: 8 }}>
              Error fetching lyrics for{" "}
              <span style={{ color: "#fff" }}>
                {lyrics.track.title || "(no title)"}
                {lyrics.track.artist ? ` — ${lyrics.track.artist}` : ""}
              </span>
              . Will retry on next track.
            </div>
            {lyrics.errors && lyrics.errors.length > 0 && (
              <div
                style={{
                  fontSize: 11,
                  fontFamily:
                    "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                  color: "#fca5a5",
                  background: "#1a0d0d",
                  border: "1px solid #3a1a1a",
                  borderRadius: 4,
                  padding: "8px 10px",
                  whiteSpace: "pre-wrap",
                  lineHeight: 1.5,
                }}
              >
                {lyrics.errors.join("\n")}
              </div>
            )}
          </div>
        ) : lyrics.status === "instrumental" ? (
          <div style={{ color: "#aaa" }}>♪ instrumental</div>
        ) : lyrics.status === "synced" ? (
          <div
            style={{
              maxHeight: 200,
              overflowY: "auto",
              fontSize: 12,
              lineHeight: 1.5,
            }}
          >
            {lyrics.lines.slice(0, 10).map((line, i) => (
              <div key={i} style={{ display: "flex", gap: 12 }}>
                <span style={{ color: "#666", minWidth: 60 }}>
                  {fmtMs(line.time_ms)}
                </span>
                <span style={{ color: "#ddd" }}>{line.text || "♪"}</span>
              </div>
            ))}
            {lyrics.lines.length > 10 && (
              <div style={{ color: "#555", marginTop: 6 }}>
                …{lyrics.lines.length - 10} more lines
              </div>
            )}
          </div>
        ) : lyrics.status === "plain" ? (
          <div
            style={{
              maxHeight: 200,
              overflowY: "auto",
              fontSize: 12,
              color: "#ccc",
              whiteSpace: "pre-wrap",
            }}
          >
            {(lyrics.plain ?? "").split("\n").slice(0, 10).join("\n")}
            {(lyrics.plain ?? "").split("\n").length > 10 && (
              <div style={{ color: "#555", marginTop: 6 }}>
                …{(lyrics.plain ?? "").split("\n").length - 10} more lines
              </div>
            )}
          </div>
        ) : (
          <div style={{ color: "#666" }}>idle</div>
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
            maxHeight: "40vh",
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
                <span style={{ color: entry.color }}>{entry.event}</span>{" "}
                <span style={{ color: "#aaa" }}>{entry.summary}</span>
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

function trackEventColor(e: LogEntry["event"]) {
  switch (e) {
    case "track-changed":
      return "#60a5fa";
    case "timeline-changed":
      return "#a78bfa";
    case "playback-state-changed":
      return "#f472b6";
    default:
      return "#888";
  }
}

function lyricsEventColor(e: LogEntry["event"]) {
  switch (e) {
    case "lyrics-state":
      return "#fcd34d";
    case "lyrics-loaded":
      return "#34d399";
    case "lyrics-not-found":
      return "#9ca3af";
    default:
      return "#888";
  }
}

function lyricsStatusColor(s: LyricsStatus) {
  switch (s) {
    case "synced":
      return "#34d399";
    case "plain":
      return "#a3e635";
    case "instrumental":
      return "#aaa";
    case "fetching":
      return "#fcd34d";
    case "not_found":
      return "#9ca3af";
    case "error":
      return "#f87171";
    default:
      return "#888";
  }
}
