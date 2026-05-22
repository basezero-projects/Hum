import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ArtistInfo, CurrentTrack, TourDate } from "../types";

const GOLD = "#d4af37";
const DIM = "rgba(234,234,234,0.55)";
const BG = "rgba(18, 18, 18, 0.97)";
const BORDER = "rgba(255,255,255,0.07)";

export default function ArtistPanel() {
  const [artistName, setArtistName] = useState<string>("");
  const [info, setInfo] = useState<ArtistInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Brief 2s toast surfaced when `open_ticket_url` rejects a URL (host
  // not on the whitelist, or `opener::open` fails). The spec requires a
  // user-visible signal rather than silently dropping the click.
  const [toast, setToast] = useState<string | null>(null);

  // Auto-clear toast after 2s.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2000);
    return () => clearTimeout(t);
  }, [toast]);

  // On mount: get current track, then fetch artist info.
  useEffect(() => {
    async function init() {
      try {
        const track = await invoke<CurrentTrack>("get_current_track");
        const name = track.artist?.trim() ?? "";
        setArtistName(name);
        if (!name) {
          setLoading(false);
          return;
        }
        setLoading(true);
        const result = await invoke<ArtistInfo>("get_artist_info", { artist: name });
        setInfo(result);
        setLoading(false);
      } catch (e) {
        setError(String(e));
        setLoading(false);
      }
    }
    init();
  }, []);

  // Listen for track-changed: if the artist changes, close this window.
  useEffect(() => {
    const unlisten = listen<CurrentTrack>("track-changed", (event) => {
      const newArtist = event.payload.artist?.trim() ?? "";
      if (artistName && newArtist.toLowerCase() !== artistName.toLowerCase()) {
        invoke("close_artist_panel_cmd").catch(() => {});
      }
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [artistName]);

  // ESC key closes the panel.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        invoke("close_artist_panel_cmd").catch(() => {});
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function retry() {
    setError(null);
    setLoading(true);
    invoke<ArtistInfo>("get_artist_info", { artist: artistName })
      .then((result) => { setInfo(result); setLoading(false); })
      .catch((e) => { setError(String(e)); setLoading(false); });
  }

  function openUrl(url: string) {
    invoke("open_ticket_url", { url }).catch(() => {
      setToast("Couldn't open browser");
    });
  }

  function close() {
    invoke("close_artist_panel_cmd").catch(() => {});
  }

  const photo = info?.photo_data_url ?? null;
  const displayName = info?.name ?? artistName;

  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        minHeight: "100vh",
        background: BG,
        color: "rgba(234,234,234,0.9)",
        fontFamily: "'Inter', system-ui, sans-serif",
        fontSize: 13,
        overflow: "hidden",
      }}
    >
      {/* Header — drag region */}
      <div
        data-tauri-drag-region
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "10px 12px 10px 12px",
          borderBottom: `1px solid ${BORDER}`,
          flexShrink: 0,
          userSelect: "none",
        }}
      >
        {/* Artist photo */}
        {photo ? (
          <img
            src={photo}
            alt=""
            draggable={false}
            style={{
              width: 60,
              height: 60,
              borderRadius: "50%",
              objectFit: "cover",
              flexShrink: 0,
              pointerEvents: "none",
            }}
          />
        ) : (
          <div
            style={{
              width: 60,
              height: 60,
              borderRadius: "50%",
              background: "rgba(255,255,255,0.08)",
              flexShrink: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 24,
              color: DIM,
            }}
          >
            ♪
          </div>
        )}

        {/* Artist name */}
        <div
          style={{
            flex: 1,
            fontSize: 18,
            fontWeight: 600,
            letterSpacing: 0.1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            pointerEvents: "none",
          }}
        >
          {displayName || "Unknown Artist"}
        </div>

        {/* Close button */}
        <button
          onClick={close}
          style={{
            background: "transparent",
            border: "none",
            color: DIM,
            cursor: "pointer",
            fontSize: 16,
            lineHeight: 1,
            padding: "2px 4px",
            borderRadius: 4,
            flexShrink: 0,
          }}
          onMouseEnter={(e) => (e.currentTarget.style.color = GOLD)}
          onMouseLeave={(e) => (e.currentTarget.style.color = DIM)}
          aria-label="Close"
        >
          ✕
        </button>
      </div>

      {/* Scrollable body */}
      <div style={{ flex: 1, overflowY: "auto", padding: "12px 14px" }}>
        {loading && !error && (
          <div>
            <LoadingDots />
            <div style={{ color: DIM, fontSize: 12, marginTop: 8 }}>Loading…</div>
          </div>
        )}

        {error && (
          <div style={{ textAlign: "center", padding: "24px 0" }}>
            <div style={{ color: "rgba(229,115,115,0.9)", marginBottom: 12 }}>
              Couldn't load artist info
            </div>
            <button onClick={retry} style={retryButtonStyle}>
              Retry
            </button>
          </div>
        )}

        {!loading && !error && info && (
          <>
            {/* Bio section */}
            {info.bio && (
              <section style={{ marginBottom: 16 }}>
                <SectionLabel>Bio</SectionLabel>
                <p
                  style={{
                    margin: 0,
                    lineHeight: 1.6,
                    color: "rgba(234,234,234,0.82)",
                    fontSize: 12.5,
                  }}
                >
                  {info.bio.text}
                </p>
                <div style={{ marginTop: 6 }}>
                  <ExternalLink url={info.bio.lastfm_url} onOpen={openUrl}>
                    Read more on Last.fm →
                  </ExternalLink>
                </div>
              </section>
            )}

            {/* Similar artists */}
            {info.similar_artists.length > 0 && (
              <section style={{ marginBottom: 16 }}>
                <SectionLabel>Similar to</SectionLabel>
                <p style={{ margin: 0, color: DIM, fontSize: 12.5, lineHeight: 1.6 }}>
                  {info.similar_artists.join(", ")}
                </p>
              </section>
            )}

            {/* Tour dates */}
            <section style={{ marginBottom: 16 }}>
              <SectionLabel>Upcoming shows</SectionLabel>
              <TourDatesList dates={info.tour_dates} onOpenUrl={openUrl} />
            </section>
          </>
        )}
      </div>

      {/* Footer attribution */}
      <div
        style={{
          flexShrink: 0,
          borderTop: `1px solid ${BORDER}`,
          padding: "6px 14px",
          fontSize: 10,
          color: "rgba(234,234,234,0.3)",
          textAlign: "center",
          display: "flex",
          gap: 6,
          justifyContent: "center",
          flexWrap: "wrap",
        }}
      >
        <span>Powered by</span>
        <FooterLink url="https://bandsintown.com" onOpen={openUrl}>Bandsintown</FooterLink>
        <span>·</span>
        <FooterLink url="https://last.fm" onOpen={openUrl}>Last.fm</FooterLink>
        <span>·</span>
        <FooterLink url="https://www.theaudiodb.com" onOpen={openUrl}>TheAudioDB</FooterLink>
      </div>

      {/* Toast overlay — shown briefly when open_ticket_url fails. */}
      {toast ? (
        <div
          style={{
            position: "absolute",
            bottom: 12,
            left: "50%",
            transform: "translateX(-50%)",
            background: "rgba(0,0,0,0.85)",
            color: "rgba(234,234,234,0.95)",
            fontSize: 11,
            padding: "6px 12px",
            borderRadius: 6,
            boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
            pointerEvents: "none",
            zIndex: 100,
          }}
        >
          {toast}
        </div>
      ) : null}
    </div>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: 0.8,
        textTransform: "uppercase",
        color: GOLD,
        marginBottom: 6,
      }}
    >
      {children}
    </div>
  );
}

