# Vision — macOS research

## API

- `VNRecognizeTextRequest` (macOS 10.15+), `recognitionLevel` accurate/fast,
  `usesLanguageCorrection` (on with accurate), `recognitionLanguages`,
  `automaticallyDetectsLanguage` (macOS 13+). Results are
  `VNRecognizedTextObservation`s; `topCandidates(1)` gives the string and
  its 0–1 confidence; `boundingBox` is normalized with a **bottom-left**
  origin, so `y = 1 - y - height` for the contract's top-left origin.
- `supportedRecognitionLanguagesAndReturnError` depends on the request's
  level: 30 tags for accurate on macOS 26.6 (`en-US, fr-FR, it-IT, de-DE,
es-ES, pt-BR, zh-Hans, zh-Hant, …`).
- `VNImageRequestHandler initWithData:options:` decodes via ImageIO and
  honors EXIF orientation. ImageIO reads PNG, JPEG, WebP, GIF (frame 0),
  HEIC.
- macOS 26's `RecognizeDocumentsRequest` (paragraphs/tables) is Swift-only
  (Swift concurrency API), so unreachable from objc2. Not needed yet.
- No TCC permission: Vision is fully local.

## Verification run (2026-09-28, macOS 26.6.2, Apple Silicon)

Rendered a 1200×500 slide (three lines, Helvetica 44 pt) as PNG, JPEG,
WebP, GIF and HEIC, plus a 90°-rotated JPEG, a blank PNG, a French line
and a non-image file:

- All five formats: exact text, confidence 1.0, ~220 ms each (accurate);
  first line box `y = 0.124` for text drawn at pixel 60 of 500 — top-left
  origin confirmed. Fast mode: 39 ms, same text.
- Rotated JPEG: all three lines read correctly.
- Blank: `{ text: "", lines: [] }`. Non-image: `InvalidImage`.
- `languages: ["fr"]` → resolved to `fr-FR`, text exact. `["tlh"]` →
  `Unsupported` naming `"tlh"`.
- End to end in `apps/playground` (`tauri dev`, via the dev inspector):
  `desktop.vision.languages()` → 30 tags; `recognizeText` on a fetched
  JPEG exact; `INVALID_ARGUMENT` and `UNSUPPORTED` arrive as `ChainError`
  codes.

## Document structure — `RecognizeDocumentsRequest` (request 23)

Swift-only (Vision's Swift API, macOS 26); no `VN…` class, so objc2
can't reach it. `swift/ChainVision.swift` runs it and returns JSON; Rust
flips boxes and shapes the result. Read from the SDK's
`Vision.swiftinterface`: `DocumentObservation.document` has `title`,
`paragraphs`, `tables`, `lists`, `barcodes`, `text`; a table's `rows` is
`[[Cell]]` with each cell's `rowRange`/`columnRange` and `content`
(a container, whose `text.transcript` is the cell text).

Probed 2026-09-29 on a drawn page (`crates/core/tests/fixtures/
document-with-table.png`):

- A merged cell ("Sales" over two columns) appears **once**, in its
  starting row, with `columnRange 1...2`, and is absent from the slot it
  covers — HTML's model, passed straight through as `colSpan`. Row spans
  untested; `shape()` dedupes by range in case a spanning cell is listed
  in each row it covers.
- `paragraphs` also contains every table cell and list item as its own
  paragraph (and the title). `shape()` drops paragraphs whose center is
  inside a table or list box.
- Lists give `markerString` ("•") and `itemString` separately; items are
  reported without the marker.
- `boundingRegion.boundingBox` is normalized, bottom-left origin.
- Running it needs the Swift concurrency runtime at `/usr/lib/swift`;
  chain-core's own test binary gets that rpath from build.rs (apps
  already add it in their build.rs).
- **Tight columns** (reported by mneme on a Chrome print-to-PDF table with
  zero cell padding; reproduced in `tests/fixtures/table-tight-columns.png`):
  the grid (cell boxes) is right, but a text line running across a cell
  border lands whole in one cell — `["Monday", "Requirements Quiz 1", ""]`.
  Identical at 1×–4× render scale; the request has no table options
  (only text-recognition ones). Each cell's `text.words` keep true boxes,
  so `reassign_misplaced_words()` rebuilds a table's cell text from word
  positions whenever any word's center is outside its own cell (nearest
  cell for a word on a border; a line break when a word sits below the
  previous one). Tables where every word is inside its cell keep Vision's
  transcripts untouched.
