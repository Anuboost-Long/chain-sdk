# Browser — Semantic Contract

`desktop.browser` opens a separate OS window holding a full web view —
a real browser for one site the user signs in to — with its own
persistent session. The app controls the window and reads the page the
user is looking at; it never sees the user's credentials. Structural
contract: `capabilities/browser/contract.ts`.

Requested by mneme (request 36) to import pages from a school's LMS
behind single sign-on.

## Sessions and windows

- A **session** is a named, persistent cookie and site-storage store
  (`session` option, default `"default"`, 1–100 characters). It's kept
  between launches by the OS web engine, apart from the app's own
  webview and from every other session. Sessions exist implicitly: using
  a name creates it.
- **One window per session.** Every method takes `{ session }` to pick
  it; events carry `session`.
- Nothing about a session is written anywhere the app can read: no API
  returns cookie values, passwords or tokens, and the app can't run
  script in the page. `read()` and `fetch()` are the only ways in.

## Methods

### `availability()`

Never rejects. `available` is false outside a Chain app, on Linux, and
on macOS before 14 (no separate persistent store there — falling back to
the app's own store would break the separation above). When `available`
is true every other flag is true on macOS and Windows today; they exist
so an app can tell which parts work on a future platform.

### `open(options)`

- Not open → opens a window at `url` (required, `http`/`https`), sized
  `width` × `height` (default 1100 × 800) with a minimum of
  `minWidth` × `minHeight` (default 480 × 360), and resolves once it
  shows. With `beside` (default true) it sits next to the app window on
  the same screen — right if there's room, else left, else centered.
- Already open → brings it forward and, if `url` is given, navigates
  there; if `buttons` is given, replaces them. Size, title and toolbar
  options are ignored.
- `title` fixes the window's title; without it the title follows the
  page's.
- The page behaves like a normal browser: JavaScript, cookies,
  redirects, single sign-on and multi-factor all work. `window.open` and
  `target=_blank` open a popup window on the same session, and a popup
  that closes itself returns to its opener — the shape Microsoft,
  Google and Okta sign-in use.
- The page may only navigate to `http`, `https`, `about:`, `data:` and
  `blob:` addresses, never to the app's own origin.
- Rejects `UNSUPPORTED` where `available` is false, `INVALID_ARGUMENT`
  for a missing/invalid `url`, a bad `session`, sizes that aren't
  positive numbers, or buttons with an empty/duplicate id or empty label.

### Toolbar

With `toolbar` (default true): Back, Forward, Reload and the current
address (read-only), then the app's buttons in order. Pressing an
enabled app button emits `onButton` with its `id` and the page's `url`
and `title`. `setButtons(buttons)` replaces the set while the window is
open — use it to enable/disable (`enabled`, default true) or relabel.
Buttons need the toolbar; with `toolbar: false` they're accepted and
not shown. Only the toolbar can press a button — the page can't fake a
press.

### `close()` and `onClose`

`close()` closes the session's window and its popups; a no-op if it
isn't open. `onClose` fires whenever the window closes, with `reason`
`"user"` (the user closed it) or `"app"` (after `close()`). The session
itself stays signed in.

### `current()` and `onNavigate`

`current()` → `{ session, url, title }` of the page shown, or `null`
when that session's window isn't open. `onNavigate` fires when a page
finishes loading and when the page's title changes — which also catches
in-page navigation (history API) on sites that set a title per page.
No polling.

### `read()`

Reads the page as it stands now, after its scripts ran:
`document.documentElement.outerHTML`, with the `url` and `title` it
came from. `frames` holds every frame the page itself is allowed to
read (same origin as its parent, nested frames included), each with its
own `url`; `unreadableFrames` lists the addresses of the rest. Reading
goes through the page's own same-origin rules — it never reaches into a
cross-origin frame.

Call it in response to the user (a button press); it reads whatever is
on screen then. Rejects `NOT_FOUND` when the window isn't open,
`UNAVAILABLE` while the page is still loading its document (or doesn't
answer within 10 s), and `UNSUPPORTED` when the page isn't an HTML
document (an image or PDF viewer).

### `fetch(url, options)`

A GET of an `http`/`https` URL with the session's cookies — for the
page's own login-protected pictures. Resolves `{ url, status,
contentType, bytes }` (`url` after redirects; each redirect hop gets its
own cookies). The window needn't be open. Cookies the response sets
aren't saved to the session.

Like `desktop.http`: a status outside 200–299 rejects `HTTP_ERROR` with
the response attached as `error.response`; no response at all →
`UNAVAILABLE`; a malformed or non-http(s) URL → `INVALID_ARGUMENT`;
more than `maxBytes` → `TOO_LARGE`. `timeout` defaults to 30 s.

### `clearSession()`

Signs out: deletes the session's cookies and site storage, and resolves
once they're gone. An open window stays open and reloads, now signed
out. Rejects `UNAVAILABLE` when the store can't be cleared (Windows:
the folder is still locked just after closing the window).

## Errors

| Code | When |
| --- | --- |
| `UNSUPPORTED` | not available here; `read()` on a non-HTML page |
| `INVALID_ARGUMENT` | bad URL, session name, size or buttons |
| `NOT_FOUND` | `read()`/`setButtons()` with no window open for that session |
| `UNAVAILABLE` | page still loading or not answering; network failure; session store can't be opened or cleared |
| `HTTP_ERROR` | `fetch()` got a status outside 200–299 (`error.response`) |
| `TOO_LARGE` | `fetch()` past `maxBytes` |
| `NATIVE_FAILURE` | anything else |

## Non-goals

- Reading any page other than the one shown, or crawling a site.
- Filling in or submitting login forms, or storing passwords.
- Running app-supplied script in the page, or exposing cookies.
- Downloads from the window, tabs, browser extensions, devtools in
  release builds, an editable address bar.
- POST or other methods in `fetch()`, or saving cookies it receives.
