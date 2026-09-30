import fs from "node:fs";
import path from "node:path";

/**
 * Cargo features an app turns on from its own package.json. Today only
 * one: `"chain": { "gpl": true }` builds chain-core with text-to-speech,
 * which links espeak-ng (GPL-3.0) — so the app's releases take on GPL-3.0
 * obligations. See agent-docs/capabilities/models/research/LICENSING.md.
 */
export function chainCoreFeatures(cwd: string): string[] {
  const pkg = JSON.parse(fs.readFileSync(path.join(cwd, "package.json"), "utf8")) as {
    chain?: { gpl?: unknown };
  };
  const gpl = pkg.chain?.gpl;
  if (gpl !== undefined && typeof gpl !== "boolean") {
    throw new Error('package.json "chain.gpl" must be true or false.');
  }
  return gpl ? ["chain-core/tts"] : [];
}

export function chainCoreFeaturesOrExit(cwd: string): string[] {
  try {
    return chainCoreFeatures(cwd);
  } catch (error) {
    console.error(`Error: ${(error as Error).message}`);
    process.exit(1);
  }
}
