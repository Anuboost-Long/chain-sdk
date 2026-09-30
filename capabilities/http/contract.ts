/**
 * Structural contract for the Http capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";

export type HttpParamValue = string | number | boolean | Date | null | undefined;

/**
 * What a request body can be, and how it's sent (Content-Type in brackets,
 * used only when `headers` doesn't set one):
 * - string → as-is [text/plain;charset=utf-8]
 * - URLSearchParams → a=1&b=2 [application/x-www-form-urlencoded]
 * - Uint8Array / ArrayBuffer → raw bytes [application/octet-stream]
 * - anything else (object, array, number, boolean) → JSON [application/json]
 */
export type HttpData =
  string | URLSearchParams | Uint8Array | ArrayBuffer | object | number | boolean;

export interface HttpRequestConfig {
  url: string;
  /** Case-insensitive; defaults to GET. */
  method?: HttpMethod | Lowercase<HttpMethod>;
  headers?: Record<string, string>;
  /** Added to the URL's query. Arrays repeat as `key[]=a&key[]=b`; null/undefined are skipped. */
  params?: Record<string, HttpParamValue | HttpParamValue[]>;
  data?: HttpData;
  /** Milliseconds. Defaults to 30000. */
  timeout?: number;
  /** "bytes": `data` is the body as a Uint8Array and `body` is "". Defaults to "text". */
  responseType?: "text" | "bytes";
  /** With responseType "bytes": a longer body rejects TOO_LARGE, without being read past the limit. */
  maxBytes?: number;
}

export interface HttpResponse<T = unknown> {
  status: number;
  statusText: string;
  /** status is 200–299. A non-2xx response still resolves. */
  ok: boolean;
  /** Lowercased names; repeated headers joined with ", ". */
  headers: Record<string, string>;
  /** The raw body as text; "" with responseType "bytes". */
  body: string;
  /**
   * `body` parsed as JSON when the response says it's JSON and it parses,
   * else `body`. With responseType "bytes", the body as a Uint8Array.
   */
  data: T;
}

type Config = Omit<HttpRequestConfig, "url" | "method" | "data">;

export interface HttpApi {
  request<T = unknown>(config: HttpRequestConfig): Promise<HttpResponse<T>>;
  get<T = unknown>(url: string, config?: Config): Promise<HttpResponse<T>>;
  delete<T = unknown>(url: string, config?: Config): Promise<HttpResponse<T>>;
  head<T = unknown>(url: string, config?: Config): Promise<HttpResponse<T>>;
  options<T = unknown>(url: string, config?: Config): Promise<HttpResponse<T>>;
  post<T = unknown>(url: string, data?: HttpData, config?: Config): Promise<HttpResponse<T>>;
  put<T = unknown>(url: string, data?: HttpData, config?: Config): Promise<HttpResponse<T>>;
  patch<T = unknown>(url: string, data?: HttpData, config?: Config): Promise<HttpResponse<T>>;
}
