# Http — Windows Research

Source: initial survey in `docs/chain-sdk-requests/04-lms-page-fetch.md`
(mneme's capability request), which specifically flagged the
corporate/school-managed-machine proxy/root-cert risk. **Not yet verified
on real Windows hardware/CI — no Windows machine was available while
implementing this. Per chain-sdk rule 3 (no single-platform contracts),
this capability's contract stays Draft, not stable, until someone runs
the checklist below.**

## Approach

Same as macOS — `reqwest` is the same portable Rust HTTP client on
Windows; there is no Windows-specific Rust code in
`crates/core/src/http.rs`.

## The TLS-backend question the request doc raised

The request doc asked whether `reqwest`'s TLS backend respects the
Windows certificate store, or needs `rustls-tls-native-roots` instead,
because school-managed laptops sometimes intercept HTTPS via a proxy or
a locally-installed root certificate.

**Resolved: use `reqwest`'s actual default (`default-tls`, i.e.
`native-tls`), not `rustls-tls`.** This capability adds a plain
`reqwest = "0.12"` dependency with default features on — it does not
opt into a `rustls-tls` feature. The reasoning is the opposite of what
the request doc's phrasing suggested:

- `default-tls` (`native-tls`) delegates to the OS's own certificate
  verification — **SChannel on Windows**, which reads the Windows
  Certificate Store (`certmgr.msc`). A corporate/school MDM that installs
  its own root CA into that store (the standard way managed-device HTTPS
  interception works) is trusted automatically, the same way Edge/Chrome
  already trust it on that machine.
- `rustls` (if chosen instead, e.g. via `rustls-tls-webpki-roots`)
  ships a **bundled** Mozilla root store and does *not* consult the OS
  store at all by default — that would make this capability fail (TLS
  trust error) on exactly the managed machines the request doc is
  worried about, unless the `rustls-tls-native-roots` feature variant
  were used specifically to bridge rustls to the OS store. Plain
  `default-tls` gets this behavior for free without needing that extra
  feature flag.
- `reqwest`'s default features also read the OS/env proxy configuration
  (`HTTP_PROXY`/`HTTPS_PROXY` env vars, and system proxy settings), so an
  explicit forward-proxy setup (as opposed to a transparent
  cert-injecting one) should also work without extra configuration — not
  yet confirmed on real Windows hardware.

This is a reasoned choice based on how `native-tls`/SChannel and `rustls`
are documented to behave, not something exercised against a real
managed-Windows proxy/cert-injection setup — that's exactly what the
checklist below is for.

## Known risks to verify (not yet confirmed either way)

- **Whether SChannel actually picks up an MDM-pushed root CA in
  practice** on a real managed device — the reasoning above should hold,
  but "should" isn't "confirmed."
- **Timeout behavior under a proxy that silently drops connections**
  rather than actively refusing them — `http.rs`'s request timeout (see
  `crates/core/src/http.rs`) bounds this to a fixed 30s either way, but
  worth confirming that's actually long enough (and not needlessly long)
  for a real school-network round trip.

## Checklist for whoever verifies this on Windows

- [ ] `cargo build`/`cargo check`/`cargo test -p chain-core` succeed.
- [ ] `chain doctor` + a fresh `chain init` + `npm run dev` produces a
      working app whose `http_get` command works end to end against a
      real external URL (mirror the macOS verification steps in
      `AGENTS.md`).
- [ ] On a real managed/school Windows machine (or a simulated MITM
      proxy with a locally-installed root CA), confirm `desktop.http.get()`
      succeeds against an HTTPS URL routed through it, and fails with a
      clear error (not a silent hang) when it isn't trusted.
- [ ] Confirm the 30s request timeout is a reasonable fit for real
      school/corporate network latency.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and this
      capability's `component.json` once confirmed.
