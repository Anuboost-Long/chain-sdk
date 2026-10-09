# Share Capability — Agent Memory

Scope: `desktop.share` — the system share menu for app-owned files.
Requested by mneme (request 42, `docs/chain-sdk-requests/42-share-file.md`
in the mneme repo) for "Share as PDF".

## What's already decided

- **Files are `desktop.files` references, never paths.**
- **Every share stages copies** under the recipient's names in
  `Caches/<app>/chain-share/<id>/`: the managed names are random ids, and
  it lets the app delete its file as soon as `show()` is called. Swift
  deletes the folder on cancel only; `sweep` removes folders older than a
  day. **Never delete on `didShareItems`**: Copy reports done after
  putting only the file URL on the pasteboard, so pasting failed (mneme
  report, 8 October 2026).
- **The result is the menu's outcome**, not delivery: "picked" with the
  display name (no stable public id on macOS), or "cancelled".
- One menu at a time (`MENU_OPEN`); text-only shares and sharing to a
  named service are non-goals.

## Status

- [x] macOS 27.0.1, `apps/playground` dev build, 8 October 2026 — see
      research/MACOS.md "Verified" (menu anchored, staged name, delete
      straight away, UNAVAILABLE/INVALID_ARGUMENT/NOT_FOUND, cancel).
- [x] `cargo test -p chain-core --lib share::` (names, staging, sweep,
      result shape).
- [x] Propagated to mneme with `chain update`, 8 October 2026 (lib.rs
      merged, pdf.rs added; third run "Already up to date"); its native
      `cargo check` clean.

## Not done yet

- [ ] A person picks AirDrop/Mail/Messages: `picked` + service name,
      Mail subject/body; Copy then paste in Finder/Messages works.
- [ ] mneme: a real run from its share button.
- [ ] Windows — research/WINDOWS.md. Linux not planned.
- [ ] Contract tests under `capabilities/share/tests/`.
