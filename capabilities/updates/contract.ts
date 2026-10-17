/**
 * Structural contract for the Updates capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** Why this run of the app can't update itself. */
export type UpdatesUnsupportedReason =
  /** package.json has no "chain.updates". */
  | "not-configured"
  /** A `chain dev` run: there's no built app to replace. */
  | "dev-build"
  /** This user can't replace the app where it's installed. */
  | "read-only"
  /** Not built yet for this OS. */
  | "unsupported-platform";

export interface UpdatesStatus {
  supported: boolean;
  /** Set when `supported` is false. */
  reason?: UpdatesUnsupportedReason;
  /** Why, in words an app can show. Set when `supported` is false. */
  message?: string;
  /** The running app's version. */
  currentVersion: string;
  /** A downloaded update's version, waiting for installAndRelaunch() or the next quit. */
  downloaded?: string;
}

/** A newer version the feed offers. */
export interface AvailableUpdate {
  version: string;
  /** ISO 8601, when the feed gives one. */
  date?: string;
  /** Release notes as the feed has them (often Markdown). */
  notes?: string;
}

export interface UpdateProgress {
  /** Bytes so far. */
  downloaded: number;
  /** Null when the server doesn't say. */
  total: number | null;
}

export type UpdatesUnsubscribe = () => void;

export interface UpdatesApi {
  /** Never rejects. */
  status(): Promise<UpdatesStatus>;
  /** Asks the feed; downloads nothing. Null when the running version is the newest. */
  check(): Promise<AvailableUpdate | null>;
  /** Downloads and verifies the update the last check() found. */
  download(): Promise<void>;
  /** Replaces the app with the downloaded update and starts it again. Doesn't resolve when it works. */
  installAndRelaunch(): Promise<void>;
  /** Progress of the running download(). */
  onProgress(listener: (progress: UpdateProgress) => void): UpdatesUnsubscribe;
}
