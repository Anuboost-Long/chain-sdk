# KeepAwake Capability — Contract

## What this is

Stops the computer idle-sleeping while the app does unattended work.
Driven by Lazify's request 09: AI agents run for minutes without input,
and a Mac that sleeps mid-turn silently stalls them. Lazify holds this
while the "Keep awake" setting is on and at least one agent is busy.

## `start(reason, { display = true })`

Holds the app's **one** power assertion: an IOKit assertion on macOS,
`PreventUserIdleDisplaySleep` (the default, like Electron's
`prevent-display-sleep`), or `PreventUserIdleSystemSleep` with
`display: false` (the screen may still turn off). `reason` is what the
OS shows, e.g. in `pmset -g assertions`; a blank one rejects
`INVALID_ARGUMENT`.

Calling `start()` while one is held **replaces** it (the new reason and
option take effect, with no gap in between). There's never more than
one. Reference counting (which agents are busy) is the app's job.

## `stop()` and `status()`

- `stop()` releases it. Idempotent.
- `status()` returns `{ reason, display }` while held, otherwise `null`.

## Lifetime

The OS releases the assertion when the app's process exits, **crash
included** (verified with `kill -9`). Nothing outlives the app, which is
the reason this isn't `caffeinate` run through `process-runner`.

## Errors

- `INVALID_ARGUMENT` — blank `reason`.
- `UNSUPPORTED` — outside a Chain runtime, or on a platform without an
  implementation yet (Windows today).
- `NATIVE_FAILURE` — the OS refused the assertion.

## Non-goals

- **No waking a sleeping machine** and no scheduling.
- **No preventing sleep when the lid closes or the user chooses Sleep** —
  only *idle* sleep.
- **Nothing about the screen saver or locking.**
- **No per-caller counting.**
