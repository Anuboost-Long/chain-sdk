# Vision Capability — Contract

## What this is

On-device text recognition for an image the app already holds as bytes:
screenshots of slides, textbook photos, pasted images. It returns the
exact text, not a model's description of it. Requested by mneme (request
20, Phase 15 "OCR"). Uses the OS's own offline engine, so no network,
no download, and no permission prompt.

## `desktop.vision.recognizeText(image, options?)`

```
recognizeText(
  image: Uint8Array,
  options?: { languages?: string[]; accurate?: boolean },
): Promise<{ text: string; lines: { text; confidence; box }[] }>
```

- `image` — encoded image bytes: PNG, JPEG, WebP, GIF (first frame only)
  or HEIC. A photo's EXIF orientation is honored. Bytes, not a
  `desktop.files` reference: images are small (apps cap them around
  10 MB) and many never get stored.
- `languages` — BCP-47 tags in priority order (`"en-US"`, `"fr"`,
  `"zh-Hans"`). A bare language (`"fr"`) matches that language's
  supported variant (`"fr-FR"`). Omitted or empty: the language is
  detected automatically.
- `accurate` — `true` (the default) for the slower, better recognizer
  with language correction; `false` for the fast one.

Resolves with:

- `lines` — one entry per recognized line, in the recognizer's reading
  order (top to bottom for ordinary text; multi-column layouts follow
  whatever order the OS engine produces). `confidence` is
  0–1. `box` is normalized to the image's size (0–1) with a **top-left**
  origin, like CSS — not the bottom-left origin the native API uses.
- `text` — every line's `text` joined with `"\n"`.

An image with no text resolves `{ text: "", lines: [] }`, not an error.

## `desktop.vision.recognizeDocument(image, options?)`

Mneme request 23. Reads the structure of a picture of a page (a rendered
PDF page, a scan, a photo) and resolves

```ts
{
  paragraphs: { text, box }[],            // outside tables and lists, reading order
  tables: { box, rows: { text, box, rowSpan?, colSpan? }[][] }[],
  lists: { items: string[], box }[]       // items without their bullet/number
}
```

- `box` is `recognizeText`'s: normalized 0–1, top-left origin.
- **Tables follow HTML's model.** `rows` runs top to bottom, each row's
  cells left to right. A merged cell appears once, in the row and column
  it starts at, with `rowSpan`/`colSpan` (omitted when 1), and is left
  out of the slots it covers — so rows can have different lengths, and
  an app lays them out the way a browser lays out `<td rowspan>`.
- A cell's text is what lies inside that cell, even when columns are
  only a few pixels apart (Vision reads such lines across the border;
  Chain puts each word back in the cell it sits in).
- Text in a table or list is reported there only, never again as a
  paragraph. A list's lead-in line ("Key points:") is a paragraph.
- `options.languages` — BCP-47 tags, resolved as for `recognizeText`;
  omitted, the language is detected. No `accurate` switch: document
  recognition has one level.
- A page with no text resolves three empty lists.

**macOS 26 or later only.** Earlier macOS and Windows reject
`UNSUPPORTED`; apps fall back to `recognizeText`. Pass `image` as it is —
render PDF pages at about 2× so small table text stays legible.

## `desktop.vision.languages()`

Resolves the BCP-47 tags `recognizeText` accepts on this machine for the
accurate recognizer (the fast one supports a subset). Order carries no
meaning.

## Errors (`ChainErrorCode`)

- `INVALID_ARGUMENT` — the bytes aren't an image the OS can decode
  (including empty input).
- `UNSUPPORTED` — a requested language isn't available (the message
  names it), or the platform has no implementation (Linux today; for
  `recognizeDocument`, also Windows and macOS before 26), or the call
  ran outside a Chain app.
- `NATIVE_FAILURE` — the recognizer itself failed; the message is the
  OS's.

## Windows differences (implemented, not yet run)

Windows OCR reports no confidence (every line is `1.0`), has one engine
(`accurate` is ignored), uses only the first requested language (all must
be installed), rejects images larger than its maximum dimension with
`INVALID_ARGUMENT`, and reads HEIC/WebP only with the Store codec
extensions. See `research/WINDOWS.md`.

## Non-goals

- Not a general image-understanding API: no object/face/barcode
  detection, no captions. Each would be its own method when an app needs
  it.
- `recognizeDocument` doesn't return images, barcodes, detected data
  (dates, links) or per-word boxes, and doesn't convert to Markdown or
  HTML — the app renders the structure. Pictures in a PDF come from the
  app's own renderer, not from here.
- No handwriting-specific mode, no custom word lists, no region-of-
  interest cropping — the app can crop before calling.
- Not a reference-based API. If an app ever needs to OCR stored images
  in bulk, add a reference overload then.
