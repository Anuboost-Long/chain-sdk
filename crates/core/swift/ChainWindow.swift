// The window capability's AppKit pieces for crates/core/src/window.rs —
// see agent-docs/capabilities/window/research/MACOS.md.
//
// Drag regions: a transparent view
// over the app's WKWebView takes the mouse-down wherever the page
// declared a drag region (minus its holes: buttons, links, inputs...) and
// lets everything else fall through to the page. Dragging from that view
// hands AppKit the real event; starting a drag later over IPC can't,
// because by then NSApp.currentEvent is no longer the mouse-down.

import AppKit

final class ChainDragView: NSView {
    /// In the webview's coordinates: CSS pixels from its top-left.
    var regions: [NSRect] = []
    var holes: [NSRect] = []
    private var doubleClickAt: NSPoint?

    override var isFlipped: Bool { true }
    override var mouseDownCanMoveWindow: Bool { true }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        let dragsHere = regions.contains { $0.contains(local) } && !holes.contains { $0.contains(local) }
        return dragsHere ? self : nil
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 {
            // The title bar acts on the second click's mouse-up, and not if the pointer moved.
            doubleClickAt = NSEvent.mouseLocation
            return
        }
        window?.performDrag(with: event)
    }

    override func mouseUp(with event: NSEvent) {
        defer { doubleClickAt = nil }
        guard event.clickCount == 2, let at = doubleClickAt, at == NSEvent.mouseLocation, let window else { return }
        titleBarDoubleClick(window)
    }
}

/// What double-clicking the title bar does, per System Settings (Desktop &
/// Dock → "Double-click a window's title bar to").
private func titleBarDoubleClick(_ window: NSWindow) {
    let defaults = UserDefaults.standard
    switch defaults.string(forKey: "AppleActionOnDoubleClick") {
    case "Minimize": window.miniaturize(nil)
    case "None": break
    case .some: window.zoom(nil)
    // Before the setting had three choices it was a yes/no.
    case nil: defaults.bool(forKey: "AppleMiniaturizeOnDoubleClick") ? window.miniaturize(nil) : window.zoom(nil)
    }
}

private func rects(_ values: UnsafePointer<Double>?, _ count: Int) -> [NSRect] {
    guard let values else { return [] }
    return (0..<count).map { i in
        NSRect(x: values[i * 4], y: values[i * 4 + 1], width: values[i * 4 + 2], height: values[i * 4 + 3])
    }
}

/// Replaces the drag regions over `webview` (a WKWebView), each rect four
/// doubles: x, y, width, height. The view is added on first use and kept
/// over the webview, matching its frame. Main thread.
@_cdecl("chain_window_set_drag_regions")
public func chainWindowSetDragRegions(
    _ webview: UnsafeMutableRawPointer,
    _ regions: UnsafePointer<Double>?,
    _ regionCount: Int,
    _ holes: UnsafePointer<Double>?,
    _ holeCount: Int
) {
    let page = Unmanaged<NSView>.fromOpaque(webview).takeUnretainedValue()
    guard let parent = page.superview else { return }
    let dragView = parent.subviews.lazy.compactMap { $0 as? ChainDragView }.first ?? {
        let view = ChainDragView(frame: page.frame)
        view.autoresizingMask = page.autoresizingMask
        parent.addSubview(view, positioned: .above, relativeTo: page)
        return view
    }()
    dragView.frame = page.frame
    dragView.regions = rects(regions, regionCount)
    dragView.holes = rects(holes, holeCount)
}

@_cdecl("chain_window_title_bar_double_click")
public func chainWindowTitleBarDoubleClick(_ nsWindow: UnsafeMutableRawPointer) {
    titleBarDoubleClick(Unmanaged<NSWindow>.fromOpaque(nsWindow).takeUnretainedValue())
}

// MARK: Title bar size

private let toolbarIdentifier = "chain-title-bar"

/// 0 = the standard title bar, 1 = medium (a compact toolbar's bar),
/// 2 = large (a unified toolbar's bar). The toolbar has no items: it's only
/// there so AppKit itself sizes the bar and lays out the window buttons
/// for it. A toolbar Chain didn't add is left alone. Main thread.
@_cdecl("chain_window_set_title_bar_size")
public func chainWindowSetTitleBarSize(_ nsWindow: UnsafeMutableRawPointer, _ size: Int) {
    let window = Unmanaged<NSWindow>.fromOpaque(nsWindow).takeUnretainedValue()
    let ours = window.toolbar.map { $0.identifier == toolbarIdentifier } ?? true
    guard ours else { return }
    if size == 0 {
        window.toolbar = nil
    } else {
        window.toolbarStyle = size == 1 ? .unifiedCompact : .unified
        if window.toolbar == nil {
            let toolbar = NSToolbar(identifier: toolbarIdentifier)
            toolbar.showsBaselineSeparator = false
            toolbar.allowsUserCustomization = false
            toolbar.displayMode = .iconOnly
            window.toolbar = toolbar
        }
    }
    ButtonPlacer.of(window)?.place()
}

