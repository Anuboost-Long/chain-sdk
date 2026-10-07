import { convertFileSrc, invoke, isTauri } from "./native";

import type { FilesApi, PickOptions, PickedFile, SaveOptions, SavedFile } from "./contracts/files";
import { chainError } from "./errors";

const NOT_FOUND_PREFIX = "NOT_FOUND: ";
const INVALID_ARGUMENT_PREFIX = "INVALID_ARGUMENT: ";
const UNAVAILABLE_PREFIX = "UNAVAILABLE: ";
const UNSUPPORTED_PREFIX = "UNSUPPORTED: ";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.files.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    if (typeof error === "string" && error.startsWith(NOT_FOUND_PREFIX)) {
      throw chainError("NOT_FOUND", error.slice(NOT_FOUND_PREFIX.length));
    }
    if (typeof error === "string" && error.startsWith(INVALID_ARGUMENT_PREFIX)) {
      throw chainError("INVALID_ARGUMENT", error.slice(INVALID_ARGUMENT_PREFIX.length));
    }
    if (typeof error === "string" && error.startsWith(UNAVAILABLE_PREFIX)) {
      throw chainError("UNAVAILABLE", error.slice(UNAVAILABLE_PREFIX.length));
    }
    if (typeof error === "string" && error.startsWith(UNSUPPORTED_PREFIX)) {
      throw chainError("UNSUPPORTED", error.slice(UNSUPPORTED_PREFIX.length));
    }
    throw chainError(
      "NATIVE_FAILURE",
      typeof error === "string" ? error : "files operation failed"
    );
  }
}

export const files: FilesApi = {
  async write(bytes: Uint8Array, options?: { extension?: string }): Promise<string> {
    requireTauri("write");
    // Vec<u8> deserializes from a JSON array of numbers — a raw
    // Uint8Array serializes as {"0":1,"1":2,...} instead, so this
    // conversion is required, not cosmetic (confirmed empirically).
    return call<string>("files_write", { bytes: Array.from(bytes), extension: options?.extension });
  },

  async read(reference: string): Promise<Uint8Array> {
    requireTauri("read");
    const bytes = await call<number[]>("files_read", { reference });
    return new Uint8Array(bytes);
  },

  async url(reference: string): Promise<string> {
    requireTauri("url");
    const path = await call<string>("files_resolve_path", { reference });
    return convertFileSrc(path);
  },

  async delete(reference: string): Promise<void> {
    requireTauri("delete");
    return call<void>("files_delete", { reference });
  },

  async open(reference: string): Promise<void> {
    requireTauri("open");
    return call<void>("files_open", { reference });
  },

  async reveal(reference: string): Promise<void> {
    requireTauri("reveal");
    return call<void>("files_reveal", { reference });
  },

  async pick(options?: PickOptions): Promise<PickedFile[]> {
    requireTauri("pick");
    const response = await call<ArrayBuffer>("files_pick", {
      multiple: options?.multiple,
      extensions: options?.extensions
    });
    return decodePicked(response);
  },

  async save(bytes: Uint8Array, options?: SaveOptions): Promise<SavedFile | null> {
    requireTauri("save");
    const name = await call<string | null>("files_save", {
      bytes: Array.from(bytes),
      suggestedName: options?.suggestedName,
      extensions: options?.extensions
    });
    return name === null ? null : { name };
  }
};

// files_pick replies with raw bytes (see templates/lib.rs): a u32
// little-endian header length, a JSON `[{ name, size }]` header, then
// every file's bytes back to back in the same order.
function decodePicked(response: ArrayBuffer): PickedFile[] {
  const headerLength = new DataView(response).getUint32(0, true);
  const header = JSON.parse(
    new TextDecoder().decode(new Uint8Array(response, 4, headerLength))
  ) as { name: string; size: number }[];
  let offset = 4 + headerLength;
  return header.map(({ name, size }) => {
    const bytes = new Uint8Array(response.slice(offset, offset + size));
    offset += size;
    return { name, size, bytes };
  });
}
