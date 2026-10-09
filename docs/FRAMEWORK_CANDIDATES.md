# Framework Candidates

Tracks which capabilities are generalizable framework modules vs.
application-specific logic that only looks reusable. A capability only
moves from "used by one app" to "framework module" after a second real
application needs the same thing and the contract survives a second
platform implementation. See rule 7 in the root `AGENTS.md`.

## Platform / System

Used by:

- Mneme (real consumer — wired and running)

Generalizable:
Yes — every app needs basic OS/arch/runtime info.

Contract:
Draft (`agent-docs/capabilities/platform/CONTRACT.md`, `capabilities/platform/contract.ts`)

macOS:
Implemented, verified (`apps/playground` and `mneme`)

Windows:
Not started

Possible package:
`@chain/sdk` (bundled in core, not a separate package — too small to split out)

## Storage

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/01-local-storage.md` in the mneme repo, and
  `26-typed-queries.md` for `table()`/`transaction()`)

Generalizable:
Yes — SQLite-backed local storage is a near-universal desktop-app need,
not mneme-specific (the actual schema on top of it is mneme-specific).

Contract:
Draft (`agent-docs/capabilities/storage/CONTRACT.md`, `capabilities/storage/contract.ts`)

macOS:
Implemented, verified end to end in `mneme`'s running window (including
persistence across restarts)

Windows:
Not started — see `agent-docs/capabilities/storage/research/WINDOWS.md` for
specific risks to check before implementing/verifying (rusqlite's
`bundled` feature is itself cross-platform, so the Rust code needs no
changes; only verification is pending)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## Files

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/02-files.md` in the mneme repo)

Generalizable:
Yes — managed local blob storage (images, attachments, exports) is a
near-universal desktop-app need, not mneme-specific (deciding which
attachments to keep and how to render them is mneme's own logic, built
on top of this).

Contract:
Draft (`agent-docs/capabilities/files/CONTRACT.md`, `capabilities/files/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window
(byte-exact write/read round trip, asset-protocol URL, idempotent
delete) and propagated to `mneme` via `chain update`

Windows:
Not started — see `agent-docs/capabilities/files/research/WINDOWS.md` for
specific risks to check before implementing/verifying (`std::fs` is
itself cross-platform, so the Rust code needs no changes; MAX_PATH and
antivirus-locking risks are the same category `storage` already flagged)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## Http

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/04-lms-page-fetch.md` in the mneme repo)

Generalizable:
Yes — a webview `fetch()` to an arbitrary external origin hits ordinary
CORS restrictions in any desktop-app framework, not just mneme's case;
a native-side GET is a near-universal need for "import content from a
URL the user pastes in" (the actual HTML parsing on top is app-specific).

Contract:
Draft (`agent-docs/capabilities/http/CONTRACT.md`, `capabilities/http/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window
against a real external HTTPS URL (200 success path and a real 404
resolving rather than rejecting)

Windows:
Not started — see `agent-docs/capabilities/http/research/WINDOWS.md` for
specific risks to check before implementing/verifying (`reqwest` is
itself cross-platform, so the Rust code needs no changes; whether the OS
certificate store is actually consulted behind a managed-machine
proxy/root-cert is the specific risk to confirm, not a portability gap)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## AgentServer

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/09-agent-tool-server.md` in the mneme repo)

Generalizable:
Yes — a local, loopback-only HTTP listener that forwards requests to a
JS handler is a near-universal need for any desktop app that wants an
external process (an AI agent, a CLI tool, a companion script) to reach
its own logic; native stays deliberately protocol-agnostic, so nothing
about it is MCP- or mneme-specific (mneme's own MCP/JSON-RPC framing and
tool dispatch is app-level, built on top).

Contract:
Draft (`agent-docs/capabilities/agent-server/CONTRACT.md`,
`capabilities/agent-server/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window —
a real external `curl` process reached a live JS handler through the
native listener across several real requests, plus the
already-running/`UNAVAILABLE` rejection path

Windows:
Not started — see `agent-docs/capabilities/agent-server/research/WINDOWS.md`
for the specific risk to check before implementing/verifying (whether
Windows Defender Firewall prompts for a loopback-only bind; the
underlying `tiny_http`/`TcpListener` approach itself needs no Windows-
specific Rust code)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## ProcessRunner

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/10-subprocess-runner.md` in the mneme repo,
  plus `12-process-runner-stdin.md` for the one-shot `options.stdin`
  payload)
