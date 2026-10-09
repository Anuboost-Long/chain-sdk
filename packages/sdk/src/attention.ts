import { listen } from "@tauri-apps/api/event";

import { invoke, isTauri } from "./native";
import type {
  AttentionApi,
  AttentionUnsubscribe,
  NotificationPermission,
  NotifyOptions,
  NotifyResult
} from "./contracts/attention";
import { chainError } from "./errors";

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.attention.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : `attention.${method}() failed`;
    if (message.startsWith("INVALID_ARGUMENT: ")) throw chainError("INVALID_ARGUMENT", message.slice(18));
    throw chainError("NATIVE_FAILURE", message);
  }
}

// Native emits these to this page's webview only; one listener per event,
// for the page's lifetime, shared by every subscriber.
const listening = new Set<string>();

function subscribe<T>(event: string, handlers: Set<(value: T) => void>, handler: (value: T) => void) {
  if (!isTauri()) return () => {};
  if (!listening.has(event)) {
    listening.add(event);
    void listen<T>(event, (e) => {
      for (const each of handlers) each(e.payload);
    });
  }
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

const focusHandlers = new Set<(focused: boolean) => void>();
const clickHandlers = new Set<(payload: { id: string }) => void>();

export const attention: AttentionApi = {
  isFocused() {
    return call<boolean>("isFocused", "attention_is_focused");
  },

  onFocusChange(handler: (focused: boolean) => void): AttentionUnsubscribe {
    return subscribe("chain://attention-focus", focusHandlers, handler);
  },

  notificationPermission() {
    return call<NotificationPermission>("notificationPermission", "attention_permission");
  },

  notify(options: NotifyOptions) {
    return call<NotifyResult>("notify", "attention_notify", { id: options.id, title: options.title, body: options.body });
  },

  onNotificationClick(handler: (id: string) => void): AttentionUnsubscribe {
    return subscribe<{ id: string }>("chain://attention-click", clickHandlers, (payload) => handler(payload.id));
  },

  requestAttention() {
    return call<void>("requestAttention", "attention_request");
  }
};
