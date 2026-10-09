/**
 * Structural contract for the Folders capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** `readOnly` grants come only from the app's declared folders. */
export type FolderAccess = "readWrite" | "readOnly";

/** `app`: one of the app's own folders (`appFolder()`). */
export type FolderGrantSource = "picked" | "dropped" | "declared" | "app";

/** `data` persists across restarts and updates; the OS may clear `temp`. Both are private to the app. */
export type AppFolderKind = "data" | "temp";

export interface FolderGrant {
  /** Stable across restarts. A declared folder's id is `declared:` plus the path as declared; app folders are `app:data` and `app:temp`. */
  id: string;
  /** Absolute and canonical: symlinks resolved, the case as stored on disk. */
  path: string;
  /** A picked or dropped file grants that one file, not its folder. */
  kind: "folder" | "file";
  access: FolderAccess;
  source: FolderGrantSource;
}

export interface PickFoldersOptions {
  /** Let the user choose more than one. Default `false`. */
  multiple?: boolean;
  /** Let the user choose files as well as folders. Default `false`. */
  files?: boolean;
}

export type FolderEntryKind = "file" | "folder" | "symlink" | "other";

export interface FolderEntry {
  /** Absolute path, under the canonical form of the folder that was listed. */
  path: string;
  name: string;
  /** A symlink is reported as itself, never as what it points to. */
  kind: FolderEntryKind;
  /** Bytes. `0` for folders. */
  size: number;
  /** Milliseconds since the Unix epoch. */
  modifiedMs: number;
}

export interface ListOptions {
  /** Walk every subfolder too. Symlinked folders are listed but never entered. Default `false`. */
  recursive?: boolean;
  /** Folder names (exact, case-sensitive) that a recursive listing reports but does not enter, e.g. `node_modules`. */
  skipFolders?: string[];
}

export interface ReadTextOptions {
  /** Reject with `TOO_LARGE` instead of reading a file bigger than this. */
  maxBytes?: number;
}

export interface ReadBytesOptions {
  /** Byte offset to start reading at. Default `0`. Past the end reads nothing. */
  offset?: number;
  /** Read at most this many bytes from `offset`. Default: to the end of the file. */
  length?: number;
  /** Reject with `TOO_LARGE` when the bytes to read would exceed this. */
  maxBytes?: number;
}

export interface DeleteOptions {
  /** Move to the Trash instead of deleting permanently. Default `false`. */
  toTrash?: boolean;
}

export interface WatchOptions {
  /** Also report changes in every subfolder. Default `false`. */
  recursive?: boolean;
}

/**
 * `rescan`: the OS dropped events (overflow, or it coalesced too much to
 * say what changed). Anything under `path` may have changed.
 */
export type FolderChangeKind = "created" | "modified" | "removed" | "renamed" | "other" | "rescan";

export interface FolderChange {
  path: string;
  kind: FolderChangeKind;
}

export type FolderChangeHandler = (changes: FolderChange[]) => void;

export interface FolderWatch {
  /** Releases the OS watcher. Idempotent. */
  stop(): Promise<void>;
}

export type FoldersUnsubscribe = () => void;

export interface FoldersApi {
  /** Shows the OS folder picker and grants what the user chose. `[]` on cancel. */
  pick(options?: PickFoldersOptions): Promise<FolderGrant[]>;
  /** Every grant the user or the build made: picked, dropped and declared. App folders aren't listed. */
  grants(): Promise<FolderGrant[]>;
  /** One of the app's own read-write folders, created if missing. Its path can go into process arguments and `cwd`. */
  appFolder(kind: AppFolderKind): Promise<FolderGrant>;
  /** Forgets a picked or dropped grant. Idempotent. Declared grants can't be revoked. */
  revoke(id: string): Promise<void>;
  /** Grants whatever the user drops onto the window, while at least one handler is subscribed. */
  onDrop(handler: (grants: FolderGrant[]) => void): FoldersUnsubscribe;

  list(path: string, options?: ListOptions): Promise<FolderEntry[]>;
  stat(path: string): Promise<FolderEntry>;
  exists(path: string): Promise<boolean>;
  readText(path: string, options?: ReadTextOptions): Promise<string>;
  readBytes(path: string, options?: ReadBytesOptions): Promise<Uint8Array>;

  /** Atomic: the file is either the old content or the new, never half-written. */
  writeText(path: string, text: string): Promise<void>;
  /** Atomic, like `writeText`. */
  writeBytes(path: string, bytes: Uint8Array): Promise<void>;
  /** Creates the folder and any missing parents. Already existing is fine. */
  createFolder(path: string): Promise<void>;
  /** Renames or moves a file or folder. Fails if `to` already exists. */
  move(from: string, to: string): Promise<void>;
  /** Deletes a file, a symlink (never its target) or a folder with everything in it. Idempotent. */
  delete(path: string, options?: DeleteOptions): Promise<void>;

  watch(path: string, onChange: FolderChangeHandler, options?: WatchOptions): Promise<FolderWatch>;
}
