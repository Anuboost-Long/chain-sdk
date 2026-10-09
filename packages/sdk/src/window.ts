import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  ResolvedWindowOptions,
  TitleBarInsets,
  WindowApi,
  WindowAvailability,
  WindowRect,
  WindowUnsubscribe
} from "./contracts/window";
import { chainError } from "./errors";

const NOTHING_AVAILABLE: WindowAvailability = {
  available: false,
  titleBarStyles: { standard: false, transparent: false, overlay: false, hidden: false },
  titleBarSize: false,
  titleVisible: false,
  windowButtonsPosition: false,
  windowButtonsVisibility: false,
  appearance: false,
  backgroundColor: false,
  dragRegions: false,
  startDrag: false,
  titleBarInsets: false,
  fullScreen: false,
  showWhen: { immediately: false, firstPaint: false, showCalled: false }
};

const INSETS_EVENT = "chain://window-insets";
const FULL_SCREEN_EVENT = "chain://window-full-screen";
const SHOWN_EVENT = "chain://window-shown";

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.window.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "the window failed";
    if (message.startsWith("INVALID_ARGUMENT: ")) throw chainError("INVALID_ARGUMENT", message.slice(18));
    throw chainError("NATIVE_FAILURE", message);
  }
}

/** Synchronous to the caller (handy in effects); the listener is dropped
 * even if unsubscribed before Tauri finished registering it. */
function subscribe<T>(event: string, listener: (payload: T) => void): WindowUnsubscribe {
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

export const appWindow: WindowApi = {
  async availability(): Promise<WindowAvailability> {
    if (!isTauri()) return NOTHING_AVAILABLE;
    return call<WindowAvailability>("availability", "window_availability");
  },
  options: () => call<ResolvedWindowOptions>("options", "window_options"),
  setOptions: (options) => call<void>("setOptions", "window_set_options", { options }),
  insets: () => call<TitleBarInsets>("insets", "window_insets"),
  onInsetsChange: (listener) => subscribe(INSETS_EVENT, listener),
  isFullScreen: () => call<boolean>("isFullScreen", "window_is_full_screen"),
  onFullScreenChange: (listener) => subscribe(FULL_SCREEN_EVENT, listener),
  show: () => call<void>("show", "window_show"),
  isShown: () => call<boolean>("isShown", "window_is_shown"),
  onShown: (listener) => subscribe(SHOWN_EVENT, () => listener()),
  startDrag: () => call<void>("startDrag", "window_start_drag")
};

// --- What the SDK does in every Chain page, once it's imported ---

function showInsets({ height, left, right, windowButtons }: TitleBarInsets): void {
  const root = document.documentElement.style;
  const px = (value = 0) => `${value}px`;
  root.setProperty("--chain-title-bar-height", px(height));
  root.setProperty("--chain-title-bar-inset-left", px(left));
  root.setProperty("--chain-title-bar-inset-right", px(right));
  root.setProperty("--chain-window-buttons-x", px(windowButtons?.x));
  root.setProperty("--chain-window-buttons-y", px(windowButtons?.y));
  root.setProperty("--chain-window-buttons-width", px(windowButtons?.width));
  root.setProperty("--chain-window-buttons-height", px(windowButtons?.height));
}

function showFullScreen(fullScreen: boolean): void {
  document.documentElement.toggleAttribute("data-chain-full-screen", fullScreen);
}

const DRAG_REGION = "data-chain-drag-region";
const REGIONS = `[${DRAG_REGION}]:not([${DRAG_REGION}="false"])`;
// Pressing these works as usual inside a drag region, as does anything
// inside data-chain-drag-region="false".
const HOLES = [
  "a",
  "button",
  "input",
  "select",
  "textarea",
  "label",
  "summary",
  "option",
  '[contenteditable]:not([contenteditable="false"])',
  '[tabindex]:not([tabindex="-1"])',
  ...["button", "link", "menuitem", "tab", "checkbox", "radio", "switch", "option"].map((role) => `[role="${role}"]`),
  `[${DRAG_REGION}="false"]`
].join(",");

function inDragRegion(event: MouseEvent): boolean {
  if (!(event.target instanceof Element)) return false;
  const hole = event.target.closest(HOLES);
  const region = event.target.closest(REGIONS);
  if (region === null) return false;
  return hole === null || !(region.contains(hole) || hole.contains(region));
}

interface DragRegions {
  regions: WindowRect[];
  holes: WindowRect[];
}

function measureDragRegions(): DragRegions {
  const measured: DragRegions = { regions: [], holes: [] };
  const add = (list: WindowRect[], element: Element) => {
    const { x, y, width, height } = element.getBoundingClientRect();
    if (width > 0 && height > 0) list.push({ x, y, width, height });
  };
  for (const region of document.querySelectorAll(REGIONS)) {
    if (region.matches(HOLES) || region.parentElement?.closest(HOLES)) continue;
    add(measured.regions, region);
    region.querySelectorAll(HOLES).forEach((hole) => add(measured.holes, hole));
  }
  return measured;
}

/** macOS: a native view over the page takes presses in the regions, so
 * they're measured and sent whenever the layout may have moved them. */
function sendDragRegions(first: DragRegions): void {
  let sent = JSON.stringify(first);
  let scheduled = false;
  let watched: Element[] = [];
  const resizes = new ResizeObserver(schedule);
  function send() {
    scheduled = false;
    const regions = [...document.querySelectorAll(REGIONS)];
    if (regions.length !== watched.length || regions.some((region, i) => region !== watched[i])) {
      watched.forEach((region) => resizes.unobserve(region));
      regions.forEach((region) => resizes.observe(region));
      watched = regions;
    }
    const measured = measureDragRegions();
    const json = JSON.stringify(measured);
    if (json === sent) return;
    sent = json;
    invoke("window_set_drag_regions", { ...measured }).catch(() => {});
  }
  // A microtask, not requestAnimationFrame: WebKit pauses frames while
  // the window is covered, and the regions must still follow the layout.
  function schedule() {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(send);
  }
  new MutationObserver(schedule).observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["class", "style", "hidden", DRAG_REGION, "contenteditable", "tabindex", "role"]
  });
  addEventListener("resize", schedule);
  addEventListener("scroll", schedule, { capture: true, passive: true });
  schedule();
}

