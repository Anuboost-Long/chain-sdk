/**
 * Structural contract for the Files capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface PickedFile {
  /** The file's own name, e.g. "report.pdf" — never its path. */
  name: string;
  /** Size in bytes. */
  size: number;
  bytes: Uint8Array;
}

export interface PickOptions {
  /** Allow selecting more than one file. Default false. */
  multiple?: boolean;
  /** Only show files with these extensions (letters/digits, no leading dot). Omitted or empty: any file. */
  extensions?: string[];
}

export interface SaveOptions {
  /** The name the panel starts with, e.g. "backup-2026-09-28.json". A file name only, never a path. */
  suggestedName?: string;
  /** Allowed extensions (letters/digits, no leading dot); the first is appended if the user's name has none of them. */
  extensions?: string[];
}

export interface SavedFile {
  /** The name the user settled on — never its path. */
  name: string;
}

export interface FilesApi {
  write(bytes: Uint8Array, options?: { extension?: string }): Promise<string>;
  read(reference: string): Promise<Uint8Array>;
  url(reference: string): Promise<string>;
  delete(reference: string): Promise<void>;
  /** Resolves `[]` when the user cancels. */
  pick(options?: PickOptions): Promise<PickedFile[]>;
  /** Always shows the save panel; resolves `null` when the user cancels. */
  save(bytes: Uint8Array, options?: SaveOptions): Promise<SavedFile | null>;
}
