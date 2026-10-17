/**
 * Structural contract for the Attention capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/**
 * `unavailable`: there's no notification system to ask — notably under
 * `chain dev`, whose binary isn't an app bundle (see CONTRACT.md).
 */
export type NotificationPermission = "granted" | "denied" | "notDetermined" | "unavailable";

export interface NotifyOptions {
  /** The app's own id, handed back by `onNotificationClick`. A newer notification with the same id replaces the older one. */
  id: string;
  title: string;
  body?: string;
}

/**
 * Never a rejection: the app decides what to tell the user. `denied` means
 * the user said no; `failed` means the system refused, with its `message`.
 */
export type NotifyResult =
  | { outcome: "shown" | "denied" | "unavailable" }
  | { outcome: "failed"; message: string };

export type AttentionUnsubscribe = () => void;

export interface AttentionApi {
  /** Whether this page's window has keyboard focus. */
  isFocused(): Promise<boolean>;
  onFocusChange(handler: (focused: boolean) => void): AttentionUnsubscribe;

  notificationPermission(): Promise<NotificationPermission>;
  /** Shows a system notification with the app's icon. The first call asks the user for permission. */
  notify(options: NotifyOptions): Promise<NotifyResult>;
  /** The window that showed the notification is already brought forward when this fires. */
  onNotificationClick(handler: (id: string) => void): AttentionUnsubscribe;

  /** Bounces the Dock icon once (macOS), if the app isn't active. */
  requestAttention(): Promise<void>;
}