/** Windows and Linux start the drag from the page's own mousedown. */
function listenForDrags(): void {
  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || !(event.detail === 1 || event.detail === 2) || !inDragRegion(event)) return;
    event.preventDefault();
    const cmd = event.detail === 2 ? "window_title_bar_double_click" : "window_start_drag";
    invoke(cmd).catch(() => {});
  });
}

async function handleDragRegions(): Promise<void> {
  const first = measureDragRegions();
  try {
    if (await invoke<boolean>("window_set_drag_regions", { ...first })) sendDragRegions(first);
    else listenForDrags();
  } catch {
    // Outside a Chain window: nothing to drag.
  }
}

async function showChrome(): Promise<void> {
  try {
    showInsets(await invoke<TitleBarInsets>("window_insets"));
    showFullScreen(await invoke<boolean>("window_is_full_screen"));
  } catch {
    // Outside a Chain window: nothing to show.
  }
}

if (isTauri() && typeof document !== "undefined") {
  subscribe<TitleBarInsets>(INSETS_EVENT, showInsets);
  subscribe<boolean>(FULL_SCREEN_EVENT, showFullScreen);
  void showChrome();
  // The second frame callback runs once the first frame is drawn: what
  // showWhen "firstPaint" waits for.
  requestAnimationFrame(() => requestAnimationFrame(() => void invoke("window_page_painted").catch(() => {})));
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => void handleDragRegions(), { once: true });
  } else {
    void handleDragRegions();
  }
}
