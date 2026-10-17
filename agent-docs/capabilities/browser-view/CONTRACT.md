# BrowserView — Semantic Contract

`desktop.browserView` shows other websites **inside the app's own
window**: an embedded view the app places, sizes, shows and hides as
part of its own layout (a browser tab's content area, a dev-server
preview pane). Each view has its own history and runs on a named,
persistent session, isolated from the app's own webview. Structural
contract: `capabilities/browser-view/contract.ts`.

Requested by Lazify (request 18,
`docs/chain-sdk-requests/18-embedded-browser-view.md` in the
lazify-chain repo) for its in-app browser tabs and agent preview.
`desktop.browser` (a separate sign-in window) is a different
capability and is unaffected.

## Views

- A **view** is created by `create()` and identified by the id it
  resolves with (unique for the app's lifetime, never reused). The app
  can have any number at once; nothing here is a singleton. Every event
  carries the view's `id`.
- A view belongs to the app page that created it. It closes on
  `close(id)`, when its window closes, and when that page reloads or
  navigates away, so a reloaded app never inherits views it can't see.
- **Placement.** `bounds` are in the app page's CSS pixels, relative to
  its viewport — exactly what `element.getBoundingClientRect()` returns
  — and Chain converts them, page zoom included. Chain doesn't follow
  the element: when the layout changes (window resize, a panel opening),
  the app calls `setBounds()` again, typically from a `ResizeObserver`.
  A width or height of 0 is allowed and shows nothing.
- **Stacking.** A view is drawn **above** the app's page, never under
  it: no app element (menu, popover, dialog) can show over a view. Hide
  the view (`setVisible(id, false)`) or shrink it while something must
  overlap it. Views created later are above earlier ones; keeping every
  tab's view alive and showing only the current one is the intended
  pattern, and a hidden view keeps its page, history and scroll
  position.
- **Focus.** A click in a view gives it keyboard focus. While it has
  focus, key presses go to the page in it, not to the app's page —
  app-level shortcuts that must keep working belong in the app menu.

## Sessions

`session` names a persistent cookie and site-storage store (1–100
characters), kept between launches by the OS web engine, apart from
the app's own webview and from every other session. Views sharing a
name share sign-ins; two different names never see each other's
cookies. **A name means the same store here and in `desktop.browser`**:
`desktop.browser.clearSession({ session })` signs every view on that
session out as well (open views keep their page until reloaded).

## What a view will show

A view only ever shows `http`, `https`, `about:`, `data:` and `blob:`
addresses, and never the app's own origin (its dev server, or the
custom scheme it's served from in a release build) — so a page in a
view can never call the app's commands. Other `localhost` ports are
allowed, so a dev server's preview works. This applies to `create()`,
`load()`, every navigation the page makes (nested frames included) and
every new window it asks for; a forbidden address in `create()`/
`load()` rejects `INVALID_ARGUMENT`, and a page's own attempt is
quietly refused. Pages in a view never get Chain's or Tauri's APIs, a
preload, or Node, and can't create views of their own.

## Methods

### `availability()`

Never rejects. `available` is false outside a Chain app, on macOS
before 14 (no separate persistent store there), and on Windows and
Linux today. When it's true, every other flag is true on macOS (and
`pictureInPicture` as long as WebKit still has the switch it needs);
they exist so an app can tell which parts work on a platform added
later.

### `create(options)`

Places a view in the window of the app page that called it, at
`bounds`, shown unless `visible: false`, and starts loading `url` (any
address a view may show — usually `http`/`https`, or `about:blank` for
an empty tab).
Resolves with its id once it's placed. Rejects `UNSUPPORTED` where
`available` is false and `INVALID_ARGUMENT` for a bad `url`, `session`
or `bounds` (not finite, or negative width/height).

Creation-time settings, fixed for the view's life:

- `allowFullscreen` (default false): a page's element fullscreen (a
  video player's fullscreen button) works. Off, the page sees
  fullscreen as unavailable.
