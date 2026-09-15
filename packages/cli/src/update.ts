import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  chainRoot,
  scaffoldContext,
  desiredContent,
  TRACKED_FILES,
  type ScaffoldContext
} from "./scaffold.js";

function readIfExists(p: string): string | undefined {
  return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : undefined;
}

/** Runs git's own 3-way text merge: `ours` (the developer's current file)
 * against `theirs` (the newly generated template), using `base` (the
 * baseline snapshot from the last sync) as the common ancestor. Returns
 * the merged text and whether it contains unresolved conflict markers. */
function mergeFile(
  base: string,
  ours: string,
  theirs: string
): { merged: string; conflicted: boolean } {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "chain-update-"));
  const oursPath = path.join(dir, "ours");
  const basePath = path.join(dir, "base");
  const theirsPath = path.join(dir, "theirs");
  fs.writeFileSync(oursPath, ours);
  fs.writeFileSync(basePath, base);
  fs.writeFileSync(theirsPath, theirs);
  try {
    const result = execFileSync(
      "git",
      [
        "merge-file",
        "-p",
        "-L",
        "yours",
        "-L",
        "previous chain-sdk version",
        "-L",
        "new chain-sdk version",
        oursPath,
        basePath,
        theirsPath
      ],
      { encoding: "utf8" }
    );
    return { merged: result, conflicted: false };
  } catch (error) {
    const e = error as { status?: number; stdout?: string };
    if (e.status === 1 && typeof e.stdout === "string") {
      // exit 1 = merged with conflict markers, not a real failure
      return { merged: e.stdout, conflicted: true };
    }
    throw error;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function syncTextFile(
  ctx: ScaffoldContext,
  baselineDir: string,
  file: Extract<(typeof TRACKED_FILES)[number], { kind: "patched" | "template" | "copy" }>
): { status: "new" | "unchanged" | "merged" | "conflict" | "skipped-deleted" } {
  const filePath = path.join(ctx.target, file.relPath);
  const baselinePath = path.join(baselineDir, file.relPath);
  const baseline = readIfExists(baselinePath);
  const theirs = desiredContent(file, baseline, ctx);

  if (baseline === undefined) {
    // Newly tracked file (introduced by a newer chain-sdk) — nothing to
    // merge against yet.
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, theirs);
    fs.mkdirSync(path.dirname(baselinePath), { recursive: true });
    fs.writeFileSync(baselinePath, theirs);
    return { status: "new" };
  }

  const ours = readIfExists(filePath);
  if (ours === undefined) {
    // Developer deleted this file on purpose — respect that.
    return { status: "skipped-deleted" };
  }

  if (theirs === baseline) return { status: "unchanged" };

  if (ours === baseline) {
    fs.writeFileSync(filePath, theirs);
    fs.writeFileSync(baselinePath, theirs);
    return { status: "merged" };
  }

  const { merged, conflicted } = mergeFile(baseline, ours, theirs);
  fs.writeFileSync(filePath, merged);
  fs.writeFileSync(baselinePath, theirs);
  return { status: conflicted ? "conflict" : "merged" };
}

function syncIconDir(
  ctx: ScaffoldContext,
  baselineDir: string,
  file: Extract<(typeof TRACKED_FILES)[number], { kind: "binary-dir" }>
): { added: string[]; updated: string[]; skipped: string[] } {
  const sourceDir = path.join(chainRoot, file.sourceDir);
  const targetDir = path.join(ctx.target, file.relPath);
  const baselineFileDir = path.join(baselineDir, file.relPath);
  fs.mkdirSync(targetDir, { recursive: true });
  fs.mkdirSync(baselineFileDir, { recursive: true });

  const added: string[] = [];
  const updated: string[] = [];
  const skipped: string[] = [];

  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const theirs = fs.readFileSync(path.join(sourceDir, entry.name));
    const baselinePath = path.join(baselineFileDir, entry.name);
    const targetPath = path.join(targetDir, entry.name);
    const baseline = fs.existsSync(baselinePath) ? fs.readFileSync(baselinePath) : undefined;

    if (baseline === undefined) {
      fs.writeFileSync(targetPath, theirs);
      fs.writeFileSync(baselinePath, theirs);
      added.push(`${file.relPath}/${entry.name}`);
      continue;
    }
    if (!fs.existsSync(targetPath)) {
      skipped.push(`${file.relPath}/${entry.name}`);
      continue;
    }
    const ours = fs.readFileSync(targetPath);
    if (ours.equals(baseline)) {
      if (!theirs.equals(baseline)) {
        fs.writeFileSync(targetPath, theirs);
        fs.writeFileSync(baselinePath, theirs);
        updated.push(`${file.relPath}/${entry.name}`);
      }
    } else {
      skipped.push(`${file.relPath}/${entry.name}`);
    }
  }
  return { added, updated, skipped };
}

