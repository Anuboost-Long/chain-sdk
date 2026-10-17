/**
 * Structural contract for the Share capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface SharedFile {
  /** A `desktop.files` reference. */
  reference: string;
  /** The file name the recipient sees, e.g. "Cell biology summary.pdf". Defaults to the reference. The reference's extension is added when the name has none. */
  name?: string;
}

/** In CSS pixels from the page's top-left, as getBoundingClientRect() gives them. */
export interface ShareAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ShareOptions {
  /** The element the menu points at, usually the button that opened it. Defaults to the centre of the page. */
  anchor?: ShareAnchor;
  /** Sent along with the files to services that take text, e.g. a message body. */
  text?: string;
  /** For services that take a subject, e.g. Mail. */
  title?: string;
}

export type ShareResult =
  /** `service` is the name the menu showed, e.g. "AirDrop", or null where the platform doesn't say. */
  | { status: "picked"; service: string | null }
  | { status: "cancelled" };

/** Which options work here. Options that don't are accepted and have no effect. */
export interface ShareAvailability {
  /** Whether show() works here at all. Every flag below is false when this is. */
  available: boolean;
  anchor: boolean;
  text: boolean;
  title: boolean;
  /** show() reports which service was picked. */
  serviceName: boolean;
}

export interface ShareApi {
  availability(): Promise<ShareAvailability>;
  /** Opens the system share menu for the files; resolves when it closes. Cancelling resolves, never rejects. */
  show(files: SharedFile[], options?: ShareOptions): Promise<ShareResult>;
}
