# Http — macOS Research

Source: initial survey in `docs/chain-sdk-requests/04-lms-page-fetch.md`
(mneme's capability request), expanded during implementation.

## Approach

A plain outbound HTTP GET is not a native macOS surface — `reqwest`
(built on `hyper`) is a portable Rust HTTP client, the same reasoning
`storage` already established for `rusqlite` and `files` for `std::fs`.
There is no macOS-specific Rust code in `crates/core/src/http.rs`.

- **TLS backend**: `reqwest`'s default feature set uses `default-tls`
  (the `native-tls` crate), which on macOS delegates to Apple's Security
  framework — the same trust store `Keychain Access` shows and the same
  one Safari/curl use. No corporate-proxy/injected-root-cert risk here
  the way there could be with a bundled-roots TLS stack (see WINDOWS.md
  for why this matters more there).
- **System proxy**: `reqwest`'s default features on macOS include
  `macos-system-configuration`, which reads the OS's actual proxy
  configuration (System Settings → Network → Proxies), not just
  `HTTP_PROXY`/`HTTPS_PROXY` env vars. Relevant if a managed Mac routes
  traffic through an MDM-configured proxy.
- **App Sandbox / entitlements**: outbound network access needs the
  `com.apple.security.network.client` entitlement only when the app is
  sandboxed (Mac App Store distribution) — same caveat `storage`/`files`
  already carry for their own sandboxed-distribution edge case, not
  relevant to local `chain dev`/unsigned-build testing.

## Verified

- `cargo test -p chain-core` passes `http::tests::*` (round trip against
  a local loopback TCP server, invalid-URL rejection, connection-refused
  handling — see `crates/core/src/http.rs`) on this machine (macOS
  aarch64).
- `cargo check` succeeds for both `crates/core` and
  `apps/playground/src-tauri` with the new module wired in.

## Verified end to end

- Real end-to-end exercise through `apps/playground`'s actual running
  webview (`tauri dev --features chain-dev-inspector`, invoking
  `http_get` directly via the dev-inspector's `eval` bridge): a real
  `https://example.com` GET resolved `{ status: 200, ok: true, body:
  <real HTML> }`, `ftp://example.com` rejected with the expected
  `INVALID_ARGUMENT` error string, and a real 404 page resolved (rather
  than rejected) with `status: 404, ok: false` — see `AGENTS.md`'s Status
  section for the full detail. This is beyond the loopback unit tests: a
  real DNS resolution, a real TLS handshake against a real certificate,
  and the full JS → SDK-shape IPC → Rust → `reqwest` round trip.

## Not yet verified

- Behavior against a real LMS-shaped page specifically (redirects,
  `gzip`/`br` content-encoding, non-UTF-8 charset, a realistic page
  size) — `example.com` is a real server but a deliberately minimal one;
  the loopback test server and this manual check both exercise the
  plumbing, not LMS-shaped edge cases.
