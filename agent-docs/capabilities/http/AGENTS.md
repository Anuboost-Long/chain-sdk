# Http Capability — Agent Memory

Scope: `desktop.http.get(url)` — a single native-side HTTP GET, returning
`{ status, ok, body }`. Requested by mneme (see
`docs/chain-sdk-requests/04-lms-page-fetch.md` in the mneme repo) as its
fourth real capability gap: Phase 10 ("Basic LMS Page Import") needs to
fetch an arbitrary external course-page URL, which a webview `fetch()`
can't do reliably (CORS — see CONTRACT.md).

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/http.rs` / `packages/sdk/src/http.ts`

## What's already decided

- **Stateless — no lazy-opened directory/connection**, unlike
  `storage`/`files`. Every call is an independent request; there's no
  `HttpState` struct in `templates/lib.rs`, just a plain `async fn
http_get`.
- **Async Tauri command, using `reqwest`'s async client** (not
  `reqwest::blocking`). Every other capability's commands
  (`storage_*`/`files_*`) are sync `fn`s doing blocking I/O, which Tauri
  runs via its own blocking-thread dispatch — but `reqwest::blocking`
  internally builds its own single-thread Tokio runtime via `block_on`,
  which is documented to panic ("Cannot start a runtime from within a
  runtime") when called from a thread that's already inside a Tokio
  context, which a Tauri command's execution thread already is (Tauri v2
  runs on Tokio internally). An `async fn` command that `.await`s
  `reqwest::Client` sidesteps this entirely by just running as a normal
  task on Tauri's own existing runtime — no separate thread/runtime
  management needed on this capability's side. Not the pattern the other
  capabilities use, but the correct one for this one; don't "fix" it to
  match `storage`/`files`' sync style without re-deriving this reasoning.
- **`resolving != HTTP success`.** `get()` resolves for any received HTTP
  response (2xx through 5xx) with `ok` mirroring `Response.ok` from the
  web `fetch()` API on purpose (rule 2 — reuse a shape app developers
  already know) — it only _rejects_ when no response was received at all
  (bad URL, DNS/connection failure) or the native layer itself failed.
  See CONTRACT.md's Errors section for the exact `ChainErrorCode` mapping.
- **TLS backend: `reqwest`'s default (`default-tls`/`native-tls`), not
  `rustls-tls`.** This is the opposite of what the request doc's
  phrasing suggested — see `research/WINDOWS.md` for the full reasoning.
  Short version: `native-tls` reads the OS certificate store (Security
  framework / SChannel), which is what actually makes a
  corporate/school-installed root CA get trusted; `rustls` with its
  default bundled webpki roots would _not_ trust that same cert. Don't
  switch this to `rustls-tls` to "simplify the dependency tree" without
  re-reading that research doc first — it would silently break exactly
  the managed-machine case this capability needs to work on.
- **Fixed 30s request timeout, not configurable.** A safety default so a
  stalled connection can't hang the caller forever (see CONTRACT.md's
  Non-goals) — not exposed as a parameter until a real need for a
  different value shows up.
- Redirects are followed by `reqwest`'s own default policy (an
  independent, uninspectable step from this capability's point of view —
  see CONTRACT.md's Non-goals on redirect-chain inspection).
- **No new `capabilities/default.json` permission entry needed** — same
  reasoning `files`' `AGENTS.md` already documented: app-defined
  `#[tauri::command]`s aren't gated by the plugin ACL/permission system,
  only plugin-exposed commands are. This isn't a Tauri HTTP _plugin_
  (`@tauri-apps/plugin-http`), it's a plain custom command wrapping
  `reqwest` directly.
- URL scheme validated (`http`/`https` only) and parsed _before_ any
  network activity, so a malformed/unsupported-scheme URL fails fast
  with `INVALID_ARGUMENT` rather than surfacing as a confusing
  `NATIVE_FAILURE` from deep inside `reqwest`.

## Axios-style extension (2026-09-28)

The user asked for axios-shaped requests: `request(config)` plus
`get/delete/head/options(url, config)` and `post/put/patch(url, data,
config)` with `method`, `headers`, `params`, `data` and `timeout`; the
response gained `statusText`, `headers` and `data` (parsed JSON). This
retired the old "GET only / no headers / no response headers" non-goals.

