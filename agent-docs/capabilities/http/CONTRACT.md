# Http Capability — Contract

## What this is

Outbound HTTP requests made native-side — any method, request headers,
query params and a body, shaped like axios. Exists because a webview
`fetch()` to an arbitrary external origin is subject to ordinary browser
CORS rules — the webview's own origin (`tauri://localhost` /
`http://localhost:1420` in dev) is cross-origin from whatever server the
app wants, and ordinary servers don't send permissive
`Access-Control-Allow-Origin` headers. Making the request native-side
sidesteps this, the same reasoning Tauri's own `@tauri-apps/plugin-http`
exists for. Started as a single GET for mneme's Phase 10 "Basic LMS Page
Import"; extended to the axios shape on 2026-09-28 at the user's request.

## `desktop.http.request(config)` and the method shortcuts

```
request<T>(config: {
  url: string;
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"; // any case; default GET
  headers?: Record<string, string>;
  params?: Record<string, value | value[]>;   // value: string | number | boolean | Date | null | undefined
  data?: string | URLSearchParams | Uint8Array | ArrayBuffer | object | number | boolean;
  timeout?: number;                           // ms, default 30000
  responseType?: "text" | "bytes";            // default "text"
  maxBytes?: number;                          // "bytes" only
}): Promise<HttpResponse<T>>

get / delete / head / options (url, config?)
post / put / patch             (url, data?, config?)
```

- `params` are appended to the URL's existing query, in order, the way
  axios serializes them by default: arrays repeat as `key[]=a&key[]=b`,
  `null`/`undefined` are skipped, `Date`s become ISO strings, everything
  else `String(value)`.
- `data` decides the body and, **only when `headers` doesn't set
  `Content-Type`**, its type: a string is sent as-is
  (`text/plain;charset=utf-8`); `URLSearchParams` as a form
  (`application/x-www-form-urlencoded`); `Uint8Array`/`ArrayBuffer` as raw
  bytes (`application/octet-stream`); anything else as JSON
  (`application/json`).
- `headers` are sent as given; an invalid name or value rejects before
  anything is sent.

Resolves with:

```
{ status, statusText, ok, headers, body, data }
```

- `ok` is `status` 200–299. **A non-2xx response still resolves** — unlike
  axios, which rejects. Check `ok` (or `status`). This is the contract
  `get(url)` has had from the start, kept so existing callers don't change.
- `headers` — response headers, names lowercased, repeated headers joined
  with `", "` (axios's shape).
- `body` — the response body as text.
- `data` — `body` parsed as JSON when the response's `Content-Type` is
  JSON (`application/json` or `+json`) and it parses; otherwise `body`.

Redirects are followed transparently (the result is the final response).

### Binary responses — `responseType: "bytes"` (mneme request 24)

```ts
const image = await desktop.http.get<Uint8Array>(url, { responseType: "bytes", maxBytes: 10 * 1024 * 1024 });
image.headers["content-type"]; // e.g. "image/png"
await desktop.files.write(image.data, "png");
```

- `data` is the body as a `Uint8Array`, untouched; `body` is `""`.
  Everything else (`status`, `ok`, `headers`, redirects, `timeout`) is as
  for text.
- `maxBytes` — a body longer than this rejects `TOO_LARGE`. Checked
  against `Content-Length` before reading and again as the body arrives,
  so an oversized or undeclared body is never read past the limit. It
  must be a whole number ≥ 0 (else `INVALID_ARGUMENT`); ignored for text.
- The bytes cross IPC as one raw buffer, not JSON, so a 10 MB image stays
  10 MB.

## Errors

Only a request that gets **no** HTTP response rejects:

- `INVALID_ARGUMENT` — malformed URL, a scheme other than `http`/`https`,
  an unknown method, or an invalid header name/value. Nothing is sent.
- `UNAVAILABLE` — DNS failure, connection refused, or the `timeout`
  elapsed (including while reading the body).
- `TOO_LARGE` — `responseType: "bytes"` with a body over `maxBytes`.
- `NATIVE_FAILURE` — anything else native-side (TLS failure, …), with the
  underlying message.
- `UNSUPPORTED` — called outside a Chain (Tauri) app.

## Non-goals

- **No cookies or sessions.** No cookie jar: `Set-Cookie` is exposed in
  `headers` but never stored or sent back. An app that needs a session
  sends the header itself.
- **No downloads to disk.** `responseType: "bytes"` holds the whole body
  in memory — right for images and small files (set `maxBytes`).
  `desktop.models` handles large downloads straight to disk.
- **No multipart/`FormData` bodies, no upload/download progress, no
  streaming.** Whole bodies only.
- **No interceptors, instances, `baseURL` or cancel tokens** — axios's
  request shape, not its client machinery. Apps wrap `request()` if they
  want those.
- **No redirect-chain inspection** — only the final response is seen.
- **No caching and no rate limiting.**