// MARK: Window button position

private var placerKey: UInt8 = 0

/// Keeps the window buttons at a position AppKit doesn't offer. AppKit
/// lays them out again on resize, zoom, focus and full-screen changes;
/// their frame-change notifications arrive inside that same layout pass,
/// so they're put back before anything is drawn — no visible jump.
private final class ButtonPlacer: NSObject {
    private weak var window: NSWindow?
    var left: Double
    var centerY: Double
    /// AppKit's own distance between buttons, taken before they're moved:
    /// mid-relayout, some have moved back and some haven't.
    private var spacing: Double?
    private var placing = false
    private var observed: [NSView] = []

    init(window: NSWindow, left: Double, centerY: Double) {
        self.window = window
        self.left = left
        self.centerY = centerY
        super.init()
        let center = NotificationCenter.default
        for name in [NSWindow.didResizeNotification, NSWindow.didExitFullScreenNotification,
                     NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            center.addObserver(self, selector: #selector(relayout), name: name, object: window)
        }
    }

    static func of(_ window: NSWindow) -> ButtonPlacer? {
        objc_getAssociatedObject(window, &placerKey) as? ButtonPlacer
    }

    static func set(_ window: NSWindow, _ placer: ButtonPlacer?) {
        if let old = of(window) { NotificationCenter.default.removeObserver(old) }
        objc_setAssociatedObject(window, &placerKey, placer, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
    }

    @objc private func relayout(_ note: Notification) { place() }

    func place() {
        guard !placing, let window, !window.styleMask.contains(.fullScreen) else { return }
        let buttons = [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton].compactMap(window.standardWindowButton)
        guard buttons.count == 3, let container = buttons[0].superview?.superview else { return }
        placing = true
        defer { placing = false }
        watch(buttons + [container])

        let windowHeight = window.frame.height
        let barHeight = max(0, windowHeight - window.contentLayoutRect.maxY)
        let buttonHeight = buttons[0].frame.height
        var frame = container.frame
        frame.size.height = max(barHeight, centerY * 2)
        frame.origin.y = windowHeight - frame.height
        if frame != container.frame { container.frame = frame }

        let spacing = self.spacing ?? buttons[1].frame.minX - buttons[0].frame.minX
        self.spacing = spacing
        let bottom = windowHeight - centerY - buttonHeight / 2
        for (i, button) in buttons.enumerated() {
            guard let parent = button.superview else { continue }
            let origin = parent.convert(NSPoint(x: left + Double(i) * spacing, y: bottom), from: nil)
            if button.frame.origin != origin { button.setFrameOrigin(origin) }
        }
    }

    /// AppKit rebuilds the title bar's views on some changes (a toolbar
    /// added or removed), so the ones watched are refreshed on every pass.
    private func watch(_ views: [NSView]) {
        guard views != observed else { return }
        let center = NotificationCenter.default
        for view in observed { center.removeObserver(self, name: NSView.frameDidChangeNotification, object: view) }
        for view in views {
            view.postsFrameChangedNotifications = true
            center.addObserver(self, selector: #selector(relayout), name: NSView.frameDidChangeNotification, object: view)
        }
        observed = views
    }
}

/// Puts the window buttons at (left, centerY) — CSS pixels from the
/// window's top-left to the first button's left edge and to the buttons'
/// centre — and keeps them there; `keep` false hands them back to AppKit.
/// Main thread.
@_cdecl("chain_window_place_buttons")
public func chainWindowPlaceButtons(_ nsWindow: UnsafeMutableRawPointer, _ keep: Bool, _ left: Double, _ centerY: Double) {
    let window = Unmanaged<NSWindow>.fromOpaque(nsWindow).takeUnretainedValue()
    if keep {
        if let placer = ButtonPlacer.of(window) {
            placer.left = left
            placer.centerY = centerY
            placer.place()
        } else {
            let placer = ButtonPlacer(window: window, left: left, centerY: centerY)
            ButtonPlacer.set(window, placer)
            placer.place()
        }
        return
    }
    guard ButtonPlacer.of(window) != nil else { return }
    ButtonPlacer.set(window, nil)
    // AppKit only lays the title bar out from scratch when it's rebuilt;
    // a toolbar coming and going rebuilds it.
    let toolbar = window.toolbar
    window.toolbar = NSToolbar(identifier: "chain-relayout")
    window.toolbar = toolbar
}
