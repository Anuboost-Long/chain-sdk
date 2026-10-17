/**
 * Structural contract for the ProcessRunner capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface ProcessOutputChunk {
  stream: "stdout" | "stderr";
  /** UTF-8; a character split across reads is never broken (see CONTRACT.md). */
  data: string;
}

/** With `output: "bytes"`: exactly the bytes read, undecoded. */
export interface ProcessByteChunk {
  stream: "stdout" | "stderr";
  data: Uint8Array;
}

/**
 * One argv element. A `{ fileReference }` is a `desktop.files` reference;
 * native replaces it with that managed file's absolute path as exactly one
 * argument — JS never sees the path (see CONTRACT.md).
 */
export type ProcessArg = string | { fileReference: string };

export type ProcessOutputHandler = (chunk: ProcessOutputChunk) => void;

export type ProcessByteHandler = (chunk: ProcessByteChunk) => void;

export interface ProcessExit {
  /** The real exit code, or `null` if the process was killed or exited via a signal with no exit code (see CONTRACT.md). */
  code: number | null;
  /** `true` if `kill()` caused this exit. */
  killed: boolean;
}

export interface ProcessHandle {
  readonly id: string;
  /** Stops the process and everything it started; rejects `TIMEOUT` if any of it outlives 5 s (see CONTRACT.md). */
  kill(): Promise<void>;
  /** Writes to stdin while it runs. Needs `keepStdinOpen`; rejects `UNAVAILABLE` once stdin is closed or the process exited. */
  write(text: string): Promise<void>;
  /** Closes stdin, so the process sees EOF. Idempotent. */
  closeStdin(): Promise<void>;
  /** Resolves once the process exits, however it exits — never rejects (see CONTRACT.md). */
  readonly exited: Promise<ProcessExit>;
}

export interface ProcessRunOptions {
  /** `"bytes"`: `onOutput` gets `Uint8Array` chunks (see CONTRACT.md). Default `"text"`. */
  output?: "text" | "bytes";
  /** Written to the process's stdin as UTF-8, then stdin is closed (EOF) unless `keepStdinOpen`. Omitted: the process gets no stdin at all (see CONTRACT.md). */
  stdin?: string;
  /** Keep stdin open for `handle.write()` until `handle.closeStdin()`. Default `false`. */
  keepStdinOpen?: boolean;
  /** Absolute path of an existing folder inside a `desktop.folders` grant. Default: the app's own working directory. */
  cwd?: string;
  /** Set over the inherited environment (which includes the login-shell `PATH`); a `PATH` here wins. */
  env?: Record<string, string>;
}

export interface ProcessRunnerApi {
  run(
    command: string,
    args: ProcessArg[],
    onOutput: ProcessOutputHandler,
    options?: ProcessRunOptions & { output?: "text" }
  ): Promise<ProcessHandle>;
  run(
    command: string,
    args: ProcessArg[],
    onOutput: ProcessByteHandler,
    options: ProcessRunOptions & { output: "bytes" }
  ): Promise<ProcessHandle>;
  /** Starts a program the app never stops — not at quit, not on a crash (see CONTRACT.md). Resolves once started. */
  launch(command: string, args: string[]): Promise<void>;
}
