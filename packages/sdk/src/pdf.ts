import { invoke, isTauri } from "./native";

import type { PdfApi, PdfAvailability, RenderPdfOptions, RenderedPdf } from "./contracts/pdf";
import { chainError, type ChainErrorCode } from "./errors";

const NOTHING_AVAILABLE: PdfAvailability = {
  available: false,
  defaultPaper: { width: 210, height: 297 },
  pageSetup: false,
  cssPageBreaks: false,
  cssPageSize: false,
  cssPageMarginBoxes: false,
  header: false,
  footer: false,
  printBackground: false,
  colorScheme: false,
  metadata: false,
  runScripts: false,
  selectableText: false,
  complexScriptSearch: false,
  links: false
};

// Native errors arrive as "CODE: message" — see templates/pdf.rs.
const CODES: ChainErrorCode[] = ["INVALID_ARGUMENT", "TIMEOUT", "NOT_FOUND", "UNSUPPORTED"];

export const pdf: PdfApi = {
  async availability(): Promise<PdfAvailability> {
    if (!isTauri()) return NOTHING_AVAILABLE;
    return invoke<PdfAvailability>("pdf_availability");
  },

  async render(html: string, options?: RenderPdfOptions): Promise<RenderedPdf> {
    if (!isTauri()) {
      throw chainError(
        "UNSUPPORTED",
        "desktop.pdf.render() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
      );
    }
    try {
      return await invoke<RenderedPdf>("pdf_render", { html, options });
    } catch (error) {
      const message = typeof error === "string" ? error : "rendering the PDF failed";
      const code = CODES.find((candidate) => message.startsWith(`${candidate}: `));
      if (code) throw chainError(code, message.slice(code.length + 2));
      throw chainError("NATIVE_FAILURE", message);
    }
  }
};