function TourDatesList({
  dates,
  onOpenUrl,
}: {
  dates: TourDate[];
  onOpenUrl: (url: string) => void;
}) {
  if (dates.length === 0) {
    return (
      <p style={{ margin: 0, color: DIM, fontStyle: "italic", fontSize: 12 }}>
        No upcoming tour dates.
      </p>
    );
  }

  const visible = dates.slice(0, 10);
  const hasMore = dates.length > 10;

  return (
    <div>
      {visible.map((event, i) => (
        <TourDateRow key={i} event={event} onOpenUrl={onOpenUrl} />
      ))}
      {hasMore && (
        <div style={{ marginTop: 8 }}>
          <ExternalLink
            url={`https://bandsintown.com`}
            onOpen={onOpenUrl}
          >
            View all on Bandsintown →
          </ExternalLink>
        </div>
      )}
    </div>
  );
}

function TourDateRow({
  event,
  onOpenUrl,
}: {
  event: TourDate;
  onOpenUrl: (url: string) => void;
}) {
  const dateStr = formatTourDate(event.date_unix_ms);
  const location =
    event.region
      ? `${event.city}, ${event.region}`
      : `${event.city}${event.country ? `, ${event.country}` : ""}`;

  const isSoldOut = event.status === "sold_out";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 10,
        padding: "6px 0",
        borderBottom: `1px solid ${BORDER}`,
      }}
    >
      {/* Date */}
      <div
        style={{
          width: 44,
          flexShrink: 0,
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          fontWeight: 600,
          color: GOLD,
        }}
      >
        {dateStr}
      </div>

      {/* Location + venue */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        <div
          style={{
            fontSize: 12.5,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {location}
        </div>
        {event.venue && (
          <div
            style={{
              fontSize: 11,
              color: DIM,
              fontStyle: "italic",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {event.venue}
          </div>
        )}
      </div>

      {/* Ticket button */}
      {event.ticket_url && (
        <button
          disabled={isSoldOut}
          onClick={() => !isSoldOut && event.ticket_url && onOpenUrl(event.ticket_url)}
          style={{
            flexShrink: 0,
            fontSize: 11,
            fontWeight: 600,
            padding: "3px 8px",
            borderRadius: 4,
            border: "none",
            cursor: isSoldOut ? "not-allowed" : "pointer",
            background: isSoldOut ? "rgba(255,255,255,0.1)" : GOLD,
            color: isSoldOut ? DIM : "#111",
            opacity: isSoldOut ? 0.6 : 1,
            transition: "background 120ms ease",
          }}
          onMouseEnter={(e) => {
            if (!isSoldOut) (e.currentTarget.style.background = "#b8962d");
          }}
          onMouseLeave={(e) => {
            if (!isSoldOut) (e.currentTarget.style.background = GOLD);
          }}
        >
          {isSoldOut ? "Sold Out" : "Tickets"}
        </button>
      )}
    </div>
  );
}

function formatTourDate(unix_ms: number): string {
  const d = new Date(unix_ms);
  const now = new Date();
  const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
  const month = months[d.getUTCMonth()];
  const day = d.getUTCDate();
  const year = d.getUTCFullYear();
  if (year === now.getUTCFullYear()) {
    return `${month} ${day}`;
  }
  return `${month} ${day}, ${year}`;
}

function ExternalLink({
  url,
  onOpen,
  children,
}: {
  url: string;
  onOpen: (url: string) => void;
  children: React.ReactNode;
}) {
  return (
    <span
      role="link"
      tabIndex={0}
      onClick={() => onOpen(url)}
      onKeyDown={(e) => e.key === "Enter" && onOpen(url)}
      style={{
        color: GOLD,
        cursor: "pointer",
        fontSize: 12,
        textDecoration: "underline",
        textDecorationColor: "rgba(212,175,55,0.4)",
      }}
    >
      {children}
    </span>
  );
}

function FooterLink({
  url,
  onOpen,
  children,
}: {
  url: string;
  onOpen: (url: string) => void;
  children: React.ReactNode;
}) {
  return (
    <span
      role="link"
      tabIndex={0}
      onClick={() => onOpen(url)}
      onKeyDown={(e) => e.key === "Enter" && onOpen(url)}
      style={{ cursor: "pointer", color: "rgba(234,234,234,0.35)", fontSize: 10 }}
      onMouseEnter={(e) => (e.currentTarget.style.color = "rgba(234,234,234,0.65)")}
      onMouseLeave={(e) => (e.currentTarget.style.color = "rgba(234,234,234,0.35)")}
    >
      {children}
    </span>
  );
}

function LoadingDots() {
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
      {[0, 1, 2].map((i) => (
        <div
          key={i}
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: GOLD,
            opacity: 0.6,
            animation: `pulse 1s ease-in-out ${i * 0.2}s infinite`,
          }}
        />
      ))}
      <style>{`@keyframes pulse { 0%,100%{opacity:0.3;transform:scale(0.85)} 50%{opacity:1;transform:scale(1.1)} }`}</style>
    </div>
  );
}

const retryButtonStyle: React.CSSProperties = {
  background: "transparent",
  border: `1px solid ${GOLD}`,
  color: GOLD,
  cursor: "pointer",
  fontSize: 12,
  padding: "4px 14px",
  borderRadius: 4,
};
