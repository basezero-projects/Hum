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
  // Track when the window is actually visible so we don't burn API
  // tokens fetching commentary while the user has the window minimized
  // / hidden via the tray. Fetches resume when visibility returns.
  const [isVisible, setIsVisible] = useState<boolean>(
    typeof document !== "undefined" ? document.visibilityState === "visible" : true,
  );
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
    const onVis = () => setIsVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVis);
    return () => {
      unTrack.then((fn) => fn()).catch(() => {});
      unSettings.then((fn) => fn()).catch(() => {});
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  useEffect(() => {
    if (!isVisible) return; // Window hidden → don't burn API tokens.
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
  }, [track?.title, track?.artist, track?.album, isVisible]);

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
          <CommentaryBody text={resp.text} source={resp.source} />
        ) : (
          <Placeholder>
            {track?.title ? "No commentary available." : "Play a track to see commentary."}
          </Placeholder>
        )}
      </div>
    </div>
  );
}

function CommentaryBody({ text, source }: { text: string; source: string }) {
  // Split on sentence boundaries followed by a space + capital, but keep
  // the punctuation. Resulting "paragraphs" are each 1-2 sentences for
  // breathing room — way more readable than one wall-of-text block.
  const sentences = splitSentences(text);
  const groups: string[] = [];
  for (let i = 0; i < sentences.length; i += 2) {
    groups.push(sentences.slice(i, i + 2).join(" "));
  }
  return (
    <div style={{ padding: "20px 22px" }}>
      {groups.map((para, i) => (
        <p
          key={i}
          style={{
            margin: i === 0 ? "0 0 12px" : "0 0 12px",
            // Lead paragraph is slightly larger; subsequent paragraphs settle.
            fontSize: i === 0 ? 15 : 14,
            lineHeight: 1.55,
            color: i === 0 ? "#eaeaea" : "rgba(234,234,234,0.78)",
            maxWidth: 56 * 8, // ~56ch readable measure
          }}
        >
          {para}
        </p>
      ))}
      <div
        style={{
          marginTop: 14,
          fontSize: 10,
          opacity: 0.35,
          letterSpacing: 0.6,
          textTransform: "uppercase",
        }}
      >
        {source === "cache" ? "cached" : source === "api" ? "fresh from claude" : source}
      </div>
    </div>
  );
}

// Split a paragraph into individual sentences. Handles ., !, ? followed
// by space + capital letter / quote / paren. Keeps the punctuation with
// the sentence it belongs to. Conservative — false negatives (under-split)
// are way better than false positives (mid-sentence breaks).
function splitSentences(text: string): string[] {
  const parts: string[] = [];
  let buf = "";
  for (let i = 0; i < text.length; i++) {
    buf += text[i];
    const ch = text[i];
    if (ch === "." || ch === "!" || ch === "?") {
      const next1 = text[i + 1] ?? "";
      const next2 = text[i + 2] ?? "";
      const looksLikeBoundary =
        next1 === " " &&
        (/[A-Z"'(]/.test(next2) || next2 === "");
      if (looksLikeBoundary) {
        parts.push(buf.trim());
        buf = "";
      }
    }
  }
  if (buf.trim()) parts.push(buf.trim());
  return parts.length > 0 ? parts : [text];
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
