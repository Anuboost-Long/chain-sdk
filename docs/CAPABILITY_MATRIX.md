# Capability Matrix

Status of every Chain capability, per platform. Update this whenever a
capability's status changes — this is the fastest way for a human or agent
to see what actually exists vs. what's just planned.

| Capability      | macOS | Windows | Linux | Contract |
| --------------- | ----- | ------- | ----- | -------- |
| Platform/System | 🧪    | ⏳      | ⏳    | Draft    |
| Storage         | 🧪    | ⏳      | ⏳    | Draft    |
| Files           | 🧪    | ⏳      | ⏳    | Draft    |
| Http            | 🧪    | ⏳      | ⏳    | Draft    |
| AgentServer     | 🧪    | ⏳      | ⏳    | Draft    |
| ProcessRunner   | 🧪    | ⏳      | ⏳    | Draft    |
| Microphone      | 🧪    | ⏳      | ⏳    | Draft    |
| Vision          | 🧪    | ⚠       | ⏳    | Draft    |
| Speech          | 🧪    | ⏳      | ⏳    | Draft    |
| Models          | 🧪    | ⏳      | ⏳    | Draft    |
| Tts             | 🧪    | ⏳      | ⏳    | Draft    |
| AudioRecorder   | 🧪    | ⏳      | ⏳    | Draft    |
| Browser         | 🧪    | ⚠       | ⏳    | Draft    |
| Embeddings      | 🧪    | ⏳      | ⏳    | Draft    |
| Window          | 🧪    | ⚠       | ⚠     | Draft    |

Legend:

```
✅ Tested
🧪 Experimental
⏳ Not implemented
⚠ Partial
❌ Unsupported
```

`platform` has a native implementation wired end to end on macOS,
verified running in both `apps/playground` and `mneme`'s own window —
still marked Experimental, not Tested, until contract tests exist and
it's verified on Windows too (`agent-docs/capabilities/platform/AGENTS.md`).

`storage` (SQLite-backed `migrate`/`query`/`execute`, the typed
`table()` builder, and `transaction()`) is implemented and
verified end to end on macOS — real inserts/queries through the full
React → SDK → Tauri → Rust → SQLite path in `mneme`'s running window,
including persistence across app restarts — but not yet verified on
Windows (`agent-docs/capabilities/storage/research/WINDOWS.md` has
specific risks to check first: WAL over network drives, antivirus file
locking).

`files` (managed local blob storage: `write`/`read`/`url`/`delete` by an
opaque reference) is implemented and verified end to end on macOS — a
real write/read/url/delete round trip through the full React → SDK →
Tauri → Rust → `std::fs` path in `apps/playground`'s running window,
confirmed byte-exact — and propagated to `mneme` via `chain update`, but
not yet verified on Windows
(`agent-docs/capabilities/files/research/WINDOWS.md` has the specific
risks to check first: MAX_PATH limits, antivirus file locking).

`http` (native-side HTTP requests — since 2026-09-28 axios-style: any
method, headers, params and JSON/form/bytes bodies, with `statusText`,
`headers` and parsed `data` in the response; originally a single GET) is implemented and verified end to end on macOS — a real request
through the full React → SDK → Tauri → Rust → `reqwest` path in
`apps/playground`'s running window, against a real external HTTPS URL
(`https://example.com`), confirming both the success path (200, real
HTML body) and that a non-2xx response resolves rather than rejects
(a real 404) — but not yet verified on Windows
(`agent-docs/capabilities/http/research/WINDOWS.md` has the specific
risk to check first: whether the OS certificate store is actually
consulted the way `native-tls`/SChannel is documented to on a real
managed-machine proxy/root-cert setup).

