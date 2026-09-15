# Platform Capability (`desktop.platform.getInfo()`)

## How it works

This is the first, and so far only, Chain capability — a deliberately
tiny vertical slice used to prove the full path (React → Chain SDK →
Chain Core (Rust) → Tauri → OS → back) before any other capability is
started (see `docs/MNEME_DESKTOP_FRAMEWORK.md` section 43).

It's defined as a three-layer contract:

1. `CONTRACT.md` — semantic contract (what it means, its non-goals).
2. `contract.ts` — structural contract (`PlatformInfo`, `PlatformApi` types).
3. (not written yet) contract tests under `tests/`.

Currently only the SDK-side stub exists
(`packages/sdk/src/platform.ts`), which throws a `ChainError` with code
`UNSUPPORTED`. The Rust side
(`crates/core/src/platform.rs::get_platform_info`) is an
`unimplemented!()` stub. Nothing calls Rust from the SDK yet — there's no
Tauri bridge, because `apps/playground` (where that bridge would live)
hasn't been scaffolded.

## How to use it

```ts
import { desktop } from "@chain/sdk";
const info = await desktop.platform.getInfo();
// currently rejects with { code: "UNSUPPORTED", message: "..." }
```

## Files to check

- `capabilities/platform/CONTRACT.md` — what this capability is supposed
  to mean/do; check this before changing behavior.
- `capabilities/platform/contract.ts` — the exact types (`PlatformInfo`,
  `PlatformApi`); change this and `packages/sdk/src/platform.ts` together,
  never one without the other.
- `capabilities/platform/AGENTS.md` — the actual TODO checklist for
  finishing this capability (Rust impl, Tauri wiring, playground app,
  contract tests). Read this first if you're picking up work here.
- `packages/sdk/src/platform.ts` — SDK-side stub; replace the `throw`
  once there's a real bridge to call into Rust.
- `crates/core/src/platform.rs` — Rust-side stub; replace
  `unimplemented!()` with real `os`/`arch`/`runtime_version` values.
- `docs/CAPABILITY_MATRIX.md` — update the status row here once any
  platform goes from ⏳ to 🧪/✅.
