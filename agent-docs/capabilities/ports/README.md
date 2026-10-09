# Ports Capability (`desktop.ports`)

## How it works

`isFree(port)` binds a TCP listener to the port on `0.0.0.0`,
`127.0.0.1`, `::` and `::1` in turn, closing each straight away
(`crates/core/src/ports.rs`). If any bind is refused (in use, or not
permitted) the port isn't free. Four addresses, because with the
`SO_REUSEADDR` every dev server sets, macOS only reports a conflict on the
exact address a server holds. Vite on macOS listens on `::1` only, which
a single `0.0.0.0` probe can't see. Nothing is left listening.

## How to use it

```ts
import { desktop } from "@chain/sdk";

// Step up from the tool's default port to the first free one.
let port = 5173;
while (!(await desktop.ports.isFree(port)) && port < 5173 + 100) port += 1;

// Wait for a stopped server's port to be released before restarting.
const deadline = Date.now() + 5000;
while (!(await desktop.ports.isFree(5000)) && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 150));
}
```

`INVALID_ARGUMENT` unless the port is an integer from 1 to 65535. The
answer is a snapshot, not a reservation.

## Files to check

- `agent-docs/capabilities/ports/CONTRACT.md` — which addresses, why,
  and the non-goals.
- `capabilities/ports/contract.ts` — the type.
- `crates/core/src/ports.rs` — the probe and its unit tests.
- `packages/cli/templates/lib.rs` (and the playground copy) —
  `ports_is_free`, with the native range check.
- `packages/sdk/src/ports.ts` — the SDK wrapper.
- `agent-docs/capabilities/ports/research/` — the macOS bind-conflict
  table and the Windows checklist.