- `swipeNavigation` (default false): a two-finger horizontal swipe on
  the trackpad goes back/forward through the view's own history, with
  the OS web engine's own animation, as in Safari. History moves made
  this way are navigations like any other — with `decideNavigation`
  they're held too (`kind: "history"`).
- `decideNavigation` (default false): see below.
- `allowPictureInPicture` (default false): a page's video may float out
  through `requestPictureInPicture()` (on a click, or from
  `runInFrames()`). Off, every call fails `NotSupportedError`. macOS has
  no public switch for this; Chain sets WebKit's unpublished one, after
  checking it exists — `availability().pictureInPicture` is false where
  it doesn't.
- `inspectable` (default false): Web Inspector reaches the view —
  "Inspect Element" in its right-click menu, and Safari's Develop menu.
  Opening or closing the inspector from code isn't offered (no public
  API for it).

**Permissions are fixed, never asked.** Camera, microphone,
geolocation, notifications, motion sensors and every other permission a
page may request are always refused, for every site, without any
prompt. (A request made while the view's window is hidden or behind
others waits until it's visible, then is refused — the engine's own
rule.) Copying to the clipboard in response to a click works (the
web's own rule); reading it doesn't. Pointer lock, keyboard lock and
DRM-protected media get whatever the OS web engine gives an embedded
view by default — not controlled here.

### `close(id)`

Removes the view and ends its page. Pending navigation requests are
refused. A no-op for an id that's already closed.

### `setBounds(id, bounds)`, `setVisible(id, visible)`

Move/resize and hide/show, as above. Reject `NOT_FOUND` for an unknown
or closed id and `INVALID_ARGUMENT` for bad bounds.

### `load(id, url)`, `back`, `forward`, `reload`, `stop`

Navigation on the app's behalf; these are never held for `answer()`.
`back`/`forward` are no-ops when there's nowhere to go, and `reload()`
after `onCrash` loads the last address shown again. `load()` rejects
`INVALID_ARGUMENT` for an address a view may not show. All reject
`NOT_FOUND` for an unknown id.

### `state(id)` and `onState`

`state(id)` → `{ id, url, title, loading, canGoBack, canGoForward }`.
`onState` fires with the whole state whenever any of it changes: a
load starting (`loading: true`) and finishing, the address changing,
the title changing, and back/forward becoming possible or not. `url`
is the address being loaded or shown: it changes as soon as a load
starts (before the page arrives — like a browser's address bar), on
redirects and on in-page/history-API navigation, and goes back to the
page still shown when a load fails. `title` empties while a new page
loads. Each change is its own event; an app keeps the latest per id.
No polling.

### `onLoadFailed`

The view's page failed to load — `{ id, url, reason, description,
nativeCode, nativeDomain }`. `reason` is one of:

| `reason` | Meaning |
| --- | --- |
| `offline` | the computer isn't connected to a network |
| `host-not-found` | the site's name doesn't resolve |
| `unreachable` | the server didn't answer, refused, timed out or dropped the connection |
| `insecure-connection` | the certificate or secure connection couldn't be trusted |
| `blocked` | the OS refused the address (a restricted port, an insecure load policy) |
| `other` | anything else |

Mapping a reason to a message is the app's. A load that was
**interrupted rather than failed** — `stop()`, a newer navigation, a
refused navigation, a download-only response — isn't reported. Only
the view's page is reported; a nested frame that fails is the page's
own concern. `nativeCode`/`nativeDomain` are the engine's own values,
for logs only: they differ by OS, so never branch on them.

### `onCrash`

The view's page process exited (crashed, or was ended by the OS under
memory pressure). The view stays in place showing nothing; `reload()`
or `load()` starts it again.

## Deciding navigation and new windows

### With `decideNavigation: true` — `onNavigationRequest` and `answer()`

Every attempt by the page to leave for another address is held before
it starts and reported to `onNavigationRequest`, until the app calls
`answer(requestId, allow)`. That covers links, form submissions,
script (`location = …`), a nested frame trying to change the whole
view's page (`fromMainFrame: false`), and swipe/keyboard history
moves. It doesn't cover the app's own `load`/`back`/`forward`/
`reload`/`stop`, server redirects of a navigation already allowed, or a
nested frame navigating only itself — those go ahead without asking
(within "What a view will show").

Each request carries `url` (the destination), `kind`
(`link`, `form`, `history`, `reload` or `other`) and the initiator
fields below. A request not answered within **10 seconds** is refused.
Refusing leaves the view where it was — that's how the app sends the
user elsewhere instead (its own new tab, the system browser) without
the view going there too. Answering an already answered, expired or
unknown `requestId` does nothing.

Without `decideNavigation`, navigations simply go ahead and nothing is
reported until `onState`.

### `onNewWindow`

`window.open()`, `target="_blank"` and the like **never open anything
on their own**: the view stays as it is and `onNewWindow` reports
`{ id, url, features, …initiator }`, so the app can open its own new
view (a new tab), hand the address to the system browser, or ignore it.
`features` holds what the page asked for in `window.open()`'s
features string; a field is absent when the page didn't say.

A window opened **with no address yet** (`window.open()`, then its
location set a moment later) is held, invisible, until it gets its
first real address — then reported like any other — or dropped after
10 seconds with nothing reported. Either way, the page's handle to that
window is never a real window: the new page has no `window.opener`,
and the opener can't script it.

### The view's right-click menu

The engine's own menu, minus what a view can't do: every "… in New
Window" item and every "Download …" item is removed, since a view never
opens a window or downloads. What stays (seen on macOS 27): on a link,
Open Link, Copy Link and Share; on the page, Back (Forward when there's
somewhere to go) and Reload; Inspect Element with `inspectable`.
**Open Link** is an ordinary link navigation (`kind: "link"`, `button:
"primary"`). **Back/Forward and Reload** are the user's own navigations,
not the app's: with `decideNavigation` they're held like any other
(`kind: "history"`, `kind: "reload"`, `button: null`) — only the app's
own `back()`/`forward()`/`reload()` skip the question.

### The initiator fields

On both `onNavigationRequest` and `onNewWindow`:

- `sourceUrl` — the document that started it (empty when the engine
  doesn't say, as for the user typing in nothing a page controls);
  `fromMainFrame` — whether that's the view's own page or a nested
  frame.
- `modifiers` (`shift`, `control`, `alt`, `meta`) and `button`
  (`primary`/`middle`/`secondary`, or null when it wasn't a click) — of
  the click or key press that started it. Chain doesn't turn these into
  "foreground tab" / "background tab": a Command-click on a plain link
  arrives as an ordinary navigation request with `meta: true`, and the
  app's own policy decides what it means.
- `inputAgeMs` — milliseconds since the last real click or key press
  *inside this view* (null when none yet). Pointer movement and
  scrolling don't count, and a page can't fake it. It tells a click
  that opened something apart from a page redirecting itself on a timer
  with nobody touching it; the threshold is the app's.

## Filtering and scripting a session's pages

Added for Lazify's request 21 (`docs/chain-sdk-requests/21-filter-and-script-embedded-pages.md`
in lazify-chain): the hooks an ad and tracker blocker, and media
picture-in-picture, plug into. The blocking engine, its filter lists,
on/off switches and every per-site decision stay app code. Everything
here is **per session**: it applies to every view on that session, open
now or created later, and to no other session or the app's own page.
Nothing is remembered between launches; an app sets its rules and
scripts again at startup (cheap, see below).

### Rules — `setRules(list)`, `removeRules({ session, id })`

A rule list is an ordered array of `{ trigger, action }` rules, the
shape of the web's content-blocker format (Safari's), written in
camelCase. **Chain owns the supported subset**, below; anything else
rejects `INVALID_ARGUMENT`. The same rules are meant to work on every
platform: Chain evaluates them itself where the engine has no
declarative rules.

- `trigger.urlFilter` (required): a regular expression matched against
  the request's full URL. Supported: `.`, `*`, `+`, `?` (on a single
  character or group), character ranges `[a-z]`, `^`/`$` anchors,
  `(…)` groups — no `|`, `{n}` or backreferences. A rule matches when
  every other given field matches too: `urlFilterIsCaseSensitive`
  (default false); `resourceType` (any of `document`, `image`,
  `style-sheet`, `script`, `font`, `media`, `svg-document`, `popup`,
  `ping`, `fetch`, `websocket`, `other`; all when absent); `loadType`
  (`first-party`/`third-party` relative to the view's page); and at most
  one of `ifDomain`/`unlessDomain` (the host of the view's top-level
  page — `*example.com` includes subdomains) or `ifTopUrl`/`unlessTopUrl`
  (regular expressions on that page's URL). Both are about the view's
  page, never a nested frame's own address: an exception for
  `example.com` lifts the list's rules for everything that page loads,
  its cross-origin frames' requests included.
- **Keep `ifTopUrl`/`unlessTopUrl` off rules that don't need them** —
  each one grows the compiled list a lot. Lazify's 120,000-rule list
  with an `unlessTopUrl` on ~115,000 rules compiled to 150 MB and took
  72–110 s; the same intent as one closing rule — `{ urlFilter: ".*",
  resourceType: ["document"], loadType: ["first-party"] }` →
  `ignore-previous-rules` ("never block the page's own document") —
  compiled to 50 MB in ≈25 s.
- `action.type`: `block` (the request is never sent — a page's main
  document too), `block-cookies`, `css-display-none` (hide what
  `selector` matches on the pages the trigger matches — element hiding;
  pair it with `urlFilter: ".*"` and `ifDomain`), `ignore-previous-rules`
  (an exception: earlier rules in the same list stop applying to what
  this trigger matches), `make-https`.
- Rules apply to everything a page loads — its document, nested frames,
  scripts, images, fetch/XHR, media, websockets — **before the request
  is sent**, inside the web engine: no per-request round trip to the
  app. A blocked page document fails its load with `onLoadFailed`
  `reason: "blocked"`.
- `setRules()` resolves once the list is in place on every view of the
  session; each view uses it **from its next load** (navigation or
  `reload()`). Replacing a list with the same `id` swaps it; lists with
  different ids all apply (an exception in one list doesn't undo
  another's rules). `removeRules()` takes a list away the same way.
- **Large lists are compiled once.** Compiling is the engine's work and
  grows with the rules' patterns (seen on an Apple Silicon Mac: 100,000
  plain host rules ≈ 1.3 s; 60,000 pattern-heavy rules ≈ 10 s;
  Ghostery's full lists as 120,000 rules ≈ 25 s). Chain keeps the
  compiled list on disk, named by its exact content, so the same rules
  set again — this launch or any later one — skip compiling, and any
  change compiles afresh. Skipping still costs handing the rules over
  and loading the compiled list: ≈ 1 s for 60,000 small rules, ≈ 7 s
  for Ghostery's 14 MB under `chain dev` (a debug build — Chain's own
  part of that is ≈ 2.6 s there and ≈ 0.2 s in a release build). A list's old compiled versions are deleted
  when it's replaced or removed. One list holds at most **150,000**
  rules — split bigger sets into several lists.
- Rejects `INVALID_ARGUMENT` for a bad session, an empty `id`, an empty
  list, or a rule outside the subset — named by its index (`rule 41:
  …`), unsupported `|` and `{…}` included. A pattern the engine itself
  refuses rejects with the engine's reason ("Invalid or unsupported
  regular expression"), which doesn't say which rule.
- **No notice when a rule blocks something** — the engine only says so
  through unpublished API, so a blocked counter isn't offered.

### Page scripts — `setScript(script)`, `removeScript({ session, id })`

`source` runs as-is **in every frame** (the page and every nested one,
cross-origin included) of every page on the session, **at document
start — before the page's own scripts — in the page's own JavaScript
world**, so it can patch the page's globals (scriptlets). It runs again
for every new document (each navigation, each frame load); in-page
navigation (history API) doesn't start a new document, so a script
that cares watches for it itself. Scripts run in the order they were
first set. Like rules, a change applies from each view's next load.
The page's Content Security Policy doesn't apply to these scripts — on a
page whose policy is `script-src 'none'`, the page's own scripts don't
run but the session's do, and `ask()` works.

**Because it shares the page's world, the page can see and tamper with
whatever the script defines** — that's the point for scriptlets, and the
reason nothing the script does is trusted.

### The channel back — `chainBrowserView.ask()` and `handleMessages()`

In every frame of every view's pages — on every session, whether it has
scripts or not — Chain defines `chainBrowserView.ask(message)` before
any script runs. With no app handler it only ever rejects. It sends any
JSON-serialisable `message` to the app and returns a Promise of the
app's answer: `handleMessages(handler)` calls `handler({ id, frame,
message })` and sends back what it returns (awaited, JSON). `id` (the
view) and `frame` (`url`, `mainFrame`) come from the engine, not the
page — they can be relied on. **`message` can't**: any page can call
`ask()` itself, so treat it as untrusted page input and answer only
what any page may know (which rules apply to a URL), never anything
privileged.

- One handler for the app; calling `handleMessages()` again replaces
  it, and the function it returns removes it.
- The Promise rejects in the page when there's no handler, the handler
  throws, its answer isn't JSON, no answer comes within **10 seconds**,
  or the view closes first.

### `runInFrames(id, body)`

Runs `body` — the body of an async function, so it may `await` and
`return` a value — in the view's page and **every nested frame,
cross-origin included**, and resolves with one result per frame:
`{ frame, value }` (the awaited return, as JSON) or `{ frame, error }`
(it threw, or the frame went away — reported once, then left out). The
main frame comes first. It runs in a world of Chain's own, not the
page's: it sees the same DOM, but the page can't see or change its
variables, and it can't see page scripts' (nor `chainBrowserView`). It
runs **as if the user had just clicked** — what
`requestPictureInPicture()` and other click-only web APIs need
(`navigator.userActivation.isActive` is true); like a real click, that
lapses, so a timer the script starts doesn't get it.

Frames are known from the moment their document starts; one created
after the call isn't included. Rejects `NOT_FOUND` for an unknown view.

## Errors

| Code | When |
| --- | --- |
| `UNSUPPORTED` | not available here |
| `INVALID_ARGUMENT` | an address a view may not show, a bad session name or bounds |
| `NOT_FOUND` | an id that doesn't exist or is closed |
| `NATIVE_FAILURE` | anything else |

## Non-goals

- **A per-request callback to the app**, rewriting requests or
  response headers, or redirecting to a substitute body: rules are
  declarative and run inside the engine (above). A blocked counter.
- App scripts in the app's own page through this capability, or
  scripts in another session's views.
- A per-site or per-request permission decision; the engine's own
  permission prompts; `alert`/`confirm`/`prompt` dialogs (dismissed
  without showing — `confirm` answers false, `prompt` null).
- Downloads: a link or response that would download does nothing.
- Keeping `window.opener` for windows a page opens (see above).
- A raw swipe-gesture stream for an app-drawn indicator: the engine's
  own swipe (`swipeNavigation`) gives the gesture and its animation.
- The app's page drawing over a view, or a view following an element
  by itself.
- A separate OS window (that's `desktop.browser`), always-on-top
  windows, devtools, find-in-page, printing, zoom of a view's page.
- Browsing policy — which popups to allow, which errors to show how,
  history and tab organisation. All app code.
