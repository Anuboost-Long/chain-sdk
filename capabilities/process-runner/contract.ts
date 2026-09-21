/**
 * Structural contract for the ProcessRunner capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface ProcessOutputChunk {
  stream: "stdout" | "stderr";
  /** Decoded as UTF-8, lossily — see CONTRACT.md's Non-goals. */
  data: string;
}

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

export interface ProcessRunnerApi {
  run(command: string, args: string[], onOutput: ProcessOutputHandler): Promise<ProcessHandle>;
}
