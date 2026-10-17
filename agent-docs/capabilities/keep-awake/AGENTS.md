# KeepAwake Capability — Agent Memory

Scope: `desktop.keepAwake.start(reason, { display })` / `stop()` /
`status()` — one power assertion per app. Requested by Lazify (request
09 in `lazify-chain/docs/chain-sdk-requests/`).

## What's already decided

- **An OS assertion, not `caffeinate`**: the kernel releases it on exit
  or crash, and there's no child process to track.
- **One per app; `start()` replaces.** Lazify keeps its own busy-run
  counting. Replacement creates the new assertion before releasing the
  old one.
- **`display` defaults to `true`**, matching Electron's
  `prevent-display-sleep`, which Lazify used.
- **Plain FFI to IOKit from Rust.** No Swift, no new crates.

## Status

**Verified on macOS**: unit test (`pmset` lists it, replacement, stop
idempotent, blank reason rejected), and end to end in a throwaway app
through the SDK, including release after `kill -9`.

- In Lazify itself (reported by its session): with an agent busy,
  `pmset -g assertions` showed `PreventUserIdleDisplaySleep named:
  "Lazify: an agent is working"` for the app, released when the agent
  stopped.

## What's NOT done yet

- [ ] Windows (`PowerCreateRequest`), see `research/WINDOWS.md`.
- [ ] Contract tests under `capabilities/keep-awake/tests/`.