`agent-server` (a local `127.0.0.1`-only HTTP listener forwarding
requests to a JS handler, so an external AI agent process can call into
the app) is implemented and verified end to end on macOS — a real
external `curl` client hit a running server in `apps/playground`'s
window across several sequential requests (GET/POST, varying paths and
bodies), each correctly round-tripped through the native listener to a
live JS handler and back; the "only one server per app run" rejection
(`UNAVAILABLE`) was also exercised for real via React's dev-mode
double-effect-invoke naturally calling `start()` twice — but not yet
verified on Windows (`agent-docs/capabilities/agent-server/research/WINDOWS.md`
has the specific risk to check first: whether Windows Defender Firewall
prompts for a loopback-only listener). Native stays deliberately
protocol-agnostic — no MCP/JSON-RPC awareness — see that capability's
`CONTRACT.md` for why.

`process-runner` (spawn an executable by argv array and stream its
stdout/stderr back incrementally, with exit-code reporting and early
`kill()`) is implemented and verified end to end on macOS — a real
subprocess spawned in `apps/playground`'s window produced output whose
arrival timing matched its own real `sleep` calls almost exactly (proof
it streams rather than buffers to exit), stderr was captured separately
from stdout, a real non-zero exit code round-tripped correctly, and
`kill()` stopped a long-running process well before its own timeout —
but not yet verified on Windows, where the risk is real and specific
(`agent-docs/capabilities/process-runner/research/WINDOWS.md`: npm-
global-installed CLIs are often `.cmd`/`.ps1` shims, and `Command::new`
doesn't do PATHEXT-aware extension search the way `cmd.exe` does — three
candidate fixes documented, none implemented yet). Native stays
deliberately generic — no AI-CLI/output-format awareness, and no
compiled-in executable allowlist (that's Phase 28's job) — see that
capability's `CONTRACT.md` for why.

