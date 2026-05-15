import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CurrentTrack, Settings } from "./types";

type CommentaryResp = {
  track_key: string;
  text: string;
  source: "cache" | "api" | "empty" | "error";
  error: string | null;
};

const ACCENT = "#d4af37";

export default function CommentaryView() {
  const [track, setTrack] = useState<CurrentTrack | null>(null);
  const [resp, setResp] = useState<CommentaryResp | null>(null);
  const [loading, setLoading] = useState(false);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  // Avoid double-firing the same track's API call when track-changed
  // events arrive in pairs (iTunes pause-then-resume produces two of these).
  const lastFetchedKey = useRef<string>("");

  useEffect(() => {
    invoke<CurrentTrack>("get_current_track").then(setTrack).catch(() => {});
    invoke<Settings>("get_settings").then((s) => setHasKey(s.claude_api_key.trim().length > 0)).catch(() => {});

    const unTrack = listen<CurrentTrack>("track-changed", (e) => {
      setTrack(e.payload);
    });
    const unSettings = listen<Settings>("settings-changed", (e) => {
      setHasKey(e.payload.claude_api_key.trim().length > 0);
    });
    return () => {
      unTrack.then((fn) => fn()).catch(() => {});
      unSettings.then((fn) => fn()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!track || !track.title || !track.artist) return;
    const key = `${track.artist}|${track.title}|${track.album}`;
    if (key === lastFetchedKey.current) return;
    lastFetchedKey.current = key;
    setLoading(true);
    invoke<CommentaryResp>("get_track_commentary", {
      title: track.title,
      artist: track.artist,
      album: track.album ?? "",
    })
      .then((r) => {
        setResp(r);
      })
      .catch((e) => {
        setResp({
          track_key: key,
          text: "",
          source: "error",
          error: String(e),
        });
      })
      .finally(() => setLoading(false));
  }, [track?.title, track?.artist, track?.album]);

  return (
    <div style={pageStyle}>
      <header style={{ marginBottom: 16 }}>
        <h1 style={{ margin: 0, fontSize: 18, fontWeight: 600, letterSpacing: 0.2 }}>
          AI Commentary
        </h1>
        {track?.title ? (
          <p style={{ margin: "6px 0 0", fontSize: 12, opacity: 0.55 }}>
            <span style={{ color: ACCENT }}>{track.title}</span>
            {track.artist ? ` — ${track.artist}` : ""}
            {track.album ? ` · ${track.album}` : ""}
          </p>
        ) : (
          <p style={{ margin: "6px 0 0", fontSize: 12, opacity: 0.45 }}>
            Waiting for a track…
          </p>
        )}
      </header>

      <div style={cardStyle}>
        {hasKey === false ? (
          <Placeholder>
            <strong style={{ color: ACCENT }}>No Claude API key set.</strong>{" "}
            Open <em>Settings → Commentary</em> and paste your Anthropic API key
            to enable AI-generated context for each track.
          </Placeholder>
        ) : loading ? (
          <Placeholder>Generating context…</Placeholder>
        ) : resp?.error ? (
          <Placeholder>
            <strong style={{ color: "#e57373" }}>Error:</strong> {resp.error}
          </Placeholder>
        ) : resp?.text ? (
          <div style={{ fontSize: 14, lineHeight: 1.55, padding: "16px 18px" }}>
            {resp.text}
            <div
              style={{
                marginTop: 12,
                fontSize: 11,
                opacity: 0.4,
                letterSpacing: 0.5,
                textTransform: "uppercase",
              }}
            >
              {resp.source === "cache" ? "cached" : resp.source === "api" ? "fresh from claude" : resp.source}
            </div>
          </div>
        ) : (
          <Placeholder>
            {track?.title ? "No commentary available." : "Play a track to see commentary."}
          </Placeholder>
        )}
      </div>
    </div>
  );
}

function Placeholder({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        padding: "24px 18px",
        fontSize: 13,
        opacity: 0.65,
        lineHeight: 1.5,
      }}
    >
      {children}
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  height: "100vh",
  width: "100vw",
  overflowY: "auto",
  padding: "22px 24px",
  boxSizing: "border-box",
  background: "#0e0e10",
  color: "#eaeaea",
  fontFamily: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
};

const cardStyle: React.CSSProperties = {
  background: "rgba(255,255,255,0.03)",
  border: "1px solid rgba(255,255,255,0.06)",
  borderRadius: 8,
  overflow: "hidden",
  minHeight: 120,
};
