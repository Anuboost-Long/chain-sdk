import "server-only";

import fs from "node:fs";
import path from "node:path";

// Read straight from the repo's capabilities/ folder at build time, so the
// site's API blocks and platform-support lines can't drift from the code.
const capabilitiesDir = path.join(process.cwd(), "..", "capabilities");

export type SupportLevel = "tested" | "experimental" | "partial" | "not-started" | "not-planned";

export interface CapabilityMeta {
  name: string;
  description: string;
  status: "draft" | "experimental" | "stable";
  platforms: Record<"macos" | "windows" | "linux", SupportLevel>;
}

export function capabilityNames(): string[] {
  return fs
    .readdirSync(capabilitiesDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);
}

export function readCapability(name: string): CapabilityMeta {
  const raw = fs.readFileSync(path.join(capabilitiesDir, name, "component.json"), "utf8");
  return JSON.parse(raw) as CapabilityMeta;
}

/** contract.ts minus its header doc comment, which points at repo-internal files. */
export function readContractSource(name: string): string {
  const raw = fs.readFileSync(path.join(capabilitiesDir, name, "contract.ts"), "utf8");
  return raw.replace(/^\/\*\*[\s\S]*?\*\/\s*/, "").trim();
}