`files.pick()` (mneme request 16 — the OS open panel attached to the
app window as a sheet, returning names and bytes, never paths),
`files.save()` (request 17 — the save panel as a sheet on every call,
native writes the bytes, returns only the file name), and
`processRunner.run()`'s `{ fileReference }` arguments (request 14 — a
managed file's path substituted natively as one argv element) are both
verified end to end on macOS in a real `chain dev` app; Windows is
unverified for all three (see each capability's `research/WINDOWS.md`).

`microphone` (mneme request 18) has no JS API: the webview's own
`getUserMedia` + `MediaRecorder` work once the app declares
`chain.permissions.microphone` in `package.json`. Verified on macOS in a
bundled playground build launched on its own (TCC prompt → AAC
`audio/mp4` recording with pause/resume), and under `chain dev` since
request 22 (the dev binary relaunches itself responsible for its own
prompts — see its CONTRACT.md). Windows
unverified (`agent-docs/capabilities/microphone/research/WINDOWS.md`).

`vision` (mneme request 20 — `recognizeText(bytes, options)` and
`languages()`, Vision's `VNRecognizeTextRequest` via objc2) is verified on
macOS: PNG/JPEG/WebP/GIF/HEIC and rotated images read exactly, blank
images resolve empty, and `INVALID_ARGUMENT`/`UNSUPPORTED` arrive as
`ChainError`s end to end in `apps/playground`. Windows (`Windows.Media.Ocr`
through the `windows` crate) is written and compile-checked but never run
— hence ⚠; checklist in `agent-docs/capabilities/vision/research/WINDOWS.md`.
`recognizeDocument` (request 23 — paragraphs, tables with spans, lists
through macOS 26's Swift-only `RecognizeDocumentsRequest`) is verified on
macOS 26.6; `UNSUPPORTED` on older macOS and Windows.

`speech` (mneme request 19 — `transcribe(reference, {locale},
onProgress)`, `cancel()`, `locales()`) is verified on macOS 26 through
SpeechAnalyzer (chain-core's first Swift code, `crates/core/swift/`): a
59.9-minute lecture in 51 s with continuous timestamps, cancel,
one-at-a-time and every error path, end to end in `apps/playground`. The
pre-26 `SFSpeechRecognizer` fallback is only partly verified (it needs
Siri & Dictation on). Windows is `UNSUPPORTED`. Because of the Swift
bridge, apps now carry a template `build.rs` and a macOS 12 minimum —
see `agent-docs/capabilities/speech/research/MACOS.md`.

`models` (mneme request 21 — `desktop.models` install/cancel/list/remove
plus `speech.transcribe`'s `engine` option on a bundled, GPL-free
sherpa-onnx) is verified on macOS end to end in `apps/playground`,
including an hour-long transcription with Moonshine tiny-en. The Windows
archive is pinned but has never been built there.

`tts` (request 21 part 2 — `voices`/`synthesize` with Kokoro/Piper, plus
`compile` for audiobook mode: one AAC file with per-segment timings) is
verified on macOS end to end; `compile` is `UNSUPPORTED` on Windows until
its encoder exists (`agent-docs/capabilities/tts/research/WINDOWS.md`). It links espeak-ng (GPL-3.0), so it's only
compiled into apps that opt in with package.json `"chain": { "gpl": true }`
(`agent-docs/capabilities/models/research/LICENSING.md`).

`audioRecorder` (request 32 — microphone, the computer's own audio, or
both mixed, streamed to AAC) is verified on macOS in a bundled
`apps/playground` build: all three sources transcribed back correctly,
pause, cancel, silence kept in the duration. The refusal path
(`PERMISSION_DENIED`) isn't verified yet. Echo cancellation for "both"
(request 33, vendored SpeexDSP, on by default) is verified live on
MacBook speakers: the 46 ms echo peak disappears with it on. Noise
suppression and gain control on the microphone (request 34, same
library, off by default) are unit-tested; noise suppression is also
verified live (−14 dB on noise from the speakers). A bug where the
microphone's gain raised leftover echo back to lecture level is fixed:
the gain now only applies while someone speaks. Choosing the microphone
(request 35: list, pick by id, avoid Bluetooth headset mics, survive the
mic disappearing) is verified with a simulated device; real Bluetooth
needs AirPods. Windows
reports nothing available until the WASAPI plan in its
research/WINDOWS.md is built.

See `docs/FRAMEWORK_CANDIDATES.md` for what's next.

`browser` (request 36 — a separate signed-in browser window with its own
persistent session, a toolbar with app buttons, reading the page and its
same-origin frames, fetching with the session) is verified on macOS 27 in
`apps/playground`'s dev build against a local test site: sign-in cookie
through a redirect, persistence across relaunch, two isolated sessions,
read with nested same-origin frames and a cross-origin frame listed as
unreadable, fetch with the window open and closed, a `window.open`
popup messaging its opener and closing itself, app button presses (and a
forged press from the page refused), sign out open and closed. Windows is
⚠: the same Tauri code compiles for it but has never run there
(`agent-docs/capabilities/browser/research/WINDOWS.md` has the checklist).

`embeddings` (request 37 — sentence vectors for search by meaning, from a
downloaded ONNX model and its Hugging Face tokenizer, on the ONNX Runtime
sherpa-onnx already links) is verified on macOS arm64: against Python
onnxruntime reference vectors for bge-small-en-v1.5 and
multilingual-e5-small in `cargo test`, and end to end in
`apps/playground` with both models installed through `desktop.models`
(every error code, cancel mid-run, unload). Windows is the same Rust on
the same runtime but has never been built there
(`agent-docs/capabilities/embeddings/research/WINDOWS.md`).

`window` (request 38 — the app window's chrome: title bar style and
size, title text, window buttons, appearance, background colour,
page-declared drag regions, insets and full screen) is verified on macOS
27 in `apps/playground`'s dev build: startup options from `package.json`
on the first frame, the native bar sizes (32/40/52) with buttons where
AppKit puts them through repeated zooms, a custom button position held
through zooms and reset, click routing by AppKit hit-testing (drag areas
to the native drag view, page and window buttons to themselves), and the
window colour behind the page. A physical drag/double-click and full
screen with real events are still to be confirmed. Windows and Linux are
⚠: only the portable subset (standard/hidden, appearance, background,
drag, full screen) applies, through Tauri's own calls, never run there
(`agent-docs/capabilities/window/research/WINDOWS.md`).
