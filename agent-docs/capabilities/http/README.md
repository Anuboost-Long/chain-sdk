# Http Capability (`desktop.http`)

## How it works

A single outbound HTTP GET, made native-side so it isn't subject to the
webview's own CORS rules. `crates/core/src/http.rs` uses `reqwest`'s
async client (default features — `default-tls`/`native-tls`, which reads
the OS certificate store rather than a bundled root list; see
`research/WINDOWS.md` for why that matters for corporate/school-managed
machines) to make the request and buffer the whole response body into a
string. Stateless — unlike `storage`/`files`, there's no lazily-opened
directory or connection to manage; every call is independent.

The Tauri command (`http_get`) is deliberately `async fn`, not the sync
style `storage_*`/`files_*` use — `reqwest`'s _blocking_ client builds
its own internal Tokio runtime and panics if called from a thread that's
already inside one, which a Tauri command's execution context already
is. An `async fn` command just runs as a normal task on Tauri's own
existing runtime instead of fighting it — see `AGENTS.md` for the full
reasoning.

**Error statuses reject, like axios.** Native code returns every
response, whatever its status; the SDK (`validated()` in
`packages/sdk/src/http.ts`) then rejects one whose status
`validateStatus` refuses — by default anything outside 200–299 — with
`HTTP_ERROR` and the response attached as `error.response` (mneme
request 27). `validateStatus: () => true` resolves everything. A request
that gets no response at all rejects as before: a malformed/
unsupported-scheme URL (`INVALID_ARGUMENT`), a request that never
reaches a server (`UNAVAILABLE`), or another native-side failure
(`NATIVE_FAILURE`).

Requested by mneme as its fourth real capability gap — see
`docs/chain-sdk-requests/04-lms-page-fetch.md` in the mneme repo.
mneme's actual problem: Phase 10 ("Basic LMS Page Import") needs to
fetch an arbitrary external course-page URL the user pastes in, and a
webview `fetch()` to that origin fails outright under ordinary CORS
rules since course pages don't send permissive
`Access-Control-Allow-Origin` headers. Parsing the fetched HTML
(`DOMParser`) needs no native help — only _retrieving_ it does.

## How to use it

```ts
import { desktop } from "@chain/sdk";

try {
  const response = await desktop.http.get("https://school.edu/course/123");
  const doc = new DOMParser().parseFromString(response.body, "text/html");
  // ... extract headings/paragraphs/links from `doc`
} catch (e) {
  // e.code === "HTTP_ERROR" for a 404/500/...; e.response is the response
}
```

Any method, headers, params and body, the way axios takes them:

```ts
const { data } = await desktop.http.get<Course[]>("https://api.school.edu/courses", {
  params: { term: "2026-fall", tags: ["bio", "chem"] }, // ?term=2026-fall&tags[]=bio&tags[]=chem
  headers: { Authorization: `Bearer ${token}` }
});

await desktop.http.post("https://api.school.edu/notes", { title: "Lecture 3" }); // JSON body
await desktop.http.put(url, new URLSearchParams({ a: "1" })); // form body
await desktop.http.request({ url, method: "PATCH", data: bytes, timeout: 5000 }); // raw bytes
```

`data` is the parsed JSON when the server answers JSON, else the text;
`body` is always the text. As in axios, a 4xx/5xx **rejects** `HTTP_ERROR`
unless `validateStatus` says otherwise:

```ts
const res = await desktop.http.get(url, { validateStatus: () => true }); // every status resolves
if (!res.ok) console.log(res.status);
```

A malformed URL or unreachable host rejects instead of resolving:

```ts
try {
  await desktop.http.get("not a url");
} catch (e) {
  // e.code === "INVALID_ARGUMENT"
}
```

Images and other binary files (request 24):

```ts
const image = await desktop.http.get<Uint8Array>(url, {
  responseType: "bytes",
  maxBytes: 10 * 1024 * 1024 // rejects TOO_LARGE past this
});
const reference = await desktop.files.write(image.data, "png");
```

`responseType: "bytes"` goes through a second command,
`http_request_bytes`, which returns one raw IPC buffer — a big-endian
u32 length, the response head as JSON, then the body
(`chain_core::http::request_bytes`, unpacked by `unframe()` in the SDK).

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `agent-docs/capabilities/http/CONTRACT.md` — the semantic contract
  (API behavior, error model, explicit non-goals — especially
  `validateStatus`/`HTTP_ERROR` and "no cookies/caching"). Check this
  before changing behavior or adding a method.
- `capabilities/http/contract.ts` — the exact types (`HttpApi`,
  `HttpResponse`); change this and both implementations below together,
  never one without the others.
- `agent-docs/capabilities/http/AGENTS.md` — the actual TODO checklist
  (Windows verification, contract tests, mneme's own Phase 10 UI) and
  what's already decided and why — especially the async-vs-sync command
  reasoning and the TLS-backend choice.
- `agent-docs/capabilities/http/research/MACOS.md` /
  `agent-docs/capabilities/http/research/WINDOWS.md` — platform-specific
  findings; Windows is explicitly **not yet verified** (no Windows
  machine was available) — read the checklist there before assuming it
  works, especially the corporate-proxy/root-cert reasoning.
- `crates/core/src/http.rs` — the actual Rust implementation (`get()`,
  `HttpError`, `HttpResponse`). Has real unit tests (`cargo test -p
chain-core`) against a local loopback server — no internet dependency
  — extend them here rather than only testing through the Tauri layer.
- `packages/sdk/src/http.ts` — SDK-side wrapper; handles the
  `isTauri()` check and wraps native failures into `ChainError`
  (mapping the `"INVALID_ARGUMENT: "`/`"UNAVAILABLE: "`/`"TOO_LARGE: "`-prefixed
  Rust errors onto their matching `ChainErrorCode`s, same prefix-matching
  pattern `files.ts` uses for `NOT_FOUND`), and unframes the bytes response.
- `packages/cli/templates/lib.rs` — the Tauri command layer (`http_get`,
  stateless, `async fn`) that every scaffolded app gets. This is the
  file `chain update` propagates — see
  `agent-docs/framework/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just the earlier ones.
