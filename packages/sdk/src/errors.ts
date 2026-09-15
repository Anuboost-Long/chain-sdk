/**
 * Common error model shared by every Chain capability.
 * See docs/MNEME_DESKTOP_FRAMEWORK.md section 22.
 */

export type ChainErrorCode =
  | "UNSUPPORTED"
  | "PERMISSION_DENIED"
  | "NOT_FOUND"
  | "UNAVAILABLE"
  | "INVALID_ARGUMENT"
  | "CANCELLED"
  | "NATIVE_FAILURE";

export interface ChainError {
  code: ChainErrorCode;
  message: string;
  nativeCode?: string;
  debugDetails?: unknown;
}

export function chainError(code: ChainErrorCode, message: string): ChainError {
  return { code, message };
}
