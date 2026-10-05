import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  BrowserApi,
  BrowserAvailability,
  BrowserButton,
  BrowserFetchError,
  BrowserFetchResponse,
  BrowserPage,
  BrowserPageContent,
  BrowserUnsubscribe
} from "./contracts/browser";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = ["UNSUPPORTED", "INVALID_ARGUMENT", "NOT_FOUND", "UNAVAILABLE", "TOO_LARGE"];

const NOTHING_AVAILABLE: BrowserAvailability = {
  available: false,
  toolbar: false,
  buttons: false,
  popups: false,
  persistentSessions: false,
  namedSessions: false,
  clearSession: false,
  readContent: false,
  readFrames: false,
  fetch: false,
  openBeside: false
};

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.browser.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  requireTauri(method);
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "the browser window failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

const withDefaults = (buttons: BrowserButton[]) =>
  buttons.map(({ id, label, enabled = true }) => ({ id, label, enabled }));

/** browser_fetch's buffer: a big-endian u32 length, the head as JSON, then the body. */
function unframe(buffer: ArrayBuffer): BrowserFetchResponse {
  const headLength = new DataView(buffer).getUint32(0);
  const head = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 4, headLength)));
  return {
    url: head.url,
    status: head.status,
    contentType: head.contentType ?? undefined,
    bytes: new Uint8Array(buffer, 4 + headLength)
  };
}

/** Synchronous to the caller (handy in effects); the listener is dropped
 * even if unsubscribed before Tauri finished registering it. */
function subscribe<T>(event: string, listener: (payload: T) => void): BrowserUnsubscribe {
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

export const browser: BrowserApi = {
  async availability(): Promise<BrowserAvailability> {
    if (!isTauri()) return NOTHING_AVAILABLE;
    return call<BrowserAvailability>("availability", "browser_availability");
  },

  open: (options) =>
    call<void>("open", "browser_open", {
      options: { ...options, buttons: options.buttons && withDefaults(options.buttons) }
    }),

  close: (options) => call<void>("close", "browser_close", { session: options?.session }),

  setButtons: (buttons, options) =>
    call<void>("setButtons", "browser_set_buttons", { session: options?.session, buttons: withDefaults(buttons) }),

  current: (options) => call<BrowserPage | null>("current", "browser_current", { session: options?.session }),

  read: (options) => call<BrowserPageContent>("read", "browser_read", { session: options?.session }),

  async fetch(url, options): Promise<BrowserFetchResponse> {
    const maxBytes = options?.maxBytes;
    if (maxBytes !== undefined && !(Number.isSafeInteger(maxBytes) && maxBytes >= 0)) {
      throw chainError("INVALID_ARGUMENT", `maxBytes must be a whole number of bytes, got ${maxBytes}`);
    }
    const buffer = await call<ArrayBuffer>("fetch", "browser_fetch", {
      url,
      session: options?.session,
      maxBytes,
      timeoutMs: options?.timeout
    });
    const response = unframe(buffer);
    if (response.status >= 200 && response.status < 300) return response;
    const error: BrowserFetchError = {
      code: "HTTP_ERROR",
      message: `Request failed with status ${response.status}`,
      response
    };
    throw error;
  },

  clearSession: (options) => call<void>("clearSession", "browser_clear_session", { session: options?.session }),

  onNavigate: (listener) => subscribe("chain://browser-navigated", listener),
  onButton: (listener) => subscribe("chain://browser-button", listener),
  onClose: (listener) => subscribe("chain://browser-closed", listener)
};
