/**
 * Structural contract for the Platform capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export type ChainOs = "macos" | "windows";
export type ChainArch = "arm64" | "x64";

export interface PlatformInfo {
  os: ChainOs;
  arch: ChainArch;
  runtimeVersion: string;
}

export interface PlatformApi {
  getInfo(): Promise<PlatformInfo>;
}
