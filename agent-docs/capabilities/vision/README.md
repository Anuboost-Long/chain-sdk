# Vision capability

## How it works

`desktop.vision.recognizeText(bytes, options)` sends the image as the raw
IPC body (options JSON in a `chain-vision-options` header) to
`vision_recognize_text` in `templates/lib.rs`, which runs
`chain_core::vision::recognize_text` on a blocking thread. On macOS that
checks the bytes decode with ImageIO (`CGImageSource`) — so "not an
image" is `INVALID_ARGUMENT`, not a recognizer failure — then runs
Vision's `VNRecognizeTextRequest` through `VNImageRequestHandler
initWithData:` (which honors EXIF orientation), and converts each
observation's top candidate and bottom-left-origin box into a line with a
top-left-origin box. On Windows it decodes with `BitmapDecoder` and runs
WinRT's `OcrEngine` (written, not yet run — see research/WINDOWS.md).
Linux returns `UNSUPPORTED`.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const bytes = new Uint8Array(await (await fetch(imageUrl)).arrayBuffer());
const { text, lines } = await desktop.vision.recognizeText(bytes);
const lowConfidence = lines.filter((line) => line.confidence < 0.5);

await desktop.vision.recognizeText(bytes, { languages: ["fr"], accurate: false });
await desktop.vision.languages(); // ["en-US", "fr-FR", ...]

// macOS 26+: tables and lists, e.g. from a PDF page rendered to PNG.
const { paragraphs, tables, lists } = await desktop.vision.recognizeDocument(pagePng);
for (const table of tables) {
  for (const row of table.rows) {
    // row cells in order; a merged cell carries rowSpan/colSpan, like <td>
  }
}
```

## Files to check

- `capabilities/vision/contract.ts` — the TS types.
- `crates/core/src/vision.rs` — language resolution, ImageIO check, Vision call, box conversion; `recognize_document` + `shape()` (box flip, paragraph filtering, spans); unit tests, one against `crates/core/tests/fixtures/document-with-table.png`.
- `crates/core/swift/ChainVision.swift` — the `RecognizeDocumentsRequest` bridge (Swift-only API); built by `crates/core/build.rs` with ChainSpeech.swift into one library.
- `packages/cli/templates/lib.rs` — `vision_recognize_text`/`vision_recognize_document` (raw body + header, shared `vision_input`) and `vision_languages`.
- `packages/sdk/src/vision.ts` — SDK wrapper and error-prefix mapping.
- `research/MACOS.md` — API choices and the verification run.
