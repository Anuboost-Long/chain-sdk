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
