# Ports — macOS Research

Verified on macOS (Darwin 27), 9 October 2026.

## Bind conflicts are per exact address

`std::net::TcpListener::bind` sets `SO_REUSEADDR` on Unix, as Node and
Kestrel do. With it, the BSD stack reports `EADDRINUSE` only for a bind
to the exact address a listener holds. One listener per row, then a probe
bind on each address:

| Server on   | probe `0.0.0.0` | `127.0.0.1` | `::`   | `::1`  |
| ----------- | --------------- | ----------- | ------ | ------ |
| `0.0.0.0`   | busy            | free        | free   | free   |
| `127.0.0.1` | free            | busy        | free   | free   |
| `::`        | busy            | free        | busy   | free   |
| `::1`       | free            | free        | free   | busy   |

(`::` is dual-stack on macOS, which is why it also blocks `0.0.0.0`.)

So one probe address can't answer "free for a dev server". Probing all
four (`crates/core/src/ports.rs`) catches a server on any of them.
Confirmed with real Node servers through the SDK: Electron's
`listen(port, "0.0.0.0")` probe read a `::1` server (Vite's default on
macOS) **and** a `127.0.0.1` server as free; `isFree` read both as busy.

## Other findings

- An unprivileged process can bind ports below 1024 on macOS (since
  10.14): `0.0.0.0:80` bound fine. The "permission refused → not free"
  rule is implemented but couldn't be exercised here.
- 100 sequential `isFree` calls through the SDK and IPC took 29 ms; the
  Rust unit test's 100 probes take a few ms.
- A server bound only to a LAN address isn't detected. No dev tool Lazify
  supports does that by default.
