/**
 * Structural contract for the BrowserView capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** Which parts of an embedded view work here — OS, OS version, and web engine. */
export interface BrowserViewAvailability {
  /** Whether create() works at all. Every flag below is false when this is. */
  available: boolean;
  /** onLoadFailed gives a `reason` other than "other" where it can tell. */
  loadFailureReasons: boolean;
  /** onCrash fires when a view's page process dies. */
  crashes: boolean;
  /** The `decideNavigation` option. */
  decideNavigation: boolean;
  /** Requests carry `modifiers` and `button`. */
  inputDetails: boolean;
  /** Requests carry `inputAgeMs`. */
  inputAge: boolean;
  /** The `allowFullscreen` option. */
  fullscreen: boolean;
  /** The `swipeNavigation` option. */
  swipeNavigation: boolean;
  /** The `inspectable` option. */
  inspectable: boolean;
  /** The `allowPictureInPicture` option. */
  pictureInPicture: boolean;
  /** setRules() and removeRules(). */
  rules: boolean;
  /** setScript(), removeScript() and handleMessages(). */
  pageScripts: boolean;
  /** runInFrames(). */
  runInFrames: boolean;
}

/** In the app page's CSS pixels, relative to its viewport — what `getBoundingClientRect()` gives. */
export interface BrowserViewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BrowserViewCreateOptions {
  /** An address a view may show (CONTRACT.md) — http or https, never the app's own origin. */
  url: string;
  /** The persistent cookie and site-storage store, 1–100 characters. The same name is the same store as in desktop.browser. */
  session: string;
  bounds: BrowserViewBounds;
  /** Defaults to true. */
  visible?: boolean;
  /** Hold every navigation the page starts until answer(). Defaults to false: they go ahead. */
  decideNavigation?: boolean;
  /** Pages may take an element fullscreen (video players). Defaults to false. */
  allowFullscreen?: boolean;
  /** Pages may float a video out with `requestPictureInPicture()`. Defaults to false. */
  allowPictureInPicture?: boolean;
  /** A two-finger horizontal swipe goes back/forward, with the engine's own animation. Defaults to false. */
  swipeNavigation?: boolean;
  /** Web Inspector reaches the view (right-click → Inspect Element). Defaults to false. */
  inspectable?: boolean;
}

/** What a view shows now. */
export interface BrowserViewState {
  id: string;
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}

export type BrowserViewLoadFailureReason =
  | "offline"
  | "host-not-found"
  | "unreachable"
  | "insecure-connection"
  | "blocked"
  | "other";

/** The view's page failed to load. Nested frames' failures aren't reported. */
export interface BrowserViewLoadFailure {
  id: string;
  /** The address that failed. */
  url: string;
  reason: BrowserViewLoadFailureReason;
  /** The engine's own wording, for logs — not for showing as-is. */
  description: string;
  /** Engine-specific, for debugging only: never branch on these. */
  nativeCode: number;
  nativeDomain: string;
}

/** The view's page process died; it shows nothing until reload() or load(). */
export interface BrowserViewCrash {
  id: string;
  url: string;
}

export interface BrowserViewModifiers {
  shift: boolean;
  control: boolean;
  /** Option on macOS. */
  alt: boolean;
  /** Command on macOS, the Windows key on Windows. */
  meta: boolean;
}

export type BrowserViewMouseButton = "primary" | "middle" | "secondary";

/** Who or what started a navigation or a new window. */
export interface BrowserViewInitiator {
  /** The document that started it. Empty when the engine doesn't say. */
  sourceUrl: string;
  /** Whether that document is the view's own page; false for a nested frame. */
  fromMainFrame: boolean;
  /** Keys held by the click or key press that started it; all false when no input did. */
  modifiers: BrowserViewModifiers;
  /** The mouse button that started it, or null when it wasn't a click. */
  button: BrowserViewMouseButton | null;
  /** Milliseconds since the last click or key press inside this view, null when there's been none. Pointer movement and scrolling don't count. */
  inputAgeMs: number | null;
}

export type BrowserViewNavigationKind = "link" | "form" | "history" | "reload" | "other";

/** A navigation held for answer() — only with `decideNavigation`. */
export interface BrowserViewNavigationRequest extends BrowserViewInitiator {
  id: string;
  /** Pass to answer(). */
  requestId: string;
  /** Where the view's page would go. */
  url: string;
  kind: BrowserViewNavigationKind;
}

/** What the page asked of the window, from `window.open()`'s features. Absent when it didn't say. */
export interface BrowserViewWindowFeatures {
  left?: number;
  top?: number;
  width?: number;
  height?: number;
  resizable?: boolean;
  toolbar?: boolean;
  menubar?: boolean;
  status?: boolean;
}

/** The page tried to open a window. Chain opened nothing; the app decides what to do with `url`. */
export interface BrowserViewNewWindow extends BrowserViewInitiator {
  id: string;
  url: string;
  features: BrowserViewWindowFeatures;
}

export type BrowserViewResourceType =
  | "document"
  | "image"
  | "style-sheet"
  | "script"
  | "font"
  | "media"
  | "svg-document"
  | "popup"
  | "ping"
  | "fetch"
  | "websocket"
  | "other";

