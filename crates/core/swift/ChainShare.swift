// The share capability's menu for crates/core/src/share.rs — see
// agent-docs/capabilities/share/research/MACOS.md.
//
// share.rs has already copied the files into a folder of their own under
// the names the recipient should see; this shows NSSharingServicePicker
// for them next to the anchor. The folder is deleted here only when the
// menu is cancelled: a picked service may hold just the files' URLs (Copy
// puts them on the pasteboard and reports done at once), so share.rs's
// sweep deletes it a day later instead.

import AppKit
import WebKit

public typealias ChainShareFinish = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

// Keep in sync with share.rs's `native` module.
private let statusPicked: Int32 = 0
private let statusCancelled: Int32 = 1

private struct Anchor: Decodable {
    var x: Double
    var y: Double
    var width: Double
    var height: Double
}

private struct Request: Decodable {
    var paths: [String]
    var text: String?
    var title: String?
    var anchor: Anchor?
    var folder: String
}

private final class ShareMenu: NSObject, NSSharingServicePickerDelegate, NSSharingServiceDelegate {
    private static var open: Set<ShareMenu> = []

    private let request: Request
    private weak var webView: WKWebView?
    private let context: UnsafeMutableRawPointer?
    private let onFinish: ChainShareFinish
    private var reported = false

    init(request: Request, webView: WKWebView, context: UnsafeMutableRawPointer?, onFinish: @escaping ChainShareFinish) {
        self.request = request
        self.webView = webView
        self.context = context
        self.onFinish = onFinish
    }

    func show(in webView: WKWebView) {
        ShareMenu.open.insert(self)
        var items: [Any] = request.paths.map { URL(fileURLWithPath: $0) }
        if let text = request.text, !text.isEmpty {
            items.append(text)
        }
        let picker = NSSharingServicePicker(items: items)
        picker.delegate = self
        picker.show(relativeTo: anchorRect(in: webView), of: webView, preferredEdge: webView.isFlipped ? .maxY : .minY)
    }

    /// CSS pixels from the page's top-left → the webview's own coordinates.
    private func anchorRect(in webView: WKWebView) -> NSRect {
        let bounds = webView.bounds
        guard let anchor = request.anchor else {
            return NSRect(x: bounds.midX, y: bounds.midY, width: 0, height: 0)
        }
        let zoom = webView.pageZoom * webView.magnification
        let height = anchor.height * zoom
        let top = anchor.y * zoom
        return NSRect(
            x: anchor.x * zoom,
            y: webView.isFlipped ? top : bounds.height - top - height,
            width: anchor.width * zoom,
            height: height
        )
    }

    func sharingServicePicker(_ picker: NSSharingServicePicker, delegateFor sharingService: NSSharingService) -> NSSharingServiceDelegate? {
        if let title = request.title {
            sharingService.subject = title
        }
        return self
    }

    func sharingServicePicker(_ picker: NSSharingServicePicker, didChoose service: NSSharingService?) {
        guard let service else {
            report(statusCancelled, "")
            try? FileManager.default.removeItem(atPath: request.folder)
            done()
            return
        }
        report(statusPicked, service.title)
    }

    func sharingService(_ sharingService: NSSharingService, didShareItems items: [Any]) {
        done()
    }

    func sharingService(_ sharingService: NSSharingService, didFailToShareItems items: [Any], error: Error) {
        done()
    }

    func sharingService(
        _ sharingService: NSSharingService,
        sourceWindowForShareItems items: [Any],
        sharingContentScope: UnsafeMutablePointer<NSSharingService.SharingContentScope>
    ) -> NSWindow? {
        webView?.window
    }

    private func report(_ status: Int32, _ payload: String) {
        guard !reported else { return }
        reported = true
        payload.withCString { onFinish(context, status, $0) }
    }

    /// Nothing more will call back; the files stay for whatever the service handed them to.
    private func done() {
        ShareMenu.open.remove(self)
    }
}

/// Shows the share menu over `webview` (a WKWebView) for the request
/// (JSON, see `Request`). `onFinish` gets statusPicked with the service's
/// name, or statusCancelled, when the menu closes. Main thread.
@_cdecl("chain_share_show")
public func chainShareShow(
    _ webview: UnsafeMutableRawPointer,
    _ requestJSON: UnsafePointer<CChar>,
    _ context: UnsafeMutableRawPointer?,
    _ onFinish: ChainShareFinish
) -> Bool {
    let webView = Unmanaged<WKWebView>.fromOpaque(webview).takeUnretainedValue()
    guard let request = try? JSONDecoder().decode(Request.self, from: Data(String(cString: requestJSON).utf8)) else {
        return false
    }
    ShareMenu(request: request, webView: webView, context: context, onFinish: onFinish).show(in: webView)
    return true
}
