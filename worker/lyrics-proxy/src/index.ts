/**
 * Hum lyrics proxy.
 *
 * Why this exists: `lrclib.net` is blocked at the TLS layer on some networks.
 * The block is SNI-based, meaning the filter reads the hostname out of the
 * ClientHello and kills the connection before any HTTP happens. Verified
 * against a single Cloudflare IP: a handshake carrying SNI `www.cloudflare.com`
 * or `example.com` completes at TLS 1.3, while the same IP and port with SNI
 * `lrclib.net` is dropped. That reproduces through both schannel and OpenSSL,
 * so no client-side change can route around it.
 *
 * Fronting LRCLib with a hostname of ours means the ClientHello carries
 * `lyrics.syvr.dev`, which the filter has no rule for. Hum still tries LRCLib
 * directly first and only falls back here, so this carries traffic solely for
 * users whose networks are filtered.
 *
 * Deliberately unauthenticated. Hum ships to end users, so any token in the
 * binary is public the moment someone opens it, and pretending otherwise buys
 * nothing. Containment comes from the shape instead: this Worker can only ever
 * reach two fixed read-only paths on one fixed upstream, so the worst it can
 * be abused for is looking up song lyrics.
 */

const UPSTREAM = "https://lrclib.net";

/** The only paths that get proxied. Both are read-only lookups. */
const ALLOWED_PATHS = new Set(["/api/get", "/api/search"]);

/**
 * Query params we forward, per LRCLib's API. Anything else is dropped rather
 * than passed through, so this cannot be steered into an unintended request
 * shape by a caller appending parameters.
 */
const ALLOWED_PARAMS = new Set([
  "artist_name",
  "track_name",
  "album_name",
  "duration",
  "q",
]);

/**
 * Edge cache lifetimes.
 *
 * Hits are stable, so they are held for a day. Misses expire much sooner
 * because LRCLib is community-contributed and a track with no lyrics today may
 * have them tomorrow; caching a miss for a day would hide that from every user
 * behind this Worker at once.
 */
const CACHE_TTL_HIT_SECS = 86_400;
const CACHE_TTL_MISS_SECS = 900;

/** Upstream is a small volunteer-run service, so fail fast rather than hang. */
const UPSTREAM_TIMEOUT_MS = 10_000;

function cors(res: Response): Response {
  const out = new Response(res.body, res);
  out.headers.set("access-control-allow-origin", "*");
  out.headers.set("access-control-allow-methods", "GET, OPTIONS");
  return out;
}

export default {
  async fetch(req: Request, _env: unknown, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(req.url);

    if (req.method === "OPTIONS") {
      return cors(new Response(null, { status: 204 }));
    }

    if (url.pathname === "/healthz") {
      return cors(Response.json({ ok: true, upstream: UPSTREAM }));
    }

    if (req.method !== "GET") {
      return cors(new Response("method not allowed", { status: 405 }));
    }

    if (!ALLOWED_PATHS.has(url.pathname)) {
      return cors(new Response("not found", { status: 404 }));
    }

    // Rebuild the query from the allowlist, sorted, so that two requests for
    // the same lookup produce one cache key regardless of param order.
    const forwarded = new URLSearchParams();
    for (const key of [...url.searchParams.keys()].sort()) {
      if (!ALLOWED_PARAMS.has(key)) continue;
      const value = url.searchParams.get(key);
      if (value !== null) forwarded.set(key, value);
    }

    const upstreamUrl = `${UPSTREAM}${url.pathname}?${forwarded.toString()}`;
    const cacheKey = new Request(upstreamUrl, { method: "GET" });
    const cache = caches.default;

    const cached = await cache.match(cacheKey);
    if (cached) {
      const hit = new Response(cached.body, cached);
      hit.headers.set("x-hum-proxy-cache", "hit");
      return cors(hit);
    }

    let upstreamRes: Response;
    try {
      upstreamRes = await fetch(upstreamUrl, {
        method: "GET",
        headers: {
          // LRCLib asks clients to identify themselves. Keep Hum named here so
          // the traffic is attributable and they can reach us if it misbehaves.
          "user-agent":
            "Hum (https://github.com/basezero-projects/Hum) via lyrics.syvr.dev",
          accept: "application/json",
        },
        signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
      });
    } catch (err) {
      // 502 rather than a synthesized empty result. Hum treats a transport
      // failure and an authoritative "no lyrics" very differently, and
      // inventing a miss here would recreate the exact bug this work fixed.
      return cors(
        Response.json(
          { error: "upstream unreachable", detail: String(err) },
          { status: 502 },
        ),
      );
    }

    // Pass 5xx straight through, uncached, for the same reason.
    if (upstreamRes.status >= 500) {
      return cors(new Response("upstream error", { status: 502 }));
    }

    const ttl = upstreamRes.ok ? CACHE_TTL_HIT_SECS : CACHE_TTL_MISS_SECS;
    const body = await upstreamRes.arrayBuffer();

    const res = new Response(body, {
      status: upstreamRes.status,
      headers: {
        "content-type":
          upstreamRes.headers.get("content-type") ?? "application/json",
        "cache-control": `public, max-age=${ttl}`,
      },
    });

    // Store a clone before the response is consumed by the caller.
    ctx.waitUntil(cache.put(cacheKey, res.clone()));

    const out = new Response(res.body, res);
    out.headers.set("x-hum-proxy-cache", "miss");
    return cors(out);
  },
} satisfies ExportedHandler;
