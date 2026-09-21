# Http Capability — Contract

## What this is

A single outbound HTTP GET made native-side, returning the response body
as text. Exists because a webview `fetch()` to an arbitrary external
origin is subject to ordinary browser CORS rules — the webview's own
origin (`tauri://localhost` / `http://localhost:1420` in dev) is
cross-origin from whatever external page the app wants, and ordinary
server-rendered pages don't send permissive
`Access-Control-Allow-Origin` headers. Making the request native-side
(outside the webview's origin model entirely) sidesteps this, the same
reasoning Tauri's own `@tauri-apps/plugin-http` exists for. Backs
mneme's Phase 10 "Basic LMS Page Import" — pasting a public course-page
URL and parsing its HTML — but the capability itself knows nothing about
LMSes, HTML, or parsing; it only fetches text.

## `desktop.http.get(url)`

```
get(url: string): Promise<{ status: number; ok: boolean; body: string }>
```

Makes a GET request to `url` and resolves once a response is received —
**resolving does not mean success in the HTTP sense.** `status` is the
response's HTTP status code, `ok` is `status >= 200 && status < 300`
(the same meaning `fetch()`'s `Response.ok` already has, deliberately
reusing a shape web developers already know per rule 2), and `body` is
the response body decoded as text. A 404 or 500 response still
**resolves** with `ok: false` and whatever body text the server sent —
the caller decides what a non-2xx response means for its own use case
(e.g. mneme surfacing "couldn't fetch that page" for a non-2xx result),
the same way it would for a same-origin `fetch()`.

Redirects (3xx responses) are followed transparently, up to a reasonable
limit — the caller only ever sees the final response's status/body, not
the redirect chain itself (see Non-goals).

## Errors

`get()` only **rejects** when no HTTP response was received at all:

- A malformed URL, or a URL whose scheme isn't `http`/`https`, rejects
  with `ChainError { code: "INVALID_ARGUMENT" }` before any network
  activity happens.
- A request that never reaches a server — DNS resolution failure,
  connection refused, connection timeout — rejects with `ChainError {
  code: "UNAVAILABLE" }`.
- Any other native-side failure (TLS handshake failure, response body
  that isn't valid text, ...) rejects with `ChainError { code:
  "NATIVE_FAILURE" }`, the underlying error message in `message`.
- Calling `get()` outside a Chain (Tauri) runtime rejects with
  `ChainError { code: "UNSUPPORTED" }`, same as every other capability.

## Non-goals

- **No cookies, sessions, or auth headers.** This capability makes a
  single anonymous GET; it never stores or sends cookies, never manages
  a login session, and never accepts custom request headers. Fetching a
  page that requires authentication is explicitly Phase 11's concern
  (mneme's later, separate "Authenticated LMS Pages" request) — this
  capability does not grow toward it speculatively.
- **No response headers exposed.** Only `status`/`ok`/`body` — no
  `Content-Type`, no arbitrary header access. If a real need for header
  access shows up, that's a future contract change, not something to
  build in now on spec.
- **No redirect-chain inspection.** Redirects are followed automatically
  (so a real page behind an `http://` → `https://` or a CDN redirect
  still resolves normally), but the caller can't see the intermediate
  URLs/statuses — only Phase 10's actual need ("get me this page's
  HTML") is served here.
- **No caching.** Every call makes a fresh request.
- **No concurrent-request queueing or rate limiting.** The caller is
  responsible for not hammering a target server; this capability doesn't
  arbitrate between overlapping calls.
- **No streaming.** The whole response body is buffered into a single
  string before resolving — fine for an HTML page, not meant for large
  downloads.
- **No POST/PUT/DELETE or any other HTTP method.** GET only, matching
  the actual need ("retrieve a page"); a write-capable HTTP method is a
  different, unrequested capability.
- **A fixed request timeout is enforced native-side** (see
  `agent-docs/capabilities/http/research/WINDOWS.md`) so a stalled
  connection can't hang the caller forever — this is a safety default,
  not a configurable contract parameter; a per-call timeout override can
  be added later if a real need for one shows up.
