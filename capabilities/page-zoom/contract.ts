/**
 * Structural contract for the PageZoom capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface PageZoomApi {
  /**
   * Zooms the calling window's page like a browser's page zoom: layout
   * reflows at the new size and canvases stay sharp. `1` is 100%.
   * Rejects `INVALID_ARGUMENT` outside 0.25–5.
   */
  set(factor: number): Promise<void>;
  /** The factor last set for this window's page; `1` if never set. */
  get(): Promise<number>;
}
