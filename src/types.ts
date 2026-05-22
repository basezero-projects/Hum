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

export type OverlayMode = "edit" | "locked" | "ghost";

export type LayoutMode = "three_line" | "single_line" | "full_page";
export type TextAlign = "left" | "center" | "right";

export type Settings = {
  last_mode: OverlayMode;
  anticipate_ms: number;
  jitter_tolerance_ms: number;
  font_family: string;
  font_size_px: number;
  font_weight: number;
  text_color: string;
  text_color_dim: string;
  bg_color: string;
  bg_opacity: number;
  text_align: TextAlign;
  line_padding_px: number;
  layout_mode: LayoutMode;
  show_album_art: boolean;
  show_translation: boolean;
  tint_bg_from_album_art: boolean;
  blur_album_art_background: boolean;
  window_backdrop: "acrylic" | "mica" | "tabbed_mica" | "none";
  auto_contrast: boolean;
  streamer_enabled: boolean;
  streamer_port: number;
  show_artist_info_panel: boolean;
};

export type TicketStatus = "available" | "sold_out";

export type TourDate = {
  date_unix_ms: number;
  city: string;
  region: string;
  country: string;
  venue: string;
  ticket_url: string | null;
  status: TicketStatus;
};

export type ArtistBio = {
  text: string;
  lastfm_url: string;
};

export type ArtistInfo = {
  name: string;
  slug: string;
  bio: ArtistBio | null;
  photo_data_url: string | null;
  similar_artists: string[];
  tour_dates: TourDate[];
  mbid: string | null;
  fetched_at_unix_ms: number;
};

export type WordSpan = { time_ms: number; text: string };

export type LyricLine = {
  time_ms: number;
  text: string;
  words?: WordSpan[];
};

export type LyricsStatus =
  | "idle"
  | "fetching"
  | "synced"
  | "plain"
  | "instrumental"
  | "not_found"
  | "unsupported"
  | "error";

export type CurrentLyrics = {
  track_key: string;
  status: LyricsStatus;
  /** "memory" | "store" | "lrclib" | "lrclib-search" | "simpmusic" | "netease" | "all-sources" | "error" */
  source: string | null;
  line_count: number;
  lines: LyricLine[];
  plain: string | null;
  /** Per-line translation (currently only NetEase Chinese tlyric). */
  translation: LyricLine[] | null;
  /** Per-source failure strings populated only when status === "error".
   *  Each entry is prefixed with the source name, e.g.
   *  `"lrclib: /api/search failed: connection reset"`. */
  errors?: string[];
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