- **Params and body are encoded in the SDK** (`packages/sdk/src/http.ts`):
  params → ordered `[key, value]` pairs (axios's `key[]` array style),
  body → `{ kind: text|json|form|bytes, value }` (bytes as base64), so
  Rust only appends pairs and sets a default Content-Type.
- **Non-2xx still resolves**, deliberately unlike axios — it's the
  capability's original contract and mneme's LMS import relies on it.
- `http_get` was removed from the template; `desktop.http.get(url)` now
  goes through `http_request` with the same `{ status, ok, body }` fields
  (plus the new ones). Apps need `chain update` together with the SDK.
- Verified: `cargo test -p chain-core http` (a capturing loopback server
  checks method, query encoding, headers, JSON/bytes bodies and explicit
  Content-Type) and end to end in `apps/playground` against httpbin.org
  (every method, params, JSON/form/bytes bodies, 404, HTML as text,
  timeout → `UNAVAILABLE`, bad header → `INVALID_ARGUMENT`).

## Status

Implemented and verified for real on macOS:

- `crates/core/src/http.rs` has passing unit tests exercising a real
  round trip against a local loopback TCP server (no internet
  dependency), invalid-URL/unsupported-scheme rejection, and
  connection-refused handling.
- Wired as a real Tauri command (`http_get`) in
  `packages/cli/templates/lib.rs` (propagates to every `chain
init`/`chain update`'d app) and in `apps/playground`.
- SDK wrapper (`packages/sdk/src/http.ts`) written, exported from
  `desktop.http`, typechecks clean (`tsc --noEmit` in `packages/sdk`).
- `cargo check`/`cargo test -p chain-core` pass for `crates/core`, and
  `cargo check` passes for `apps/playground/src-tauri` with the new
  command wired in.
- **Verified end to end for real** in `apps/playground`'s actual running
  window (`tauri dev --features chain-dev-inspector`, same approach
  `files`' own verification used — a hand-rolled client speaking the
  dev-inspector's TCP protocol directly, invoking `http_get` from the
  live webview via `window.__TAURI_INTERNALS__.invoke`, no temporary
  probe code needed in the app itself this time since the inspector
  already exposes any registered command by name):
  - A real request to `https://example.com` resolved with `status: 200`,
    `ok: true`, and the actual HTML body (559 bytes, a real `<!doctype
html>...` snippet) — confirms the whole JS → SDK-shape IPC call →
    Rust → `reqwest` → real TLS handshake → response-decode path works,
    not just that it compiles.
  - `ftp://example.com` rejected with the exact
    `"INVALID_ARGUMENT: unsupported URL scheme: ftp"` string
    `packages/sdk/src/http.ts` is built to parse into `ChainError {
code: "INVALID_ARGUMENT" }`.
  - A real 404 (`https://example.com/this-page-should-not-exist-xyz`)
    **resolved** (did not reject) with `status: 404`, `ok: false` —
    confirms the "resolving isn't HTTP success" contract behavior
    against a real server, not just the loopback unit test.
  - No files were modified in `apps/playground` to run this — the dev
    session was started and stopped cleanly (`git status` shows no diff
    there).

### Error statuses reject — 1 October 2026

mneme's request 27: a non-2xx status now rejects `HTTP_ERROR` with
`error.response`, like axios; `validateStatus` overrides it. SDK-only
(`validated()` in `packages/sdk/src/http.ts`), so the 404-resolves
verification above now describes the native layer only. Tested against
a fake `invoke`: 2xx resolves; 404 and 500 reject with the message and
parsed response; `validateStatus: () => true` and a custom predicate
resolve; network failures keep their codes; `validateStatus` is never
sent native-side. The mneme session then verified it in mneme's running
app against real servers: a 404 rejected `HTTP_ERROR` ("Request failed
with status 404 Not Found", `response.status` 404),
`validateStatus: () => true` resolved it with `ok: false`, and both
callers (`lms-import.ts`, `downloadImage.ts`) were moved to the
reject-by-default shape.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark the contract/component status
      stable until confirmed there (rule: no single-platform contracts).
      See `research/WINDOWS.md` for the specific risks (OS-cert-store
      trust behind a managed-machine proxy) and the verification
      checklist.
- [x] Binary responses (request 24, 2026-09-30): `responseType: "bytes"`
      + `maxBytes`. Unit tests against a loopback server (non-UTF-8 bytes
      intact after a 302, declared and undeclared bodies over `maxBytes`
      → `TooLarge`), and end to end in the playground on macOS:
      `https://github.com/rust-lang.png` (redirects to GitHub's avatar
      CDN) → 22 KB `Uint8Array`, PNG signature, `image/png`, stored and
      read back identical through `desktop.files`; `maxBytes: 1000` →
      `TOO_LARGE`; text requests unchanged.
- [ ] Add contract tests under `capabilities/http/tests/` — currently
      the only test is `crates/core/src/http.rs`'s Rust unit tests, the
      same gap `storage`/`files` currently have too.
- [ ] mneme actually building Phase 10's "Import Module" URL input on
      top of this (HTML parsing via `DOMParser`, content-type detection,
      building pages from the parsed result) — this capability only
      needs to reach mneme via `chain update`; the app-level feature is
      mneme's job, not this capability's.
- [ ] Real end-to-end exercise against an actual external LMS-shaped
      page specifically (redirects, non-ASCII/charset content, a
      realistic page size) — the verification so far covers a real
      external HTTPS request/404/invalid-URL against `example.com` (see
      Status above), which proves the whole path works against a real
      server, but not LMS-shaped edge cases like a redirect chain or
      non-UTF-8 content.

## Rules specific to this capability

- Don't add cookies/auth/header support, response-header exposure,
  caching, or a configurable timeout speculatively — each is a
  deliberate non-goal until a real app hits the need (see CONTRACT.md).
  Phase 11 (authenticated LMS pages) is explicitly a separate, later
  capability request, not scope creep on this one.
- Never let this grow into a general-purpose HTTP client (other methods,
  request bodies, streaming) without a real request driving it — see
  rule 7 in the root `AGENTS.md`.
