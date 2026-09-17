# Platform Capability — Agent Memory

Scope: `desktop.platform.getInfo()` only. This is the first capability in
Chain, used to prove the full vertical slice (React → Chain SDK → Chain
Core (Rust) → Tauri → OS → back) before any other capability is started.
Do not add other platform-related methods here without a real Mneme
requirement driving them.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. Relevant source in `crates/core` / `packages/sdk`

## What's already decided

- API shape: `desktop.platform.getInfo(): Promise<PlatformInfo>` — see `contract.ts`.
- No arguments, no options — this call is intentionally trivial.
- Response is `{ os, arch, runtimeVersion }`. No other fields until a real
  requirement asks for them (do not add `hostname`, `locale`, etc.
  speculatively).

## Status

The full vertical slice works end to end **on macOS**:
`apps/playground` (React) → `@chain/sdk`'s `platform.ts` (calls
`invoke("get_platform_info")` via `@tauri-apps/api/core`) →
`src-tauri/src/lib.rs`'s `get_platform_info` command → `chain_core::platform::get_platform_info` → back. Verified by actually
running `apps/playground` (`npm run tauri dev`), not just compiling it.

Mneme (separate repo) now has its own real Tauri shell (via `chain
init`), wired identically — its `src-tauri/src/lib.rs` registers
`get_platform_info` the same way, and its `Cargo.toml` points
`chain-core` back at this repo's `crates/core`. Its `npm run dev`
(`tauri dev`) compiles and runs; visual confirmation of the ready state
in Mneme's own window is still pending.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark the contract stable until it's
      confirmed there too (rule: no single-platform contracts). Currently
      only macOS (aarch64) has been run.
- [ ] Add contract tests under `capabilities/platform/tests/` that assert
      the shape of `PlatformInfo` and that `os`/`arch` are one of the
      documented literal values — right now the only test is
      `crates/core/src/platform.rs`'s Rust unit test, which only checks
      the Rust side, not the full contract.
- [ ] `packages/sdk/src/platform.ts` throws `UNSUPPORTED` when not
      running inside Tauri (e.g. a plain Node/tsx script) — this is
      correct per CONTRACT.md. `mneme` now has a real Tauri shell (see
      Status above), so this only applies to bare-script usage now, not
      to `mneme` itself.
- [ ] Get an explicit visual confirmation that `mneme`'s own window
      (not just `apps/playground`'s) renders the ready state — `mneme`'s
      `Home.tsx` calls `desktop.platform.getInfo()` on mount.

## Rules specific to this capability

- Do not let this capability grow into a general "system info" grab bag.
  If Mneme needs something like `hostname` or `locale`, that's a new,
  separate capability decision — discuss it, don't just add a field.
