# Attention Capability — Agent Memory

Scope: `desktop.attention` — window focus, system notifications whose
click brings the window back and names the notification, and one Dock
bounce. Requested by Lazify (request 07 in
`lazify-chain/docs/chain-sdk-requests/`) for agents that need the user
while they're in another app.

Read order: root `/AGENTS.md`, `/docs/ARCHITECTURE.md`, this file,
`CONTRACT.md` + `capabilities/attention/contract.ts`, `research/MACOS.md`,
then `crates/core/swift/ChainAttention.swift`, `crates/core/src/attention.rs`,
`packages/cli/templates/attention.rs` and `packages/sdk/src/attention.ts`.

## What's already decided

- **Its own capability, not part of `window`**: `window` is appearance
  and layout. This is about reaching the user.
- **UNUserNotificationCenter in Swift**, bridged with C callbacks like
  `share`. Every entry point checks for an `.app` bundle first, because
  the API aborts an unbundled process (`chain dev`, `cargo test`).
  Notifications are therefore `unavailable` under `chain dev`.
- **`notify()` never rejects on permission.** `denied` is only the
  user's answer. A system refusal is `failed` with macOS's message (this
  was split out after a refusal was misreported as `denied`).
- **The app's id is the request identifier**, so a repeat replaces
  rather than stacks, and it rides in `userInfo` back to the click
  handler.
- **The click brings forward the window that called `notify()`** (map in
  `AttentionState`), falling back to any webview window.
- **Focus and the bounce are Tauri's.** No Swift.

## Status

**Implemented on macOS. Notifications not verified end to end.**

- Unit test: an unbundled process gets `unavailable` from `permission()`
  and `notify()` without aborting.
- Under `chain dev` in a throwaway app: `isFocused()` and
  `onFocusChange` (true → false when Finder was activated),
  `requestAttention()` resolves, `notify()` → `unavailable`, and an empty
  id is `INVALID_ARGUMENT`.
- In a `chain build` `.app`: macOS refused permission without a prompt
  (`UNErrorDomain` 1), which now arrives as
  `{ outcome: "failed", message }`. A minimal ad-hoc `Mini.app` behaved
  the same; from `~/Applications` a prompt appeared once but its answer
  never came back. See `research/MACOS.md`.

- In Lazify (reported by its session): `chain update` merged cleanly, and
  the capability is wired into agent events (`isFocused` gate,
  `notify({ id: "agent-waiting:<run>" | "agent-done:<run>" })`,
  `requestAttention`, click → open the run). Its headless tests pass. The
  built-app check (prompt, notification, click) is pending with the user,
  and Lazify will report the exact `notify()` result.

## What's NOT done yet

- [ ] **Find out why macOS refuses ad-hoc test bundles on this machine**
      and verify `shown` → click → window forward → `onNotificationClick`
      in a real app. Lazify's own built app is the obvious place.
- [ ] Watch a Dock bounce actually happen.
- [ ] Windows: `research/WINDOWS.md`.
- [ ] Contract tests under `capabilities/attention/tests/`.

## Rules specific to this capability

- Never call `UNUserNotificationCenter` without the bundle check — it
  aborts `chain dev`.
- Never turn a permission state into a rejection.
