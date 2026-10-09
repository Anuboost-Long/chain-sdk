/**
 * Structural contract for the Ports capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface PortsApi {
  /**
   * `true` when a local server could bind this TCP port right now on every
   * address a dev server commonly uses (`0.0.0.0`, `127.0.0.1`, `::`, `::1`).
   * Rejects `INVALID_ARGUMENT` unless `port` is an integer from 1 to 65535.
   */
  isFree(port: number): Promise<boolean>;
}
