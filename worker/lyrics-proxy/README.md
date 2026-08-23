# Hum lyrics proxy

A one-file Cloudflare Worker that fronts LRCLib at `lyrics.syvr.dev`.

## Why it exists

Some networks block `lrclib.net` at the TLS layer. The block reads the hostname
out of the ClientHello and drops the connection before any HTTP happens, so
nothing on the client side can route around it.

You can confirm the shape of it against a single Cloudflare IP. Handshake to
104.21.13.116:443 with SNI `www.cloudflare.com` and it completes at TLS 1.3.
Handshake to the same IP and port with SNI `lrclib.net` and it dies. Same
result through Windows schannel and through OpenSSL, and DNS resolves to the
real Cloudflare addresses the whole time, so it is not a client, certificate,
or resolver problem.

Asking for the same data through a hostname we own puts `lyrics.syvr.dev` in
the ClientHello instead, which those filters have no rule for.

## How Hum uses it

Hum tries `lrclib.net` directly first, every time. Only when that fails at the
transport layer does it fall back here, and it then skips the direct attempt
for ten minutes so a filtered network does not cost a doomed connection on
every track. Any successful direct request clears that immediately, so a laptop
that moves to an open network goes back to talking to LRCLib itself.

That ordering is the point. Users on normal networks never send a track title
through our infrastructure, and LRCLib keeps serving its own traffic.

## What it will and will not do

It proxies exactly two read-only paths, `/api/get` and `/api/search`, to one
fixed upstream. Everything else is a 404, anything other than GET is a 405, and
query parameters outside LRCLib's own are dropped rather than forwarded.

There is no auth, and adding one would be theater. Hum ships to end users, so a
token in the binary is public as soon as somebody opens the file. The limits
above are what actually contain it: the worst case is that a stranger looks up
song lyrics.

Responses are cached at the edge, a day for hits and fifteen minutes for
misses. Misses expire quickly because LRCLib is community-contributed, and a
track with nothing today may have lyrics tomorrow. Transport failures and 5xx
are never cached and come back as a 502, because Hum treats "could not reach
the service" and "this track has no lyrics" as very different answers and
inventing a miss here would undo that.

## Working on it

```bash
pnpm install
pnpm typecheck
pnpm dev
```

Deploy and watch logs:

```bash
pnpm deploy
```

```bash
pnpm tail
```

Quick check that it is alive:

```bash
curl -s https://lyrics.syvr.dev/healthz
```

The `x-hum-proxy-cache` response header reports `hit` or `miss` if you need to
tell whether a response came off the edge cache.
