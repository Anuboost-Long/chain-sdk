/**
 * Structural contract for the ProcessRunner capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface ProcessOutputChunk {
  stream: "stdout" | "stderr";
  /** Decoded as UTF-8, lossily — see CONTRACT.md's Non-goals. */
  data: string;
}

/**
 * One argv element. A `{ fileReference }` is a `desktop.files` reference;
 * native replaces it with that managed file's absolute path as exactly one
 * argument — JS never sees the path (see CONTRACT.md).
 */
export type ProcessArg = string | { fileReference: string };

export type ProcessOutputHandler = (chunk: ProcessOutputChunk) => void;

export interface ProcessExit {
  /** The real exit code, or `null` if the process was killed or exited via a signal with no exit code (see CONTRACT.md). */
  code: number | null;
  /** `true` if `kill()` caused this exit. */
  killed: boolean;
}

export interface ProcessHandle {
  readonly id: string;
  kill(): Promise<void>;
  /** Resolves once the process exits, however it exits — never rejects (see CONTRACT.md). */
  readonly exited: Promise<ProcessExit>;
}

export interface ProcessRunOptions {
  /** Written to the process's stdin as UTF-8, then stdin is closed (EOF). Omitted: the process gets no stdin at all (see CONTRACT.md). */
  stdin?: string;
}

export interface ProcessRunnerApi {
  run(
    command: string,
    args: ProcessArg[],
    onOutput: ProcessOutputHandler,
    options?: ProcessRunOptions
  ): Promise<ProcessHandle>;
}
