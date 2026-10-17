import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  BrowserViewApi,
  BrowserViewAvailability,
  BrowserViewBounds,
  BrowserViewFrameResult,
  BrowserViewMessage,
  BrowserViewState,
  BrowserViewUnsubscribe
} from "./contracts/browser-view";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = ["UNSUPPORTED", "INVALID_ARGUMENT", "NOT_FOUND"];

const NOTHING_AVAILABLE: BrowserViewAvailability = {
  available: false,
  loadFailureReasons: false,
  crashes: false,
  decideNavigation: false,
  inputDetails: false,
  inputAge: false,
  fullscreen: false,
  swipeNavigation: false,
  inspectable: false,
  pictureInPicture: false,
  rules: false,
  pageScripts: false,
  runInFrames: false
};

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.browserView.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "the browser view failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

/** Only the four numbers, so a DOMRect passes as-is. */
const plain = ({ x, y, width, height }: BrowserViewBounds) => ({ x, y, width, height });

/** Synchronous to the caller (handy in effects); the listener is dropped
 * even if unsubscribed before Tauri finished registering it. */
function subscribe<T>(event: string, listener: (payload: T) => void): BrowserViewUnsubscribe {
  if (!isTauri()) return () => {};
  let subscribed = true;
  const registering = listen<T>(event, ({ payload }) => {
    if (subscribed) listener(payload);
  });
  return () => {
    subscribed = false;
    registering.then((unlisten) => unlisten());
  };
}

type MessageHandler = (message: BrowserViewMessage) => unknown;

let messageHandler: MessageHandler | undefined;
let listeningForMessages = false;

async function answer(requestId: string, message: BrowserViewMessage): Promise<void> {
  let reply: { value?: string; error?: string };
  try {
    if (!messageHandler) throw new Error("the app isn't handling messages");
    const value = JSON.stringify((await messageHandler(message)) ?? null);
    reply = value === undefined ? { error: "the answer isn't JSON" } : { value };
  } catch (error) {
    reply = { error: error instanceof Error ? error.message : String(error) };
  }
  await invoke("browser_view_reply", { requestId, ...reply });
}

export const browserView: BrowserViewApi = {
  async availability(): Promise<BrowserViewAvailability> {
    if (!isTauri()) return NOTHING_AVAILABLE;
    return call<BrowserViewAvailability>("availability", "browser_view_availability");
  },

  create: (options) =>
    call<string>("create", "browser_view_create", { options: { ...options, bounds: plain(options.bounds) } }),
  close: (id) => call<void>("close", "browser_view_close", { id }),
  setBounds: (id, bounds) => call<void>("setBounds", "browser_view_set_bounds", { id, bounds: plain(bounds) }),
  setVisible: (id, visible) => call<void>("setVisible", "browser_view_set_visible", { id, visible }),
  load: (id, url) => call<void>("load", "browser_view_load", { id, url }),
  back: (id) => call<void>("back", "browser_view_go", { id, back: true }),
  forward: (id) => call<void>("forward", "browser_view_go", { id, back: false }),
  reload: (id) => call<void>("reload", "browser_view_reload", { id }),
  stop: (id) => call<void>("stop", "browser_view_stop", { id }),
  state: (id) => call<BrowserViewState>("state", "browser_view_state", { id }),
  answer: (requestId, allow) => call<void>("answer", "browser_view_answer", { requestId, allow }),

  onState: (listener) => subscribe("chain://browser-view-state", listener),
  onLoadFailed: (listener) => subscribe("chain://browser-view-load-failed", listener),
  onCrash: (listener) => subscribe("chain://browser-view-crash", listener),
  onNavigationRequest: (listener) => subscribe("chain://browser-view-navigation-request", listener),
  onNewWindow: (listener) => subscribe("chain://browser-view-new-window", listener),

  setRules: ({ session, id, rules }) => call<void>("setRules", "browser_view_set_rules", { session, id, rules }),
  removeRules: ({ session, id }) => call<void>("removeRules", "browser_view_remove_rules", { session, id }),
  setScript: ({ session, id, source }) => call<void>("setScript", "browser_view_set_script", { session, id, source }),
  removeScript: ({ session, id }) => call<void>("removeScript", "browser_view_remove_script", { session, id }),

  handleMessages(handler) {
    messageHandler = handler;
    if (!listeningForMessages) {
      listeningForMessages = true;
      subscribe<BrowserViewMessage & { requestId: string }>("chain://browser-view-message", ({ requestId, ...message }) => {
        void answer(requestId, message);
      });
    }
    return () => {
      if (messageHandler === handler) messageHandler = undefined;
    };
  },

  runInFrames: (id, body) => call<BrowserViewFrameResult[]>("runInFrames", "browser_view_run_in_frames", { id, body })
};