export async function update(): Promise<void> {
  const target = process.cwd();
  const baselineDir = path.join(target, ".chain/baseline");
  const ctx = scaffoldContext(target);

  if (
    !fs.existsSync(path.join(target, "package.json")) ||
    !fs.existsSync(path.join(target, "src-tauri"))
  ) {
    console.error(
      "Error: this doesn't look like a chain init-scaffolded app (no package.json/src-tauri here)."
    );
    process.exit(1);
  }

  if (!fs.existsSync(baselineDir)) {
    console.log(
      "No .chain/baseline found — this project predates `chain update`. Recording its current " +
        "state as the starting point; nothing is changed this run. Future `chain update` runs " +
        "will merge chain-sdk changes in from here."
    );
    for (const file of TRACKED_FILES) {
      if (file.kind === "binary-dir") {
        const dir = path.join(target, file.relPath);
        if (fs.existsSync(dir))
          fs.cpSync(dir, path.join(baselineDir, file.relPath), { recursive: true });
        continue;
      }
      const filePath = path.join(target, file.relPath);
      if (!fs.existsSync(filePath)) continue;
      const baselinePath = path.join(baselineDir, file.relPath);
      fs.mkdirSync(path.dirname(baselinePath), { recursive: true });
      fs.copyFileSync(filePath, baselinePath);
    }
    console.log(
      "Baseline recorded. Run `chain update` again any time chain-sdk's templates change."
    );
    return;
  }

  console.log(`Updating ${ctx.name} from chain-sdk...\n`);

  const conflicts: string[] = [];
  let changed = 0;

  for (const file of TRACKED_FILES) {
    if (file.kind === "binary-dir") {
      const { added, updated, skipped } = syncIconDir(ctx, baselineDir, file);
      for (const f of added) console.log(`  add      ${f}`);
      for (const f of updated) console.log(`  update   ${f}`);
      for (const f of skipped) console.log(`  skip     ${f} (customized)`);
      changed += added.length + updated.length;
      continue;
    }
    const result = syncTextFile(ctx, baselineDir, file);
    if (result.status === "new") {
      console.log(`  add      ${file.relPath}`);
      changed++;
    } else if (result.status === "merged") {
      console.log(`  update   ${file.relPath}`);
      changed++;
    } else if (result.status === "conflict") {
      console.log(`  conflict ${file.relPath} — resolve the <<<<<<< markers by hand`);
      conflicts.push(file.relPath);
      changed++;
    } else if (result.status === "skipped-deleted") {
      console.log(`  skip     ${file.relPath} (you deleted it)`);
    }
    // "unchanged" prints nothing — the common case shouldn't be noisy.
  }

  if (changed === 0) {
    console.log("Already up to date.");
    return;
  }

  console.log(`\n${changed} file(s) updated.`);
  if (conflicts.length > 0) {
    console.log(
      `\n${conflicts.length} file(s) have merge conflicts — resolve the <<<<<<< / ======= / >>>>>>> ` +
        `markers before running npm install or building:\n` +
        conflicts.map((f) => `  - ${f}`).join("\n")
    );
  } else {
    console.log("Run `npm install` if dependencies changed, then `npm run dev` to verify.");
  }
}