- Lazify (request 03 in `lazify-chain/docs/chain-sdk-requests/` — `cwd`,
  `env`, writing to stdin mid-run, and killing the whole process tree)

Generalizable:
Yes — spawning a named executable with an argv array and streaming its
stdout/stderr back incrementally is a near-universal desktop-app need
(any app that wants to run a CLI tool and show its progress live), not
mneme-specific; native has zero awareness of AI CLIs, `stream-json`, or
any other output format (mneme's own stdout parsing/dispatch is
app-level, built on top).

Contract:
Draft (`agent-docs/capabilities/process-runner/CONTRACT.md`,
`capabilities/process-runner/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window —
a real subprocess's output arrival timing matched its own real `sleep`
calls (proof it streams rather than buffers to exit), stderr captured
separately from stdout, a real non-zero exit code round-tripped
correctly, and `kill()` stopped a long-running process well before its
own timeout

Windows:
Not started — see `agent-docs/capabilities/process-runner/research/WINDOWS.md`
for a real, specific (not boilerplate) risk to resolve before
implementing/verifying: npm-global-installed CLIs are frequently
`.cmd`/`.ps1` shims on Windows, and Rust's `Command::new` doesn't do the
PATHEXT-aware extension search `cmd.exe` does, so a bare spawn can fail
against exactly the kind of CLI this capability exists to run; three
candidate fixes are documented there, none chosen yet

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## Microphone

Used by:
mneme (request 18 — Phase 16 "Audio Recording": record lectures and
notes in the page editor)

Generalizable:
Yes — any app that records audio needs the OS declaration; the recording
itself is the web platform's

Contract:
Draft (`agent-docs/capabilities/microphone/CONTRACT.md`; no `contract.ts` —
there's no JS API, only the `package.json` "chain.permissions.microphone"
declaration)

macOS:
Implemented and verified in a bundled playground build: TCC prompt, then
an AAC `audio/mp4` recording with pause/resume stored through
`desktop.files`

Windows:
Not verified — WebView2 is expected to show its own prompt; see
`agent-docs/capabilities/microphone/research/WINDOWS.md`

Possible package:
None — CLI build-time declaration plus web APIs

## Vision

Used by:
mneme (request 20 — Phase 15 "OCR": "Extract text" on image blocks and
chat attachments)

Generalizable:
Yes — exact on-device text extraction from an image is useful to any app
that handles screenshots or photos; nothing in it knows about mneme

Contract:
Draft (`agent-docs/capabilities/vision/CONTRACT.md`,
`capabilities/vision/contract.ts`)

macOS:
Implemented with Vision's `VNRecognizeTextRequest` (objc2 bindings) and
verified in `apps/playground` across five image formats, rotation,
language selection and both error paths

Windows:
Written against `Windows.Media.Ocr` (the `windows` crate) and
compile-checked, not yet run — see
`agent-docs/capabilities/vision/research/WINDOWS.md`

Possible package:
`@chain/sdk` (bundled in core)

## Speech

Used by:
mneme (request 19 — Phase 17 "Speech-to-Text": transcribe recordings,
then clean up and summarize the text with its existing AI actions)

Generalizable:
Yes — on-device transcription of stored audio fits any note-taking,
meeting or media app; it knows nothing about lectures

Contract:
Draft (`agent-docs/capabilities/speech/CONTRACT.md`,
`capabilities/speech/contract.ts`)

macOS:
Implemented. macOS 26+ uses SpeechAnalyzer through a Swift bridge
(verified end to end, including an hour-long file); older macOS falls
back to on-device `SFSpeechRecognizer` (partly verified)

Windows:
Not started — `UNSUPPORTED`; see
`agent-docs/capabilities/speech/research/WINDOWS.md`

Possible package:
`@chain/sdk` (bundled in core)

## Models

Used by:
mneme (request 21 — Settings → Extensions: optional open-source speech
models the user downloads and removes)

Generalizable:
Yes — verified, sandboxed download of data packs for bundled engines fits
any app offering optional on-device AI

Contract:
Draft (`agent-docs/capabilities/models/CONTRACT.md`,
`capabilities/models/contract.ts`)

macOS:
Implemented and verified end to end (install, cancel, integrity failure,
engine transcription of an hour-long lecture)

Windows:
Engine archive pinned, never built or run there

Possible package:
`@chain/sdk` (bundled in core)

## Tts

Used by:
mneme (request 21 part 2 — read-aloud with downloadable Kokoro/Piper voices)

Generalizable:
Yes, for apps that can accept GPL-3.0 — which is why it's a per-app opt-in
(`"chain": { "gpl": true }`), never a default

Contract:
Draft (`agent-docs/capabilities/tts/CONTRACT.md`, `capabilities/tts/contract.ts`)

macOS:
Implemented and verified end to end (Kokoro multi-lang v1.0, Piper)

Windows:
Archive pinned, never built

Possible package:
`@chain/sdk` (bundled in core; engine behind a Cargo feature)

## AudioRecorder

Used by:
mneme (request 32 — record lectures, calls and videos the laptop plays,
alone or with the microphone; request 33 — cancel the speakers' echo
from the microphone when recording both; request 34 — noise
suppression and gain control on the microphone; request 35 — choose the
microphone, avoid Bluetooth headset mics)

Generalizable:
Yes — any app that records meetings, lectures or its own demos

Contract:
Draft (`agent-docs/capabilities/audio-recorder/CONTRACT.md`, `capabilities/audio-recorder/contract.ts`)

macOS:
Implemented (Core Audio process tap + aggregate device, macOS 14.2+ for
system audio); verified end to end except the refusal path. Echo
cancellation (SpeexDSP, platform-independent Rust/C) verified live on
laptop speakers; noise suppression and gain control (same library)
unit-tested

Windows:
Not started — WASAPI loopback plan in research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Browser

Used by:
mneme (request 36 — sign in to a school LMS behind single sign-on and
import the page the student is on, with its login-protected pictures)

Generalizable:
Yes — any app that reads content behind a login the user holds (LMS,
intranets, research databases) without handling credentials

Contract:
Draft (`agent-docs/capabilities/browser/CONTRACT.md`, `capabilities/browser/contract.ts`)

macOS:
Implemented on Tauri's multi-webview window (`unstable` feature) with a
`WKWebsiteDataStore(forIdentifier:)` per session, macOS 14+; verified in
the playground against a local test site (see CAPABILITY_MATRIX.md)

Windows:
Same code through WebView2 (a user-data folder per session); compiles,
never run — checklist in research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Embeddings

Used by:
mneme (request 37 — search by meaning: index each page's passages on the
device, "By meaning" results in ⌘P, a `search_by_meaning` tool for Ask)

Generalizable:
Yes — any app with semantic search, deduplication or clustering over the
user's own text, without sending it anywhere

Contract:
Draft (`agent-docs/capabilities/embeddings/CONTRACT.md`, `capabilities/embeddings/contract.ts`)

macOS:
Implemented on the ONNX Runtime 1.28.2 inside sherpa-onnx's static
archive (C API, no second runtime) and the `tokenizers` crate; verified
against reference vectors and end to end in the playground

Windows:
Same code and runtime; never built — checklist in research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Window

Used by:
mneme (request 38 — run its dark nav bar to the top of the window with
the window buttons on it, instead of under macOS 27's solid title bar;
request 40 — show the window only once its launch screen has painted)

Generalizable:
Yes — every desktop app with its own top bar (the Notion/Linear/VS Code
look), and any app that wants its window colours right before the page
paints, or its window to appear already drawn

Contract:
Draft (`agent-docs/capabilities/window/CONTRACT.md`, `capabilities/window/contract.ts`)

macOS:
Implemented: objc2 property sets in `crates/core/src/window.rs`, a Swift
drag view, toolbar-backed bar sizes and button placer in
`crates/core/swift/ChainWindow.swift`; `showWhen` keeps the window
undrawn (alpha 0) until first paint/`show()`/timeout; verified in the
playground

Windows:
Portable subset through Tauri calls (hidden = no frame, theme,
background, drag, full screen); overlay and `showWhen` fall back; never run —
checklist in research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Pdf

Used by:
mneme (request 41 — "Share as PDF" for pages, summaries and flashcard
decks)

Generalizable:
Yes — any app that exports reports, notes or invoices as PDF from its
own HTML

Contract:
Draft (`agent-docs/capabilities/pdf/CONTRACT.md`, `capabilities/pdf/contract.ts`)

macOS:
Implemented: a hidden Tauri webview printed through WKWebView's
`printOperation`, redrawn with CoreText header/footer, metadata and
links in `crates/core/swift/ChainPdf.swift`; verified in the
playground. Complex scripts (Khmer, Arabic, Devanagari) don't copy or
search — a Quartz limitation, reported by `availability()`

Windows:
Not started — WebView2 `PrintToPdf`, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Share

Used by:
mneme (request 42 — send the PDF from "Share as PDF" by AirDrop,
Messages or Mail)

Generalizable:
Yes — any app that makes files people send on

Contract:
Draft (`agent-docs/capabilities/share/CONTRACT.md`, `capabilities/share/contract.ts`)

macOS:
Implemented: `NSSharingServicePicker` in
`crates/core/swift/ChainShare.swift` over staged, renamed copies;
verified in the playground except picking a service (needs a person)

Windows:
Not started — `DataTransferManager` share UI, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Folders

Used by:
lazify (requests 01 and 02 — open and index project folders, edit
`.env`, project-local data, change events), mneme (request 11 — read
other tools' session files incrementally, through declared folders)

Generalizable:
Yes — any app whose subject is the user's own files: editors,
developer tools, anything that opens a folder

Contract:
Draft (`agent-docs/capabilities/folders/CONTRACT.md`, `capabilities/folders/contract.ts`)

macOS:
Implemented in Rust (`std::fs`, `notify`/FSEvents, `rfd` picker,
`NSFileManager` Trash); verified end to end in a scaffolded app except
drag-and-drop and a real privacy-protection denial

Windows:
Not started — long paths, locked files and Trash in research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Terminal

Used by:
lazify (request 04 — dev servers and AI agent CLIs in real terminals
that survive a page reload)

Generalizable:
Yes — any app that embeds a terminal: IDEs, devtools, agent monitors

Contract:
Draft (`agent-docs/capabilities/terminal/CONTRACT.md`, `capabilities/terminal/contract.ts`)

macOS:
Implemented with `portable-pty` (openpty); verified end to end,
including reattaching after a real reload

Windows:
Not started — ConPTY through the same crate, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Ports

Used by:
lazify (request 05 — step a dev script up to a free port; wait for a
stopped server's port before restarting)

Generalizable:
Yes — any app that starts local servers (devtools, IDEs, local AI
runtimes)

Contract:
Draft (`agent-docs/capabilities/ports/CONTRACT.md`, `capabilities/ports/contract.ts`)

macOS:
Implemented in Rust (`std::net` binds on four addresses); verified end to
end against real Node servers

Windows:
Not started — different bind-conflict rules, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## Attention

Used by:
lazify (request 07 — notify and bounce the Dock when an agent waits for
an answer or finishes while the user is in another app)

Generalizable:
Yes — any app that does long work in the background

Contract:
Draft (`agent-docs/capabilities/attention/CONTRACT.md`, `capabilities/attention/contract.ts`)

macOS:
Partial — focus and the Dock bounce verified; notifications implemented
in Swift (`UNUserNotificationCenter`), bundle-only, shown/click path
unverified (macOS refused ad-hoc test bundles, research/MACOS.md)

Windows:
Not started — WinRT toasts, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## PageZoom

Used by:
lazify (request 08 — its zoom setting and Cmd +/−/0)

Generalizable:
Yes — any app with a zoom or text-size setting

Contract:
Draft (`agent-docs/capabilities/page-zoom/CONTRACT.md`, `capabilities/page-zoom/contract.ts`)

macOS:
Implemented through Tauri's `Webview::set_zoom` (WKWebView `pageZoom`);
verified end to end

Windows:
Not verified — WebView2 `ZoomFactor`, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)

## KeepAwake

Used by:
lazify (request 09 — keep the Mac awake while agents work)

Generalizable:
Yes — any app doing long unattended work (renders, syncs, agents)

Contract:
Draft (`agent-docs/capabilities/keep-awake/CONTRACT.md`, `capabilities/keep-awake/contract.ts`)

macOS:
Implemented with IOKit power assertions; verified end to end with `pmset`

Windows:
Not started — `PowerCreateRequest`, research/WINDOWS.md

Possible package:
`@chain/sdk` (bundled in core)
