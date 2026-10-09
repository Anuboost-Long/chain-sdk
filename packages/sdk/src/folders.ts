import { listen } from "@tauri-apps/api/event";

import { invoke, isTauri } from "./native";
import type {
  AppFolderKind,
  DeleteOptions,
  FolderChange,
  FolderChangeHandler,
  FolderEntry,
  FolderGrant,
  FoldersApi,
  FoldersUnsubscribe,
  FolderWatch,
  ListOptions,
  PickFoldersOptions,
  ReadBytesOptions,
  ReadTextOptions,
  WatchOptions
} from "./contracts/folders";
import { chainError, type ChainErrorCode } from "./errors";

// Native errors arrive as "CODE: message" — see templates/folders.rs.
const CODES: ChainErrorCode[] = [
  "INVALID_ARGUMENT",
  "NOT_GRANTED",
  "PERMISSION_DENIED",
  "NOT_FOUND",
  "UNAVAILABLE",
  "TOO_LARGE",
  "UNSUPPORTED"
];

async function call<T>(method: string, cmd: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.folders.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : `folders.${method}() failed`;
    const code = CODES.find((candidate) => message.startsWith(`${candidate}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

// One shared listener per event, multiplexed by id, set up on first use.
const watchHandlers = new Map<string, FolderChangeHandler>();
let changeListener: Promise<unknown> | null = null;

function ensureChangeListener(): Promise<unknown> {
  changeListener ??= listen<{ id: string; changes: FolderChange[] }>("chain://folders-change", (event) => {
    watchHandlers.get(event.payload.id)?.(event.payload.changes);
  });
  return changeListener;
}

const dropHandlers = new Set<(grants: FolderGrant[]) => void>();
let stopDropListener: Promise<() => void> | null = null;

function newId(): string {
  return crypto.randomUUID();
}

export const folders: FoldersApi = {
  pick(options?: PickFoldersOptions) {
    return call<FolderGrant[]>("pick", "folders_pick", { multiple: options?.multiple, files: options?.files });
  },

  grants() {
    return call<FolderGrant[]>("grants", "folders_grants");
  },

  appFolder(kind: AppFolderKind) {
    return call<FolderGrant>("appFolder", "folders_app_folder", { folder: kind });
  },

  revoke(id: string) {
    return call<void>("revoke", "folders_revoke", { id });
  },

  onDrop(handler: (grants: FolderGrant[]) => void): FoldersUnsubscribe {
    if (!isTauri()) return () => {};
    dropHandlers.add(handler);
    if (dropHandlers.size === 1) {
      stopDropListener = listen<FolderGrant[]>("chain://folders-dropped", (event) => {
        for (const each of dropHandlers) each(event.payload);
      });
      void invoke("folders_accept_drops", { accept: true });
    }
    return () => {
      if (!dropHandlers.delete(handler) || dropHandlers.size > 0) return;
      void invoke("folders_accept_drops", { accept: false });
      void stopDropListener?.then((stop) => stop());
      stopDropListener = null;
    };
  },

  list(path: string, options?: ListOptions) {
    return call<FolderEntry[]>("list", "folders_list", {
      path,
      recursive: options?.recursive,
      skipFolders: options?.skipFolders
    });
  },

  stat(path: string) {
    return call<FolderEntry>("stat", "folders_stat", { path });
  },

  exists(path: string) {
    return call<boolean>("exists", "folders_exists", { path });
  },

  readText(path: string, options?: ReadTextOptions) {
    return call<string>("readText", "folders_read_text", { path, maxBytes: options?.maxBytes });
  },

  async readBytes(path: string, options?: ReadBytesOptions) {
    const bytes = await call<ArrayBuffer>("readBytes", "folders_read_bytes", {
      path,
      offset: options?.offset,
      length: options?.length,
      maxBytes: options?.maxBytes
    });
    return new Uint8Array(bytes);
  },

  writeText(path: string, text: string) {
    return call<void>("writeText", "folders_write_text", { path, text });
  },

  // One raw body — u32 little-endian path length, the path, the bytes —
  // since a JSON number array would quadruple the payload.
  writeBytes(path: string, bytes: Uint8Array) {
    const encodedPath = new TextEncoder().encode(path);
    const body = new Uint8Array(4 + encodedPath.length + bytes.length);
    new DataView(body.buffer).setUint32(0, encodedPath.length, true);
    body.set(encodedPath, 4);
    body.set(bytes, 4 + encodedPath.length);
    return call<void>("writeBytes", "folders_write_bytes", body);
  },

  createFolder(path: string) {
    return call<void>("createFolder", "folders_create_folder", { path });
  },

  move(from: string, to: string) {
    return call<void>("move", "folders_move", { from, to });
  },

  delete(path: string, options?: DeleteOptions) {
    return call<void>("delete", "folders_delete", { path, toTrash: options?.toTrash });
  },

  async watch(path: string, onChange: FolderChangeHandler, options?: WatchOptions): Promise<FolderWatch> {
    // Registered before native starts watching, so no early batch is lost.
    const id = newId();
    if (isTauri()) await ensureChangeListener();
    watchHandlers.set(id, onChange);
    try {
      await call<void>("watch", "folders_watch", { id, path, recursive: options?.recursive });
    } catch (error) {
      watchHandlers.delete(id);
      throw error;
    }
    return {
      async stop() {
        if (!watchHandlers.delete(id)) return;
        await call<void>("watch", "folders_unwatch", { id });
      }
    };
  }
};
