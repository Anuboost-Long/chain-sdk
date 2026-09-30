/**
 * Structural contract for the Models capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface ModelManifest {
  /** App-chosen, 1–64 of a–z, 0–9 and "-", e.g. "moonshine-tiny-en". */
  id: string;
  /** https only. */
  url: string;
  /** Of the downloaded file, hex. Checked before anything is kept. */
  sha256: string;
  /** Omitted: inferred from the URL's extension, else "none". */
  archive?: "tar.bz2" | "tar.gz" | "zip" | "none";
  /** Used for progress when the server sends no Content-Length. */
  sizeBytes?: number;
}

export interface InstalledModel {
  id: string;
  /** On disk, after extraction. */
  sizeBytes: number;
  /** ISO 8601 UTC. */
  installedAt: string;
}

export type ModelInstallProgress = (received: number, total: number | null) => void;

export interface ModelsApi {
  install(manifest: ModelManifest, onProgress?: ModelInstallProgress): Promise<InstalledModel>;
  /** The running install of `id` rejects `CANCELLED`. No-op when none is running. */
  cancel(id: string): Promise<void>;
  list(): Promise<InstalledModel[]>;
  /** Idempotent. Cancels an install of `id` that's still running. */
  remove(id: string): Promise<void>;
}
