# Ports Capability — Agent Memory

Scope: `desktop.ports.isFree(port)` — whether a dev server could bind a
TCP port right now. Requested by Lazify (request 05 in
`lazify-chain/docs/chain-sdk-requests/`) for dev-port stepping and
waiting out a .NET restart.

Read order: root `/AGENTS.md`, `/docs/ARCHITECTURE.md`, this file,
`CONTRACT.md` + `capabilities/ports/contract.ts`, `research/`, then
`crates/core/src/ports.rs`.

## What's already decided

- **A real bind, not `lsof`**: it answers exactly what the next server
  will hit, costs no process launch, and sees bound-but-not-listening
  sockets.
- **Four addresses: `0.0.0.0`, `127.0.0.1`, `::`, `::1`.** The address
  choice was left to Chain. With `SO_REUSEADDR` (every dev server sets
  it) macOS only conflicts on the exact address, so fewer addresses gave
  wrong "free" answers. See `research/MACOS.md`.
- **Busy = `AddrInUse` or `PermissionDenied`.** Missing address families
  (no IPv6) are skipped. Anything else rejects `NATIVE_FAILURE`.
- **The range check happens twice:** in the SDK (integer, 1–65535) and
  natively (`ports_is_free` takes a `u32`, so 70000 reaches the check
  instead of failing to deserialize).
- **One function, its own capability** (`ports`): it fits no existing one
  (`http` is outbound requests), and "find a free port" stays the app's
  loop.

## Status

**Implemented and verified on macOS**, 9 October 2026. Not on Windows.

- `crates/core/src/ports.rs`: 4 unit tests. A server on each of the four
  addresses makes the port busy and frees it on close; probing leaves
  nothing listening; 100 probes are quick; port 0 is rejected.
- End to end in a throwaway `chain init` app through the SDK and IPC:
  - Node servers on `::1:47311` and `127.0.0.1:47312` both read busy,
    where an Electron-style `0.0.0.0` probe read both as free.
  - An empty port read free, and 0, 70000 and 5173.5 rejected
    `INVALID_ARGUMENT`.
  - Stepping from 47311 found 47313, with 100 probes in 29 ms.
  - Both ports read free again after the servers stopped.
- In Lazify itself (reported by its session after `chain update`):
  `isFree(1420)` read busy for Vite's `[::1]`-only listener, and a second
  Vite dev server started through Lazify's scripts got `--port 5174`.
  It's wired into dev-port stepping and the .NET restart wait.

## What's NOT done yet

- [ ] Windows: `research/WINDOWS.md`'s checklist.
- [ ] The permission-refused path on a system that actually refuses
      (macOS allows unprivileged low ports).
- [ ] Contract tests under `capabilities/ports/tests/`.

## Rules specific to this capability

- Never leave a probe socket open past `is_free`'s return.
- Port tests hold `ONE_AT_A_TIME` and keep fixed ranges below the
  ephemeral ports (49152+). Run in parallel, they flaked about half the
  time: one test's momentary probe or just-released `bind(0)` port
  showed up as "busy" in another.
- Don't add process lookup or killing here — that's `process-runner`
  running `lsof`/`ps`, parsed by the app.
