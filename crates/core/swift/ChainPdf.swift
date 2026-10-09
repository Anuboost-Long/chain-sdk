// The pdf capability's renderer for crates/core/src/pdf.rs — see
// agent-docs/capabilities/pdf/research/MACOS.md.
//
// The WKWebView is a hidden Tauri webview (templates/pdf.rs), so the
// document loads the app's own asset-protocol URLs, CSS and fonts. This
// loads the HTML, waits for its images, stylesheets and fonts, prints it
// with WebKit's paginating print path to a PDF beside the output, then
// redraws that into the output adding the header, footer and metadata.

import AppKit
import CoreText
import WebKit

public typealias ChainPdfFinish = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

// Keep in sync with pdf.rs's `native` module.
private let statusOk: Int32 = 0
private let statusTimedOut: Int32 = 1
private let statusResourceFailed: Int32 = 2
private let statusFailed: Int32 = 3

private struct MarginText: Decodable {
    var left: String?
    var center: String?
    var right: String?
}

private struct Margins: Decodable {
    var top: Double
    var right: Double
    var bottom: Double
    var left: Double
}

/// Lengths in points, the paper already turned for its orientation.
private struct Request: Decodable {
    var html: String
    var baseUrl: String?
    var outPath: String
    var width: Double
    var height: Double
    var landscape: Bool
    var margins: Margins
    var printBackground: Bool
    var dark: Bool
    var header: MarginText?
    var footer: MarginText?
    var title: String?
    var author: String?
    var creator: String?
    var timeoutMs: Double
}

private struct Readiness: Decodable {
    var failed: [String]
    var title: String
}

/// WebKit prints 1 CSS px as 0.8 pt; CSS (and Chromium) say 0.75.
private let cssPixelZoom = 0.75 / 0.8

// Runs in its own content world, so it works with the page's scripts off.
private let readinessScript = """
const style = document.createElement("style");
style.textContent = `:root { zoom: ${zoom}; }` +
  (printBackground ? "*, *::before, *::after { -webkit-print-color-adjust: exact !important; print-color-adjust: exact !important; }" : "");
document.head.prepend(style);
if (document.readyState !== "complete") {
  await new Promise((resolve) => addEventListener("load", resolve, { once: true }));
}
const images = [...document.images];
images.forEach((image) => { image.loading = "eager"; });
await Promise.all(images.map((image) => image.complete ? null : new Promise((resolve) => {
  image.addEventListener("load", resolve, { once: true });
  image.addEventListener("error", resolve, { once: true });
})));
await document.fonts.ready;
const failed = [
  ...images.filter((image) => image.getAttribute("src") && image.naturalWidth === 0).map((image) => image.currentSrc || image.src),
  ...[...document.querySelectorAll('link[rel~="stylesheet"]')].filter((link) => !link.sheet).map((link) => link.href),
  ...[...document.fonts].filter((font) => font.status === "error").map((font) => `the font ${font.family}`),
];
return JSON.stringify({ failed, title: document.title });
"""

private final class PdfRender: NSObject {
    private static var running: Set<PdfRender> = []

    private let webView: WKWebView
    private let request: Request
    private let context: UnsafeMutableRawPointer?
    private let onFinish: ChainPdfFinish
    private let printedURL: URL
    private var loading: NSKeyValueObservation?
    private var finished = false
    private var printing = false
    private var documentTitle = ""

    init(webView: WKWebView, request: Request, context: UnsafeMutableRawPointer?, onFinish: @escaping ChainPdfFinish) {
        self.webView = webView
        self.request = request
        self.context = context
        self.onFinish = onFinish
        printedURL = URL(fileURLWithPath: request.outPath + ".print")
    }

    func start() {
        PdfRender.running.insert(self)
        webView.appearance = NSAppearance(named: request.dark ? .darkAqua : .aqua)
        DispatchQueue.main.asyncAfter(deadline: .now() + request.timeoutMs / 1000) { [weak self] in
            guard let self, !self.finished, !self.printing else { return }
            let seconds = (self.request.timeoutMs / 1000).formatted()
            self.finish(statusTimedOut, "the document's images, stylesheets and fonts didn't load within \(seconds) s")
        }
        var started = false
        loading = webView.observe(\.isLoading, options: [.new]) { [weak self] webView, _ in
            if webView.isLoading {
                started = true
            } else if started {
                self?.loaded()
            }
        }
        webView.loadHTMLString(request.html, baseURL: request.baseUrl.flatMap(URL.init(string:)))
    }

