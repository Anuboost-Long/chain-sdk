# KeepAwake Capability (`desktop.keepAwake`)

## How it works

`start()` creates an IOKit power assertion
(`PreventUserIdleDisplaySleep`, or `PreventUserIdleSystemSleep` with
`display: false`) named with the app's reason. `stop()` releases it.
There's one per app, held in `crates/core/src/keep_awake.rs`, and a new
`start()` replaces it. The kernel releases it if the app exits or
crashes. The Tauri commands are in `templates/lib.rs`.

## How to use it

```ts
import { desktop } from "@chain/sdk";

// When the "Keep awake" setting is on and the first agent gets busy:
await desktop.keepAwake.start("Lazify: an agent is working");
// When no agent is busy any more (or the setting is turned off):
await desktop.keepAwake.stop();

const held = await desktop.keepAwake.status(); // { reason, display } | null
```

Check it with `pmset -g assertions` in a terminal.

## Files to check

- `agent-docs/capabilities/keep-awake/CONTRACT.md` — semantics and
  non-goals.
- `capabilities/keep-awake/contract.ts` — the types.
- `crates/core/src/keep_awake.rs` — the IOKit FFI, the single held
  assertion, and the `pmset` test.
- `packages/cli/templates/lib.rs` (and the playground copy) — the
  `keep_awake_*` commands.
- `packages/sdk/src/keep-awake.ts` — the SDK wrapper.
