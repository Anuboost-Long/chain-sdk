import { invoke, isTauri } from "@tauri-apps/api/core";

import type {
  HttpApi,
  HttpData,
  HttpParamValue,
  HttpRequestConfig,
  HttpResponse
} from "./contracts/http";
import { chainError, type ChainErrorCode } from "./errors";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.http.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

const PREFIXED_CODES: ChainErrorCode[] = ["INVALID_ARGUMENT", "UNAVAILABLE", "TOO_LARGE"];

type NativeResponse = Omit<HttpResponse, "data">;
type NativeBody = { kind: "text" | "json" | "form" | "bytes"; value: string };

function paramValue(value: HttpParamValue): string | null {
  if (value === null || value === undefined) return null;
  return value instanceof Date ? value.toISOString() : String(value);
}

/** axios's default serialization: `key[]=a&key[]=b` for arrays, null/undefined skipped. */
function serializeParams(params: HttpRequestConfig["params"]): [string, string][] {
  const pairs: [string, string][] = [];
  for (const [key, value] of Object.entries(params ?? {})) {
    const values = Array.isArray(value) ? value : [value];
    const name = Array.isArray(value) ? `${key}[]` : key;
    for (const item of values) {
      const text = paramValue(item);
      if (text !== null) pairs.push([name, text]);
    }
  }
  return pairs;
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

function encodeBody(data: HttpData | undefined): NativeBody | undefined {
  if (data === undefined) return undefined;
  if (typeof data === "string") return { kind: "text", value: data };
  if (data instanceof URLSearchParams) return { kind: "form", value: data.toString() };
  if (data instanceof Uint8Array) return { kind: "bytes", value: base64(data) };
  if (data instanceof ArrayBuffer) return { kind: "bytes", value: base64(new Uint8Array(data)) };
  return { kind: "json", value: JSON.stringify(data) };
}

/** Like axios: JSON responses arrive parsed; anything else (or JSON that doesn't parse) as text. */
function parseData(response: NativeResponse): unknown {
  const contentType = response.headers["content-type"] ?? "";
  if (/[/+]json\b/i.test(contentType) && response.body !== "") {
    try {
      return JSON.parse(response.body);
    } catch {
      return response.body;
    }
  }
  return response.body;
}

/** http_request_bytes's buffer: a big-endian u32 length, the head as JSON, then the body. */
function unframe(buffer: ArrayBuffer): NativeResponse & { bytes: Uint8Array } {
  const headLength = new DataView(buffer).getUint32(0);
  const head = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 4, headLength)));
  return { ...head, body: "", bytes: new Uint8Array(buffer, 4 + headLength) };
}

async function send<T>(method: string, config: HttpRequestConfig): Promise<HttpResponse<T>> {
  requireTauri(method);
  const { maxBytes } = config;
  if (maxBytes !== undefined && !(Number.isSafeInteger(maxBytes) && maxBytes >= 0)) {
    throw chainError("INVALID_ARGUMENT", `maxBytes must be a whole number of bytes, got ${maxBytes}`);
  }
  const request = {
    url: config.url,
    method: config.method,
    headers: Object.entries(config.headers ?? {}),
    params: serializeParams(config.params),
    body: encodeBody(config.data),
    timeoutMs: config.timeout,
    maxBytes
  };
  try {
    if (config.responseType === "bytes") {
      const { bytes, ...response } = unframe(await invoke<ArrayBuffer>("http_request_bytes", { request }));
      return { ...response, data: bytes as T };
    }
    const response = await invoke<NativeResponse>("http_request", { request });
    return { ...response, data: parseData(response) as T };
  } catch (error) {
    const message = typeof error === "string" ? error : "http request failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

type Config = Omit<HttpRequestConfig, "url" | "method" | "data">;

export const http: HttpApi = {
  request: (config) => send("request", config),
  get: (url, config?: Config) => send("get", { ...config, url, method: "GET" }),
  delete: (url, config?: Config) => send("delete", { ...config, url, method: "DELETE" }),
  head: (url, config?: Config) => send("head", { ...config, url, method: "HEAD" }),
  options: (url, config?: Config) => send("options", { ...config, url, method: "OPTIONS" }),
  post: (url, data?: HttpData, config?: Config) =>
    send("post", { ...config, url, method: "POST", data }),
  put: (url, data?: HttpData, config?: Config) =>
    send("put", { ...config, url, method: "PUT", data }),
  patch: (url, data?: HttpData, config?: Config) =>
    send("patch", { ...config, url, method: "PATCH", data })
};
