import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";

export { convertFileSrc, isTauri } from "@tauri-apps/api/core";

// The JS half of `chain inspect --trace` (agent-docs/framework/command/README.md).
// While a trace runs, the inspector puts `__chainTrace` on window and every
// native call the SDK makes is recorded into it, tagged with an id (the
// `chain-trace` header) the native side records too. Outside `chain dev`
// it's never there, and a call costs one property read.

interface TraceCall {
  id: number;
  cmd: string;
  /** SQL text for storage, "METHOD url" for http, else "". */
  summary: string;
  atMs: number;
  jsMs: number;
  /** JSON length of the arguments, in characters. */
  requestSize: number;
  /** JSON length of the result, or its byte length when binary. */
  responseSize: number;
  error?: string;
}

interface Trace {
  startedAt: number;
  nextId: number;
  calls: TraceCall[];
}

function summarize(args: InvokeArgs | undefined): string {
  if (!args || ArrayBuffer.isView(args) || args instanceof ArrayBuffer || Array.isArray(args)) return "";
  if (typeof args.sql === "string") return args.sql.replace(/\s+/g, " ").trim().slice(0, 300);
  const request = args.request as { method?: string; url?: string } | undefined;
  if (request?.url) return `${request.method ?? "GET"} ${request.url}`;
  return "";
}

function sizeOf(value: unknown): number {
  if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) return value.byteLength;
  return JSON.stringify(value ?? null).length;
}

export async function invoke<T>(
  cmd: string,
  args?: InvokeArgs,
  options?: { headers: Record<string, string> }
): Promise<T> {
  const trace = (globalThis as { __chainTrace?: Trace }).__chainTrace;
  if (!trace) return tauriInvoke<T>(cmd, args, options);

  const id = ++trace.nextId;
  const call: TraceCall = {
    id,
    cmd,
    summary: summarize(args),
    atMs: performance.now() - trace.startedAt,
    jsMs: 0,
    requestSize: sizeOf(args),
    responseSize: 0
  };
  const started = performance.now();
  try {
    const headers = { ...options?.headers, "chain-trace": String(id) };
    const result = await tauriInvoke<T>(cmd, args, { ...options, headers });
    call.responseSize = sizeOf(result);
    return result;
  } catch (error) {
    call.error = typeof error === "string" ? error : String(error);
    throw error;
  } finally {
    call.jsMs = performance.now() - started;
    trace.calls.push(call);
  }
}
