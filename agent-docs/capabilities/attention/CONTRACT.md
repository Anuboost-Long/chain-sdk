# Attention Capability — Contract

## What this is

Getting the user back to the app when something needs them and they're
in another app. Driven by Lazify's request 07: an AI agent is waiting
for an answer, or has finished its turn, while the user works elsewhere.
Lazify notifies and bounces the Dock icon, and clicking the notification
opens that agent's run.

## Focus

- `isFocused()` — whether the calling page's window has keyboard focus.
- `onFocusChange(handler)` — called with `true`/`false` whenever that
  window gains or loses focus. Returns an unsubscribe function.

## Notifications

### `notify({ id, title, body? })`

Shows a system notification with the app's name and icon. `id` is the
app's own. A newer notification with the same id replaces the older one
in Notification Center instead of stacking.

- The **first** call asks the user for permission (macOS's own prompt)
  and resolves once they answer. Later calls don't ask again.
- It **never rejects** for permission reasons. It resolves with:
  - `{ outcome: "shown" }`,
  - `{ outcome: "denied" }` — the user said no, now or earlier in System
    Settings,
  - `{ outcome: "unavailable" }` — there's no notification system to ask:
    under `chain dev` (see below), or on a platform without an
    implementation yet,
  - `{ outcome: "failed", message }` — macOS refused without asking the
    user (e.g. "Notifications are not allowed for this application"),
    with its own message.
- It's shown even while the app is frontmost. Whether to notify is the
  app's decision, usually after `isFocused()`.
- Rejects `INVALID_ARGUMENT` for an empty `id` or `title`.

### `onNotificationClick(handler)`

When the user clicks a notification, Chain first brings the window that
showed it forward (un-minimise, show, focus, activate the app), then
calls every handler on that window's page with the notification's `id`.
A click while no handler is subscribed (e.g. mid-reload) still brings the
window forward, but the id is lost.

### `notificationPermission()`

`"granted" | "denied" | "notDetermined" | "unavailable"`. Never asks and
never rejects. Lazify uses it to say why nothing appeared.

### Only in a built app

macOS's notification system works only for a process running from an
`.app` bundle. `chain dev` runs a bare binary, where the system API
would abort the process with an uncaught exception. Chain checks for a
bundle first, so under `chain dev`, `notify()` resolves `unavailable`
and `notificationPermission()` returns `"unavailable"`. Notifications
need `chain build`. Focus and the Dock bounce work in both.

## `requestAttention()`

Bounces the Dock icon once (macOS's "informational" request), and only if
the app isn't already active. That's the system's rule, not Chain's.
Never repeats until the app is activated (that would be "critical").

## Errors

- `INVALID_ARGUMENT` — `notify()` without an `id` or `title`.
- `UNSUPPORTED` — outside a Chain runtime.
- `NATIVE_FAILURE` — anything else unexpected.

Permission states are results, never errors.

## Non-goals

- **No keep-awake.** That's a later request.
- **No in-app toasts.** The app draws those.
- **No scheduled, repeating or push notifications**, and no actions,
  replies, images or custom sounds. A notification is about something
  happening now.
- **No delivering clicks after the app quits.** A click on an old
  notification when the app isn't running just launches it.
