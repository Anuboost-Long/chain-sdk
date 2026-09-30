# Vision Capability — Agent Memory

Scope: `desktop.vision.recognizeText(image, options)` and `languages()` —
on-device OCR for image bytes. Requested by mneme
(`docs/chain-sdk-requests/20-image-text-recognition.md` in the mneme repo)
for Phase 15 "OCR".

## What's already decided

- **objc2 bindings (`objc2-vision`, `objc2-image-io`), no Swift.** The
  Swift-only macOS 26 `RecognizeDocumentsRequest` would need a Swift
  build step in chain-core; `VNRecognizeTextRequest` read the test slides
  exactly, so it isn't worth that yet.
- **Raw IPC body, not `Vec<u8>` JSON.** `files.write` still uses a JSON
  number array; this capability doesn't, because photos are large.
  Options travel in the `chain-vision-options` header.
- **ImageIO pre-check** gives `INVALID_ARGUMENT` for undecodable bytes;
  Vision alone reports those as a generic failure.
- **Language tags are resolved, not passed through**: exact match
  (case-insensitive), else a bare language onto its first supported
  variant, else `UNSUPPORTED` naming the tag. Omitted languages turn on
  `automaticallyDetectsLanguage`.
- Blocking thread (`spawn_blocking`) — accurate recognition takes
  ~200–500 ms per image.

## Status

- [x] macOS 26.6 (Apple Silicon): `cargo test -p chain-core vision`, a
      probe over PNG/JPEG/WebP/GIF/HEIC/rotated-JPEG/blank/French/non-image
      files, and end to end through `desktop.vision` in `apps/playground`
      (see research/MACOS.md).
- [~] Windows: `Windows.Media.Ocr` via the `windows` crate is written and
  compile-checked (`cargo check`/`clippy --target x86_64-pc-windows-msvc`
  on the module alone — chain-core's bundled SQLite can't cross-compile
  from a Mac), but never run. Checklist in research/WINDOWS.md.
- [x] `recognizeDocument` (request 23), macOS 26.6: `cargo test -p
    chain-core vision` runs the real recognizer on a fixture page
      (title, paragraph, 4×4 table with a merged header, bullet list), and
      end to end through `desktop.vision` in the playground, including
      `languages: ["en"]`, `INVALID_ARGUMENT` and `UNSUPPORTED` language.
- [x] mneme, real PDF render (Chrome print-to-PDF at 2×): a ruled 4×3
      table with normal padding came back exactly; with zero padding
      cells merged across borders — fixed by `reassign_misplaced_words`
      (fixture test `keeps_cells_apart_when_columns_are_tight`).
- [ ] Borderless tables and row spans — not tried yet. Windows: not implemented (research/WINDOWS.md).
- [ ] Real-world photos (textbook pages, handwriting) — only rendered
      text has been tested.
- [ ] Contract tests under `capabilities/vision/tests/`.
