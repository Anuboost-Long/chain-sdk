/**
 * Structural contract for the Window capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/**
 * - `standard`: the system title bar.
 * - `transparent`: the system bar, drawn in the window's background colour.
 * - `overlay`: the page fills the whole window; the window buttons sit over it.
 * - `hidden`: no title bar and no window buttons.
 */
export type TitleBarStyle = "standard" | "transparent" | "overlay" | "hidden";

/**
 * The bar heights macOS lays out itself, window buttons included — the
 * native choice for an app bar the buttons sit on:
 * - `standard`: the system title bar (32 px on macOS 27).
 * - `medium`: a compact toolbar's bar (40 px on macOS 27).
 * - `large`: a toolbar's bar (52 px on macOS 27).
 * Heights change between macOS versions; read insets() rather than hardcoding them.
 */
export type TitleBarSize = "standard" | "medium" | "large";

/** The title bar's light/dark look. The page's `prefers-color-scheme` follows it. */
export type WindowAppearance = "system" | "light" | "dark";

/** Where the window buttons sit, in CSS pixels from the window's top-left corner. */
export interface WindowButtonsPosition {
  /** From the window's left edge to the first button. */
  left: number;
  /** From the window's top edge to the buttons' vertical centre — half an app bar's height centres them on it. */
  centerY: number;
}

export interface WindowButtonsOptions {
  /** Each defaults to true. */
  close?: boolean | null;
  minimize?: boolean | null;
  zoom?: boolean | null;
  /** Defaults to where AppKit puts them for the bar's size. Prefer titleBarSize for a native look; this is for an exact position. */
  position?: WindowButtonsPosition | null;
}

/**
 * The window's chrome. The same shape goes in the app's package.json,
 * under `"chain": { "window": { ... } }`, for the first frame. Everywhere,
 * null puts a field back to its default.
 */
export interface WindowOptions {
  /** Defaults to "standard". */
  titleBarStyle?: TitleBarStyle | null;
  /** Defaults to "standard". */
  titleBarSize?: TitleBarSize | null;
  /** The window title in the bar. Defaults to shown for standard and transparent, hidden for overlay; never shown with hidden. */
  titleVisible?: boolean | null;
  windowButtons?: WindowButtonsOptions;
  /** Defaults to "system". */
  appearance?: WindowAppearance | null;
  /** `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. Defaults to the OS default. */
  backgroundColor?: string | null;
}

/**
 * When the window first appears at launch:
 * - `immediately`: as soon as the app starts, before the page has drawn anything.
 * - `firstPaint`: once the page has drawn its first frame and loaded @chain/sdk.
 * - `showCalled`: when the page calls show().
 * Either way it shows when showTimeout runs out. Only the first appearance:
 * reloads never hide the window again.
 */
export type ShowWhen = "immediately" | "firstPaint" | "showCalled";

/**
 * package.json's `"chain": { "window": { ... } }`: the window's chrome for
 * the first frame, plus when the window first shows — which only applies
 * at startup, so setOptions() doesn't take these two.
 */
export interface WindowStartupOptions extends WindowOptions {
  /** Defaults to "immediately". A platform that can't do the mode shows immediately. */
  showWhen?: ShowWhen | null;
  /** Milliseconds after which the window shows whatever showWhen says. Defaults to 3000. */
  showTimeout?: number | null;
}

/** The options in effect — after defaults, and after options this platform can't do fell back. */
export interface ResolvedWindowOptions {
  titleBarStyle: TitleBarStyle;
  titleBarSize: TitleBarSize;
  titleVisible: boolean;
  windowButtons: {
    close: boolean;
    minimize: boolean;
    zoom: boolean;
    position: WindowButtonsPosition | null;
  };
  appearance: WindowAppearance;
  /** `#rrggbbaa`, or null for the OS default. */
  backgroundColor: string | null;
}

export interface WindowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The system chrome drawn over the page, in CSS pixels from the page's top-left. */
export interface TitleBarInsets {
  /** Height of the system title bar over the page: the bar's height (titleBarSize included) with `overlay`, else 0 — and 0 in full screen. */
  height: number;
  /** From the page's left edge to the right edge of window buttons over the page; 0 when none are. */
  left: number;
  /** From the page's right edge to the left edge of window buttons over the page (Windows' side); 0 when none are. */
  right: number;
  /** The visible window buttons over the page, or null when none are (standard bar, hidden, full screen). */
  windowButtons: WindowRect | null;
}

/** Which options work here. Options that don't are accepted and have no effect. */
export interface WindowAvailability {
  /** Whether this is a Chain window at all. Every flag below is false when this is. */
  available: boolean;
  titleBarStyles: Record<TitleBarStyle, boolean>;
  /** The medium and large sizes. */
  titleBarSize: boolean;
  titleVisible: boolean;
  windowButtonsPosition: boolean;
  /** Hiding window buttons one by one. */
  windowButtonsVisibility: boolean;
  appearance: boolean;
  backgroundColor: boolean;
  /** `data-chain-drag-region` and double-click on it. */
  dragRegions: boolean;
  startDrag: boolean;
  /** insets() reports where the system chrome is (otherwise it's all zero). */
  titleBarInsets: boolean;
  fullScreen: boolean;
  /** Which showWhen modes work here; the others show immediately. */
  showWhen: Record<ShowWhen, boolean>;
}

export type WindowUnsubscribe = () => void;

export interface WindowApi {
  availability(): Promise<WindowAvailability>;
  /** The options in effect for this window. */
  options(): Promise<ResolvedWindowOptions>;
  /** Changes the options given and keeps the rest; `windowButtons` merges field by field. Lasts until the app quits. */
  setOptions(options: WindowOptions): Promise<void>;
  insets(): Promise<TitleBarInsets>;
  /** Whenever insets() would answer differently: style or button changes, entering or leaving full screen. */
  onInsetsChange(listener: (insets: TitleBarInsets) => void): WindowUnsubscribe;
  isFullScreen(): Promise<boolean>;
  onFullScreenChange(listener: (fullScreen: boolean) => void): WindowUnsubscribe;
  /** Shows the window if it hasn't appeared yet; otherwise does nothing. Safe to call any time. */
  show(): Promise<void>;
  /** Whether the window has appeared yet. Stays true once it has (minimising doesn't change it). */
  isShown(): Promise<boolean>;
  /** Once, when the window first appears. Check isShown() first: it doesn't fire for a window already shown. */
  onShown(listener: () => void): WindowUnsubscribe;
  /** Moves the window with the pointer, as dragging the title bar does. Call it from a primary-button pointerdown/mousedown. */
  startDrag(): Promise<void>;
}
