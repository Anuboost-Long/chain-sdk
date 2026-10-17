// System notifications for the attention capability — see
// agent-docs/capabilities/attention/. UNUserNotificationCenter raises an
// uncaught exception (bundleProxyForCurrentProcess is nil) in a process
// that isn't running from an .app bundle, which is how `chain dev` runs,
// so every entry point checks `isBundled` before touching it.

import AppKit
import Foundation
import UserNotifications

public typealias ChainAttentionClick = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>) -> Void
/// The status, and the system's error message when there is one ("" otherwise).
public typealias ChainAttentionDone = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>) -> Void

private let statusUnavailable: Int32 = -1
private let statusNotDetermined: Int32 = 0
private let statusDenied: Int32 = 1
private let statusGranted: Int32 = 2
private let shownStatus: Int32 = 0
private let failedStatus: Int32 = 3

private let idKey = "chainNotificationId"

private func finish(_ done: ChainAttentionDone, _ context: UnsafeMutableRawPointer?, _ status: Int32, _ error: Error? = nil) {
    (error?.localizedDescription ?? "").withCString { done(context, status, $0) }
}

private var isBundled: Bool {
    Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil
}

private final class Delegate: NSObject, UNUserNotificationCenterDelegate {
    let context: UnsafeMutableRawPointer?
    let onClick: ChainAttentionClick

    init(context: UnsafeMutableRawPointer?, onClick: @escaping ChainAttentionClick) {
        self.context = context
        self.onClick = onClick
    }

    // Shown even while the app is frontmost: the app decides when to notify.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .list, .sound])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier,
           let id = response.notification.request.content.userInfo[idKey] as? String {
            id.withCString { onClick(context, $0) }
        }
        completionHandler()
    }
}

private var delegate: Delegate?

@_cdecl("chain_attention_is_bundled")
public func chainAttentionIsBundled() -> Bool {
    isBundled
}

/// Installs the click handler. Call once, at launch, so a click that
/// activates the app is delivered too. No-op outside an .app bundle.
@_cdecl("chain_attention_start")
public func chainAttentionStart(_ context: UnsafeMutableRawPointer?, _ onClick: @escaping ChainAttentionClick) {
    guard isBundled, delegate == nil else { return }
    let installed = Delegate(context: context, onClick: onClick)
    delegate = installed
    UNUserNotificationCenter.current().delegate = installed
}

private func status(of settings: UNNotificationSettings) -> Int32 {
    switch settings.authorizationStatus {
    case .notDetermined: return statusNotDetermined
    case .denied: return statusDenied
    default: return statusGranted // authorized, provisional, ephemeral
    }
}

@_cdecl("chain_attention_permission")
public func chainAttentionPermission(_ context: UnsafeMutableRawPointer?, _ done: @escaping ChainAttentionDone) {
    guard isBundled else { return finish(done, context, statusUnavailable) }
    UNUserNotificationCenter.current().getNotificationSettings { finish(done, context, status(of: $0)) }
}

/// Asks for permission the first time, then shows the notification.
/// `done` gets shownStatus, statusDenied (the user said no), statusUnavailable,
/// or failedStatus with the system's message (it refused the request).
@_cdecl("chain_attention_notify")
public func chainAttentionNotify(
    _ id: UnsafePointer<CChar>,
    _ title: UnsafePointer<CChar>,
    _ body: UnsafePointer<CChar>,
    _ context: UnsafeMutableRawPointer?,
    _ done: @escaping ChainAttentionDone
) {
    guard isBundled else { return finish(done, context, statusUnavailable) }
    let id = String(cString: id)
    let content = UNMutableNotificationContent()
    content.title = String(cString: title)
    content.body = String(cString: body)
    content.sound = .default
    content.userInfo = [idKey: id]
    let center = UNUserNotificationCenter.current()

    center.requestAuthorization(options: [.alert, .sound]) { granted, error in
        // An error isn't the user's answer: macOS refused to ask at all
        // ("Notifications are not allowed for this application").
        if let error { return finish(done, context, failedStatus, error) }
        guard granted else { return finish(done, context, statusDenied) }
        // The app's id as the request id: a second notification with the
        // same id replaces the first instead of stacking.
        let request = UNNotificationRequest(identifier: id, content: content, trigger: nil)
        center.add(request) { error in finish(done, context, error == nil ? shownStatus : failedStatus, error) }
    }
}
