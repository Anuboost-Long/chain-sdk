/**
 * Structural contract for the Vision capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface RecognizeTextOptions {
  /** BCP-47 tags, most likely first. Omitted: the language is detected. */
  languages?: string[];
  /** Slower, better recognition. Defaults to true. */
  accurate?: boolean;
}

/** Normalized to the image (0–1), origin at the top-left. */
export interface TextBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RecognizedLine {
  text: string;
  /** 0–1. */
  confidence: number;
  box: TextBox;
}

export interface RecognizedText {
  /** Every line in reading order, joined with "\n". */
  text: string;
  lines: RecognizedLine[];
}

export interface RecognizeDocumentOptions {
  /** BCP-47 tags, most likely first. Omitted: the language is detected. */
  languages?: string[];
}

export interface DocumentParagraph {
  text: string;
  box: TextBox;
}

/** Spans are omitted when 1. A spanning cell appears once, in the row and
 * column it starts at, and is left out of the slots it covers — like HTML. */
export interface TableCell {
  text: string;
  box: TextBox;
  rowSpan?: number;
  colSpan?: number;
}

export interface RecognizedTable {
  box: TextBox;
  /** Top to bottom; each row's cells left to right. */
  rows: TableCell[][];
}

export interface RecognizedList {
  /** Item text without its bullet or number. */
  items: string[];
  box: TextBox;
}

export interface RecognizedDocument {
  /** Text outside tables and lists, in reading order. */
  paragraphs: DocumentParagraph[];
  tables: RecognizedTable[];
  lists: RecognizedList[];
}

export interface VisionApi {
  recognizeText(image: Uint8Array, options?: RecognizeTextOptions): Promise<RecognizedText>;
  /** Paragraphs, tables and lists in a picture of a page. */
  recognizeDocument(
    image: Uint8Array,
    options?: RecognizeDocumentOptions
  ): Promise<RecognizedDocument>;
  /** BCP-47 tags `recognizeText` accepts on this machine. */
  languages(): Promise<string[]>;
}
