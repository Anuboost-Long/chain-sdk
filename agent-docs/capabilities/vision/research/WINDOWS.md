# Vision — Windows research

**Status: implemented, compile-checked, never run.** `crates/core/src/vision.rs`'s
`windows_ocr` module type-checks (`cargo check` + `clippy`) against
`x86_64-pc-windows-msvc`, but no one has executed it on Windows yet.

## How it's built

- WinRT `Windows.Media.Ocr.OcrEngine` through the `windows` crate 0.61 (the
  version Tauri already pulls in). No .NET — the same WinRT API a .NET app
  would call, without shipping a runtime.
- Bytes → `InMemoryRandomAccessStream` (via `DataWriter`) →
  `BitmapDecoder::CreateAsync` → `GetSoftwareBitmapAsync`. A decode failure
  is `INVALID_ARGUMENT`.
- `RoInitialize(RO_INIT_MULTITHREADED)` on each call: the command runs on a
  pooled blocking thread.
- Language: `OcrEngine` takes one language, so every requested tag must be
  in `AvailableRecognizerLanguages` (else `UNSUPPORTED`) and the first is
  used. None requested → `TryCreateFromUserProfileLanguages`.
- A line's box is the union of its words' pixel `BoundingRect`s, divided by
  the bitmap size (already top-left origin).

## Differences from macOS the contract has to allow

- **No confidence**: Windows OCR doesn't report one; every line is `1.0`.
- **`accurate` is ignored**: there's one engine.
- **One language per call**, not a priority list.
- **Size limit**: images over `OcrEngine::MaxImageDimension` on a side are
  rejected `INVALID_ARGUMENT` rather than scaled.
- **Formats**: PNG/JPEG/GIF/BMP/TIFF are built in. HEIC and WebP need the
  Store codec extensions and fail `INVALID_ARGUMENT` without them.
- Languages depend on installed OCR language packs (Settings → Time &
  language → Language → "Optical character recognition").

## Verification checklist (on a real Windows machine)

1. `cargo test -p chain-core vision` (the non-image test is macOS-only today —
   extend it to Windows).
2. In a Chain app: `recognizeText` on a PNG/JPEG slide → exact text, boxes
   match positions; blank image → empty; `languages()` lists the installed packs.
3. WebP/HEIC with and without the codec extensions.
4. A requested language that isn't installed → `UNSUPPORTED`.
5. Then flip `component.json`'s `windows` to `experimental`.

## Document structure (`recognizeDocument`) — not implemented

`Windows.Media.Ocr` returns lines and words only; there's no table or
list detection in WinRT. Options, none researched yet: the Windows App
SDK's AI text recognition (Copilot+ PCs; check whether it reports any
structure), or inferring a grid from word boxes (fragile — the reason
mneme asked for native structure). Until then it rejects
`UNSUPPORTED` and apps fall back to `recognizeText`.
