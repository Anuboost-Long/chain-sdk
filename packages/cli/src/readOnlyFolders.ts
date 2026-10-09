import fs from "node:fs";
import path from "node:path";

/**
 * Folders (or single files) the app may read without the user picking them, declared in its
 * own package.json so they're visible in review and fixed at build time:
 *
 *   "chain": { "readOnlyFolders": ["~/.claude/projects", "/Library/Logs/x"] }
 *
 * Compiled into the app as CHAIN_READ_ONLY_FOLDERS (see templates/folders.rs).
 * See agent-docs/capabilities/folders/CONTRACT.md.
 */
export function readOnlyFolders(cwd: string): string[] {
  const pkg = JSON.parse(fs.readFileSync(path.join(cwd, "package.json"), "utf8")) as {
    chain?: { readOnlyFolders?: unknown };
  };
  const declared = pkg.chain?.readOnlyFolders ?? [];
  if (!Array.isArray(declared)) {
    throw new TypeError('package.json "chain.readOnlyFolders" must be a list of folders or files.');
  }
  for (const folder of declared) {
    if (typeof folder !== "string" || !(folder.startsWith("~/") || path.isAbsolute(folder))) {
      throw new Error(
        `package.json "chain.readOnlyFolders" entry ${JSON.stringify(folder)} must be an absolute path or start with "~/".`
      );
    }
    if (folder.split(/[\\/]/).includes("..")) {
      throw new Error(`package.json "chain.readOnlyFolders" entry ${JSON.stringify(folder)} can't contain "..".`);
    }
  }
  return declared as string[];
}

export function readOnlyFoldersOrExit(cwd: string): string[] {
  try {
    return readOnlyFolders(cwd);
  } catch (error) {
    console.error(`Error: ${(error as Error).message}`);
    process.exit(1);
  }
}
