# PDF — Semantic Contract

`desktop.pdf` turns an HTML document into a real paginated PDF file,
laid out by the same web engine the app already runs in. Structural
contract: `capabilities/pdf/contract.ts`.

Requested by mneme (request 41, for "Share as PDF"): JS PDF libraries
can't shape complex scripts or render the app's HTML, canvas capture
makes pictures of text, and `window.print()` prints the app's own
window through a dialog.

## `render(html, options?)`

```ts
render(html: string, options?: RenderPdfOptions): Promise<RenderedPdf>
// RenderedPdf = { reference: string; pageCount: number; size: number }
```

Renders `html` (a whole document, `<!doctype html>` and all) in a hidden
webview of its own — it never appears in, or disturbs, the app window —
and writes the PDF into the app's files. Resolves with a
`desktop.files` reference (read it, share it with `desktop.share`, save
it with `files.read` + `files.save`, delete it when done), the page
count and the size in bytes.

- **Loading.** Relative URLs resolve against `baseUrl`, by default the
  calling page's URL, so the app's own bundled CSS and fonts load as they
  do in the app. `desktop.files.url()` URLs (the asset protocol) load
  too. The document's own scripts don't run unless `runScripts: true`.
- **Waiting.** After the document loads, `<img>` elements (lazy ones
  included), stylesheets and web fonts are waited for. If they haven't
  all finished within `timeout` (default 15000 ms) it rejects `TIMEOUT`;
  if any failed it rejects `NOT_FOUND`, naming them. A half-rendered
  PDF is never written. CSS background images aren't checked.
- **Pages.** Real pages at `paper` size (default: the system's default
  paper, A4 or Letter by region — `availability().defaultPaper`), in
  `orientation` (default portrait), inside `margins` (millimetres,
  default 15 on every side). `break-before`/`break-after: page` and
  `break-inside`/`break-after: avoid` apply, as do `@page { margin }`
  rules, which win over `margins`. 1 CSS px prints as 0.75 pt (1/96
  inch), as CSS defines it, so `mm`/`cm`/`in` lengths print true to
  size.
- **Backgrounds** colours and images print (`printBackground`, default
  true). With false, only what the document itself asks for with
  `print-color-adjust: exact` prints.
- **Colour scheme.** The document renders with `prefers-color-scheme:
  light` (`colorScheme`, default `"light"`) whatever the app's theme.
- **Header and footer.** `header`/`footer` take `left`/`center`/`right`
  text, drawn in every page's top/bottom margin, centred in it, in small
  grey system type. `{page}` becomes the page number, `{pages}` the
  page count: `{ right: "Page {page} of {pages}" }`. Margins that are
  too small for 9 pt text, or a CSS `@page` margin that moves the
  content into them, make the text overlap the content.
- **Text stays text**: selectable, fonts embedded, web links clickable.
  See "Complex scripts" below for what copying and searching give.
- **Metadata**: `title` (default: the document's `<title>`), `author`,
  `creator` (default: the app's name).
- One render at a time per app; further calls wait their turn.

### Complex scripts

On macOS the PDF writer only records which characters a glyph stands for
when the font maps that glyph straight to one character. Text in scripts
whose letters join, stack or reorder — Khmer, Arabic, Devanagari, Thai
— **looks right and selects**, but copying or searching it gives wrong
characters. Latin, Cyrillic, Greek, Chinese, Japanese and Korean copy
and search correctly. `availability().complexScriptSearch` reports this
(false on macOS). It's the OS's PDF writer, not the engine or font: text
CoreText draws straight into a PDF behaves the same (see
`research/MACOS.md`).

## `availability()`

Never rejects. Says whether `render()` works here, the default paper,
and which options and CSS features apply. Options that don't apply are
accepted and ignored. On macOS: everything true except `cssPageSize`
(`@page { size }` is ignored; `paper` sets the size),
`cssPageMarginBoxes` (no `@top-center { content: counter(page) }`; use
`header`/`footer`) and `complexScriptSearch`.

## Errors

| Code | When |
| --- | --- |
| `INVALID_ARGUMENT` | unknown option or value, non-positive paper size, negative margins or margins that leave no room, `timeout` not in (0, 600000], `baseUrl` not an absolute URL |
| `TIMEOUT` | images, stylesheets or fonts still loading after `timeout` |
| `NOT_FOUND` | an image, stylesheet or font failed to load (the message lists them) |
| `UNSUPPORTED` | outside a Chain app, or a platform without an implementation (Windows and Linux today) |
| `NATIVE_FAILURE` | the engine couldn't print, or the file couldn't be written |

Nothing is left on disk after any error.

## Non-goals

- A print dialog, or printing to a printer.
- Reading, editing or merging existing PDFs.
- Rendering a live page of the app (only an HTML string).
- Styling the header/footer text (font, size, colour) or different
  first-page headers — put those in the document itself.
- Returning bytes directly: `files.read(reference)` gives them.
