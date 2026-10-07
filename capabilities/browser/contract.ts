/**
 * Structural contract for the Browser capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** Which parts of the browser window work here — OS, OS version, and web engine. */
export interface BrowserAvailability {
  /** Whether open() can show a browser window at all. Every flag below is false when this is. */
  available: boolean;
  /** Back, Forward, Reload and the address. */
  toolbar: boolean;
  /** App-defined toolbar buttons and onButton. */
  buttons: boolean;
  /** window.open and target=_blank sign-in popups sharing the session. */
  popups: boolean;
  /** Sign-ins kept between launches, apart from the app's own webview. */
  persistentSessions: boolean;
  /** More than one session (the `session` option). */
  namedSessions: boolean;
  clearSession: boolean;
  /** read() returns the page's rendered HTML. */
  readContent: boolean;
  /** read() includes same-origin frames. */
  readFrames: boolean;
  /** fetch() with the session's cookies. */
  fetch: boolean;
  /** The `beside` option places the window next to the app's. */
  openBeside: boolean;
}

export interface BrowserButton {
  /** Unique within the window; comes back in onButton. */
  id: string;
  label: string;
  /** Defaults to true. */
  enabled?: boolean;
}

export interface BrowserSessionOptions {
  /** Which session (and so which window) — defaults to "default". 1–100 characters. */
  session?: string;
}

export interface BrowserOpenOptions extends BrowserSessionOptions {
  /** http or https. Required when the session's window isn't open; when it is, navigates there. */
  url?: string;
  /** Fixed window title. Defaults to following the page's own title. */
  title?: string;
  /** Initial size in points. Defaults to 1100 × 800. */
  width?: number;
  height?: number;
  /** Defaults to 480 × 360. */
  minWidth?: number;
  minHeight?: number;
  /** Open next to the app window, or centered when there's no room. Defaults to true. */
  beside?: boolean;
  /** Back, Forward, Reload and the address. Defaults to true. */
  toolbar?: boolean;
  /** App-defined buttons at the toolbar's end, in order. Needs the toolbar. */
  buttons?: BrowserButton[];
}

/** The page a session's window shows. */
export interface BrowserPage {
  session: string;
  url: string;
  title: string;
}

export interface BrowserFrame {
  url: string;
  html: string;
}

/** The page as it stands, after its scripts ran. */
export interface BrowserPageContent {
  url: string;
  title: string;
  /** `document.documentElement.outerHTML` of the top page. */
  html: string;
  /** Every frame the page itself may read (same origin), nested ones included, in document order. */
  frames: BrowserFrame[];
  /** Addresses of the frames it may not read (cross-origin). */
  unreadableFrames: string[];
}

export interface BrowserButtonPress extends BrowserPage {
  /** The pressed button's id. */
  id: string;
}

export interface BrowserClosed {
  session: string;
  /** "user" when the student closed it, "app" after close(). */
  reason: "user" | "app";
}

export interface BrowserFetchOptions extends BrowserSessionOptions {
  /** Rejects TOO_LARGE past this many bytes. No limit by default. */
  maxBytes?: number;
  /** Milliseconds. Defaults to 30 000. */
  timeout?: number;
}

export interface BrowserFetchResponse {
  /** After redirects. */
  url: string;
  status: number;
  /** The Content-Type header, if any. */
  contentType?: string;
  bytes: Uint8Array;
}

/** fetch()'s rejection for a status outside 200–299, with the response attached. */
export interface BrowserFetchError {
  code: "HTTP_ERROR";
  message: string;
  response: BrowserFetchResponse;
}

export type BrowserUnsubscribe = () => void;

export interface BrowserApi {
  availability(): Promise<BrowserAvailability>;
  /** Resolves once the window shows. Opening an open session's window focuses it instead. */
  open(options: BrowserOpenOptions): Promise<void>;
  /** No-op when that session's window isn't open. */
  close(options?: BrowserSessionOptions): Promise<void>;
  /** Replaces the toolbar's app buttons. */
  setButtons(buttons: BrowserButton[], options?: BrowserSessionOptions): Promise<void>;
  /** The page shown, or null when that session's window isn't open. */
  current(options?: BrowserSessionOptions): Promise<BrowserPage | null>;
  read(options?: BrowserSessionOptions): Promise<BrowserPageContent>;
  /** GET with the session's cookies. The window needn't be open. */
  fetch(url: string, options?: BrowserFetchOptions): Promise<BrowserFetchResponse>;
  /** Signs out: deletes the session's cookies and site storage. An open window reloads. */
  clearSession(options?: BrowserSessionOptions): Promise<void>;
  /** Every page change in any session's window, in-page (history) navigation included. */
  onNavigate(listener: (page: BrowserPage) => void): BrowserUnsubscribe;
  onButton(listener: (press: BrowserButtonPress) => void): BrowserUnsubscribe;
  onClose(listener: (closed: BrowserClosed) => void): BrowserUnsubscribe;
}
