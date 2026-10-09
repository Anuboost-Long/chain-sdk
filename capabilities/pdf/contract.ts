/**
 * Structural contract for the PDF capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** A paper size in millimetres, portrait: width is the shorter side. */
export interface PaperDimensions {
  width: number;
  height: number;
}

export type PaperSize = "a4" | "letter" | PaperDimensions;

export type PageOrientation = "portrait" | "landscape";

/** In millimetres. */
export interface PageMargins {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/**
 * Text drawn in a page's top or bottom margin. `{page}` becomes the page
 * number and `{pages}` the page count, e.g. "Page {page} of {pages}".
 */
export interface PageMarginText {
  left?: string;
  center?: string;
  right?: string;
}

export interface RenderPdfOptions {
  /** Defaults to the system's default paper size, which follows the region: A4, or Letter in the US and Canada. */
  paper?: PaperSize;
  /** Defaults to "portrait". */
  orientation?: PageOrientation;
  /** Millimetres; one number for all four sides. Defaults to 15 on every side. A CSS `@page { margin }` in the document wins. */
  margins?: number | Partial<PageMargins>;
  /** Print background colours and images. Defaults to true. */
  printBackground?: boolean;
  /** The `prefers-color-scheme` the document is rendered with, whatever the app's theme. Defaults to "light". */
  colorScheme?: "light" | "dark";
  /** Drawn in every page's top margin. */
  header?: PageMarginText;
  /** Drawn in every page's bottom margin. */
  footer?: PageMarginText;
  /** The PDF's title metadata. Defaults to the document's `<title>`, if any. */
  title?: string;
  /** The PDF's author metadata. */
  author?: string;
  /** The PDF's creator metadata, the app that made it. Defaults to the app's name. */
  creator?: string;
  /** What relative URLs in the document resolve against. Defaults to the calling page's URL, so the app's own CSS, fonts and images load. */
  baseUrl?: string;
  /** Run the document's own scripts. Defaults to false. */
  runScripts?: boolean;
  /** Milliseconds to wait for the document, its images, stylesheets and fonts. Defaults to 15000. */
  timeout?: number;
}

export interface RenderedPdf {
  /** A `desktop.files` reference: read, share, save, open or delete it like any other. */
  reference: string;
  pageCount: number;
  /** In bytes. */
  size: number;
}

/** Which options work here. Options that don't are accepted and have no effect. */
export interface PdfAvailability {
  /** Whether render() works here at all. Every flag below is false when this is. */
  available: boolean;
  /** What `paper` defaults to here, in millimetres. */
  defaultPaper: PaperDimensions;
  /** `paper`, `orientation` and `margins`. */
  pageSetup: boolean;
  /** `@page` margins, and `break-before`/`break-after`/`break-inside` (`page`, `avoid`). */
  cssPageBreaks: boolean;
  /** CSS `@page { size }`. Where false, `paper` alone sets the size. */
  cssPageSize: boolean;
  /** `@page` margin boxes with `counter(page)`/`counter(pages)`. Where false, use `header`/`footer`. */
  cssPageMarginBoxes: boolean;
  header: boolean;
  footer: boolean;
  printBackground: boolean;
  colorScheme: boolean;
  /** `title`, `author`, `creator`. */
  metadata: boolean;
  runScripts: boolean;
  /** Text is real text with its fonts embedded: selectable, and searchable for scripts that map letter by letter (Latin, Cyrillic, Greek, Chinese, Japanese, Korean, ...). */
  selectableText: boolean;
  /**
   * Text in scripts whose letters join or reorder (Khmer, Arabic, Devanagari, Thai, ...)
   * also copies and searches as the same text. Where false it still looks right and selects,
   * but copying or searching it gives wrong characters.
   */
  complexScriptSearch: boolean;
  /** Web links stay clickable. */
  links: boolean;
}

export interface PdfApi {
  availability(): Promise<PdfAvailability>;
  /** Renders an HTML document off-screen into a paginated PDF in the app's files. */
  render(html: string, options?: RenderPdfOptions): Promise<RenderedPdf>;
}