    private func loaded() {
        loading = nil
        let arguments: [String: Any] = ["zoom": cssPixelZoom, "printBackground": request.printBackground]
        webView.callAsyncJavaScript(readinessScript, arguments: arguments, in: nil, in: .defaultClient) { [weak self] result in
            guard let self, !self.finished else { return }
            switch result {
            case .failure(let error):
                self.finish(statusFailed, "couldn't check the document had loaded: \(error.localizedDescription)")
            case .success(let value):
                guard let json = value as? String,
                      let readiness = try? JSONDecoder().decode(Readiness.self, from: Data(json.utf8))
                else {
                    self.finish(statusFailed, "couldn't check the document had loaded")
                    return
                }
                if !readiness.failed.isEmpty {
                    self.finish(statusResourceFailed, "couldn't load \(readiness.failed.joined(separator: ", "))")
                    return
                }
                self.documentTitle = readiness.title
                self.print()
            }
        }
    }

    private func print() {
        printing = true
        guard let window = webView.window else {
            finish(statusFailed, "the render webview has no window")
            return
        }
        let info = NSPrintInfo(dictionary: [
            .jobDisposition: NSPrintInfo.JobDisposition.save,
            .jobSavingURL: printedURL,
        ])
        info.paperSize = NSSize(width: request.width, height: request.height)
        info.orientation = request.landscape ? .landscape : .portrait
        info.topMargin = request.margins.top
        info.rightMargin = request.margins.right
        info.bottomMargin = request.margins.bottom
        info.leftMargin = request.margins.left
        info.horizontalPagination = .automatic
        info.verticalPagination = .automatic
        info.isHorizontallyCentered = false
        info.isVerticallyCentered = false
        let operation = webView.printOperation(with: info)
        operation.showsPrintPanel = false
        operation.showsProgressPanel = false
        // Without a frame WebKit prints blank pages.
        operation.view?.frame = webView.bounds
        operation.runModal(for: window, delegate: self, didRun: #selector(printed(_:success:contextInfo:)), contextInfo: nil)
    }

    @objc private func printed(_ operation: NSPrintOperation, success: Bool, contextInfo: UnsafeMutableRawPointer?) {
        guard success else {
            finish(statusFailed, "WebKit couldn't print the document")
            return
        }
        do {
            let pageCount = try finishDocument()
            let size = try FileManager.default.attributesOfItem(atPath: request.outPath)[.size] as? Int ?? 0
            finish(statusOk, #"{"pageCount":\#(pageCount),"size":\#(size)}"#)
        } catch {
            finish(statusFailed, "\(error)")
        }
    }

    /// Redraws the printed pages into the output, adding the header,
    /// footer, metadata and the links redrawing drops.
    private func finishDocument() throws -> Int {
        guard let printed = CGPDFDocument(printedURL as CFURL), printed.numberOfPages > 0 else {
            throw PdfError("WebKit printed no pages")
        }
        var info: [CFString: Any] = [:]
        let title = request.title ?? documentTitle
        if !title.isEmpty { info[kCGPDFContextTitle] = title }
        if let author = request.author { info[kCGPDFContextAuthor] = author }
        if let creator = request.creator { info[kCGPDFContextCreator] = creator }
        var firstBox = printed.page(at: 1)!.getBoxRect(.mediaBox)
        guard let output = CGContext(URL(fileURLWithPath: request.outPath) as CFURL, mediaBox: &firstBox, info as CFDictionary) else {
            throw PdfError("couldn't create the PDF file")
        }
        let pageCount = printed.numberOfPages
        for number in 1...pageCount {
            guard let page = printed.page(at: number) else { continue }
            var box = page.getBoxRect(.mediaBox)
            output.beginPDFPage([kCGPDFContextMediaBox: Data(bytes: &box, count: MemoryLayout<CGRect>.size)] as CFDictionary)
            output.drawPDFPage(page)
            for (rect, url) in links(on: page) {
                output.setURL(url as CFURL, for: rect)
            }
            let substitute = { (text: String) in
                text.replacingOccurrences(of: "{page}", with: "\(number)").replacingOccurrences(of: "{pages}", with: "\(pageCount)")
            }
            if let header = request.header {
                drawMarginText(header, in: output, box: box, centerY: box.maxY - request.margins.top / 2, substitute)
            }
            if let footer = request.footer {
                drawMarginText(footer, in: output, box: box, centerY: box.minY + request.margins.bottom / 2, substitute)
            }
            output.endPDFPage()
        }
        output.closePDF()
        return pageCount
    }

    private func drawMarginText(_ text: MarginText, in context: CGContext, box: CGRect, centerY: Double, _ substitute: (String) -> String) {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 9),
            .foregroundColor: NSColor(white: 0.4, alpha: 1).cgColor,
        ]
        let placements: [(String?, (Double) -> Double)] = [
            (text.left, { _ in box.minX + self.request.margins.left }),
            (text.center, { width in box.midX - width / 2 }),
            (text.right, { width in box.maxX - self.request.margins.right - width }),
        ]
        for case let (string?, x) in placements where !string.isEmpty {
            let line = CTLineCreateWithAttributedString(NSAttributedString(string: substitute(string), attributes: attributes))
            var ascent: CGFloat = 0
            var descent: CGFloat = 0
            let width = CTLineGetTypographicBounds(line, &ascent, &descent, nil)
            context.textPosition = CGPoint(x: x(width), y: centerY - (ascent - descent) / 2)
            CTLineDraw(line, context)
        }
    }

