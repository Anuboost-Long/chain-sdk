/**
 * Structural contract for the Packages capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface PackageManifest {
  /** App-chosen, 1–64 of a–z, 0–9 and "-", e.g. "sonarlint". */
  id: string;
  /** https only. */
  url: string;
  /** Of the downloaded file, hex. Checked before anything is kept. */
  sha256: string;
  /** Omitted: inferred from the URL's extension, else "none". A `.vsix` needs "zip". */
  archive?: "zip" | "tar.gz" | "tar.bz2" | "none";
  /** Used for progress when the server sends no Content-Length. */
  sizeBytes?: number;
}

export interface InstalledPackage {
  id: string;
  /** Absolute; laid out exactly as the archive is. */
  path: string;
  /** On disk, after unpacking. */
  sizeBytes: number;
  /** ISO 8601 UTC. */
  installedAt: string;
}

export type PackageInstallProgress = (received: number, total: number | null) => void;

export interface PackagesApi {
  install(manifest: PackageManifest, onProgress?: PackageInstallProgress): Promise<InstalledPackage>;
  /** Absolute path of `file` inside package `id`; rejects `NOT_FOUND` when either isn't there. */
  resolve(id: string, file: string): Promise<string>;
  /** The running install of `id` rejects `CANCELLED`. No-op when none is running. */
  cancel(id: string): Promise<void>;
  list(): Promise<InstalledPackage[]>;
  /** Idempotent. Cancels an install of `id` that's still running. */
  remove(id: string): Promise<void>;
}
