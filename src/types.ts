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
  ad_active: boolean;
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
  ad_break_promos_enabled: boolean;
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
  wikipedia_url: string;
};

export type ArtistInfo = {
  name: string;
  slug: string;
  bio: ArtistBio | null;
  photo_data_url: string | null;
  tour_dates: TourDate[];
  fetched_at_unix_ms: number;
};

export type Promo = {
  id: string;
  product_name: string;
  tagline: string;
  url: string;
  icon_url: string | null;
  weight: number;
  active: boolean;
  cta_text: string | null;
  accent_color: string | null;
  /** Hero image URL. When set, PromoCard renders the image edge-to-edge
   * (object-fit: contain) in place of the text-driven product_name +
   * tagline + CTA layout. Recommended source dimensions: 1920×240
   * (8:1 aspect) for crispness at any overlay width on HiDPI displays. */
  image_url: string | null;
  /** Alt text for the hero image. Defaults to a generic
   * "Sponsored content from <product_name>" when null. */
  alt: string | null;
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
  | "error"
  | "ad";

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
  /** Populated when status === "ad". The rotation-picked promo to display. */
  promo?: Promo | null;
};

export function fmtMs(ms: number) {
  if (!ms || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
