export type CurrentTrack = {
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

export type LyricLine = { time_ms: number; text: string };

export type LyricsStatus =
  | "idle"
  | "fetching"
  | "synced"
  | "plain"
  | "instrumental"
  | "not_found"
  | "error";

export type CurrentLyrics = {
  track_key: string;
  status: LyricsStatus;
  source: string | null;
  line_count: number;
  lines: LyricLine[];
  plain: string | null;
  track: {
    title: string;
    artist: string;
    album: string;
    duration_ms: number;
  };
};

export function fmtMs(ms: number) {
  if (!ms || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
