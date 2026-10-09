/**
 * Structural contract for the Terminal capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface StartTerminalOptions {
  /** Program name (looked up on the login-shell `PATH`) or absolute path. Never a shell string. */
  command: string;
  args?: string[];
  /** Absolute path of an existing folder inside a `desktop.folders` grant. */
  cwd?: string;
  /** Set over the inherited environment, the login-shell `PATH` and `TERM=xterm-256color`. */
  env?: Record<string, string>;
  /** Default 80. */
  cols?: number;
  /** Default 24. */
  rows?: number;
  /** Shown back in `list()`; Chain gives it no meaning. */
  label?: string;
  /** The app's own identity for the session (project, agent, ...), shown back in `list()`. */
  metadata?: Record<string, string>;
  /** Output kept for `backlog()`, in bytes of text. Default 524288 (512 KB). */
  backlogBytes?: number;
}

export interface TerminalExit {
  /** `null` when a signal ended it. */
  code: number | null;
  /** `true` if `kill()` or `remove()` caused the exit. */
  killed: boolean;
}

export interface TerminalSession {
  /** Unique for the app's lifetime, never reused. */
  id: string;
  label: string;
  metadata: Record<string, string>;
  command: string;
  args: string[];
  cwd: string | null;
  pid: number | null;
  startedAtMs: number;
  cols: number;
  rows: number;
  /** `null` while it's running. */
  exit: TerminalExit | null;
}

export interface TerminalOutput {
  sessionId: string;
  /** 1 for a session's first chunk, then +1 per chunk. */
  seq: number;
  /** UTF-8, never split inside a character. Chunk boundaries carry no meaning. */
  data: string;
}

export interface TerminalBacklog {
  data: string;
  /** The `seq` of the newest chunk included; `0` before any output. */
  seq: number;
}

export type TerminalOutputHandler = (chunk: TerminalOutput) => void;
export type TerminalExitHandler = (sessionId: string, exit: TerminalExit) => void;
export type TerminalUnsubscribe = () => void;

export interface TerminalApi {
  start(options: StartTerminalOptions): Promise<TerminalSession>;
  /** Every session not yet removed, running or exited, oldest first. */
  list(): Promise<TerminalSession[]>;
  backlog(sessionId: string): Promise<TerminalBacklog>;
  /**
   * The backlog (as one chunk carrying its `seq`) and then live output, with
   * no gap and nothing twice. Resolves once the backlog has been delivered.
   */
  attach(sessionId: string, onOutput: TerminalOutputHandler): Promise<TerminalUnsubscribe>;
  /** Live output of every session, from now on. */
  onOutput(handler: TerminalOutputHandler): TerminalUnsubscribe;
  onExit(handler: TerminalExitHandler): TerminalUnsubscribe;
  /** Keystrokes or pasted text, sent as-is (escape sequences included). */
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  /** Stops the session's whole process tree; rejects `TIMEOUT` if any of it outlives 5 s. Idempotent. */
  kill(sessionId: string): Promise<void>;
  /** Forgets the session and its backlog, killing it first if it's running. Idempotent. */
  remove(sessionId: string): Promise<void>;
}
