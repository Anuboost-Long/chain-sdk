/**
 * Structural contract for the KeepAwake capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface KeepAwakeOptions {
  /** Keep the display on too, not just the system. Default `true`. */
  display?: boolean;
}

export interface KeepAwakeStatus {
  reason: string;
  display: boolean;
}

export interface KeepAwakeApi {
  /**
   * Keeps the computer from idle-sleeping until `stop()`, the app quits or
   * it crashes. `reason` is shown by the OS (`pmset -g assertions`). One per
   * app: calling again replaces the reason and options.
   */
  start(reason: string, options?: KeepAwakeOptions): Promise<void>;
  /** Idempotent. */
  stop(): Promise<void>;
  /** What's held now, or `null`. */
  status(): Promise<KeepAwakeStatus | null>;
}