    private func finish(_ status: Int32, _ payload: String) {
        guard !finished else { return }
        finished = true
        loading = nil
        try? FileManager.default.removeItem(at: printedURL)
        if status != statusOk {
            try? FileManager.default.removeItem(atPath: request.outPath)
        }
        payload.withCString { onFinish(context, status, $0) }
        PdfRender.running.remove(self)
    }
}

private struct PdfError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

/// The page's web links, which drawing a PDF page into another drops.
private func links(on page: CGPDFPage) -> [(CGRect, URL)] {
    var annotations: CGPDFArrayRef?
    guard let dictionary = page.dictionary, CGPDFDictionaryGetArray(dictionary, "Annots", &annotations), let annotations else {
        return []
    }
    return (0..<CGPDFArrayGetCount(annotations)).compactMap { index in
        var annotation: CGPDFDictionaryRef?
        var subtype: UnsafePointer<CChar>?
        var corners: CGPDFArrayRef?
        var action: CGPDFDictionaryRef?
        var uri: CGPDFStringRef?
        guard CGPDFArrayGetDictionary(annotations, index, &annotation), let annotation,
              CGPDFDictionaryGetName(annotation, "Subtype", &subtype), let subtype, String(cString: subtype) == "Link",
              CGPDFDictionaryGetArray(annotation, "Rect", &corners), let corners, CGPDFArrayGetCount(corners) == 4,
              CGPDFDictionaryGetDictionary(annotation, "A", &action), let action,
              CGPDFDictionaryGetString(action, "URI", &uri), let uri,
              let string = CGPDFStringCopyTextString(uri) as String?, let url = URL(string: string)
        else { return nil }
        var values = [CGPDFReal](repeating: 0, count: 4)
        for i in 0..<4 { CGPDFArrayGetNumber(corners, i, &values[i]) }
        let rect = CGRect(x: values[0], y: values[1], width: values[2] - values[0], height: values[3] - values[1]).standardized
        return (rect, url)
    }
}

/// Renders the request (JSON, see `Request`) in `webview`, a WKWebView in
/// a hidden window. `onFinish` gets a status and, for statusOk,
/// `{"pageCount":n,"size":bytes}`, else the error message. Main thread.
@_cdecl("chain_pdf_render")
public func chainPdfRender(
    _ webview: UnsafeMutableRawPointer,
    _ requestJSON: UnsafePointer<CChar>,
    _ context: UnsafeMutableRawPointer?,
    _ onFinish: ChainPdfFinish
) {
    let webView = Unmanaged<WKWebView>.fromOpaque(webview).takeUnretainedValue()
    do {
        let request = try JSONDecoder().decode(Request.self, from: Data(String(cString: requestJSON).utf8))
        PdfRender(webView: webView, request: request, context: context, onFinish: onFinish).start()
    } catch {
        "bad render request: \(error)".withCString { onFinish(context, statusFailed, $0) }
    }
}

/// The system's default paper size in points, portrait.
@_cdecl("chain_pdf_default_paper")
public func chainPdfDefaultPaper(_ width: UnsafeMutablePointer<Double>, _ height: UnsafeMutablePointer<Double>) {
    let size = NSPrintInfo.shared.paperSize
    width.pointee = min(size.width, size.height)
    height.pointee = max(size.width, size.height)
}