/** Which requests or pages a rule applies to. Every field given must match. */
export interface BrowserViewRuleTrigger {
  /** A regular expression (CONTRACT.md's subset) matched against the request's full URL. */
  urlFilter: string;
  /** Defaults to false. */
  urlFilterIsCaseSensitive?: boolean;
  /** Any of these; all types when absent. */
  resourceType?: BrowserViewResourceType[];
  /** Relative to the view's page. Both when absent. */
  loadType?: ("first-party" | "third-party")[];
  /** Only on pages of these hosts; a leading `*` includes subdomains (`*example.com`). At most one of the four below. */
  ifDomain?: string[];
  /** Except on pages of these hosts. */
  unlessDomain?: string[];
  /** Only on pages whose URL matches one of these regular expressions. */
  ifTopUrl?: string[];
  /** Except on pages whose URL matches one of these. */
  unlessTopUrl?: string[];
}

export type BrowserViewRuleAction =
  /** Don't send the request. */
  | { type: "block" }
  /** Send it without cookies. */
  | { type: "block-cookies" }
  /** Hide the elements matching `selector` on the pages the trigger matches. */
  | { type: "css-display-none"; selector: string }
  /** Undo every earlier rule in this list for what the trigger matches (an exception). */
  | { type: "ignore-previous-rules" }
  /** Load http as https. */
  | { type: "make-https" };

export interface BrowserViewRule {
  trigger: BrowserViewRuleTrigger;
  action: BrowserViewRuleAction;
}

export interface BrowserViewRuleList {
  session: string;
  /** Unique within the session; setRules() with the same id replaces it. */
  id: string;
  /** In order: a later rule can undo an earlier one. */
  rules: BrowserViewRule[];
}

export interface BrowserViewScript {
  session: string;
  /** Unique within the session; setScript() with the same id replaces it. */
  id: string;
  /** Runs as-is in every frame, at document start, in the page's own world. */
  source: string;
}

export interface BrowserViewFrame {
  url: string;
  /** Whether it's the view's own page; false for a nested frame. */
  mainFrame: boolean;
}

/** What a session's script sent with `chainBrowserView.ask(message)`. Untrusted: the page can send it too. */
export interface BrowserViewMessage {
  /** The view it came from. */
  id: string;
  frame: BrowserViewFrame;
  /** JSON, as the page sent it. */
  message: unknown;
}

/** One frame's outcome from runInFrames(). */
export interface BrowserViewFrameResult {
  frame: BrowserViewFrame;
  /** What the script returned (awaited), as JSON. Absent when it threw. */
  value?: unknown;
  /** Why it failed in this frame, when it did. */
  error?: string;
}

export type BrowserViewUnsubscribe = () => void;

export interface BrowserViewApi {
  availability(): Promise<BrowserViewAvailability>;
  /** Resolves with the new view's id once it's placed; its first load is under way. */
  create(options: BrowserViewCreateOptions): Promise<string>;
  /** No-op for an id that's already closed. */
  close(id: string): Promise<void>;
  setBounds(id: string, bounds: BrowserViewBounds): Promise<void>;
  setVisible(id: string, visible: boolean): Promise<void>;
  load(id: string, url: string): Promise<void>;
  back(id: string): Promise<void>;
  forward(id: string): Promise<void>;
  reload(id: string): Promise<void>;
  stop(id: string): Promise<void>;
  state(id: string): Promise<BrowserViewState>;
  /** Lets a held navigation go ahead, or refuses it. Ignored once answered or expired. */
  answer(requestId: string, allow: boolean): Promise<void>;
  /** Any change to a view's url, title, loading or back/forward state, in-page navigation included. */
  onState(listener: (state: BrowserViewState) => void): BrowserViewUnsubscribe;
  onLoadFailed(listener: (failure: BrowserViewLoadFailure) => void): BrowserViewUnsubscribe;
  onCrash(listener: (crash: BrowserViewCrash) => void): BrowserViewUnsubscribe;
  onNavigationRequest(listener: (request: BrowserViewNavigationRequest) => void): BrowserViewUnsubscribe;
  onNewWindow(listener: (request: BrowserViewNewWindow) => void): BrowserViewUnsubscribe;
  /** Adds or replaces a session's rule list; every view on the session uses it from its next load. */
  setRules(list: BrowserViewRuleList): Promise<void>;
  /** No-op for a list that isn't set. */
  removeRules(options: { session: string; id: string }): Promise<void>;
  /** Adds or replaces a session's script; every view on the session runs it from its next load. */
  setScript(script: BrowserViewScript): Promise<void>;
  /** No-op for a script that isn't set. */
  removeScript(options: { session: string; id: string }): Promise<void>;
  /** Answers every `chainBrowserView.ask()` with what `handler` returns (awaited). One handler; a new call replaces it. */
  handleMessages(handler: (message: BrowserViewMessage) => unknown): BrowserViewUnsubscribe;
  /** Runs `body` (an async function's body: `return` the result) in the view's page and every nested frame, cross-origin included, as if the user had just clicked. */
  runInFrames(id: string, body: string): Promise<BrowserViewFrameResult[]>;
}
