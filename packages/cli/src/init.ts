import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { checkRustQuietly } from "./doctor.js";
import {
  chainRoot,
  scaffoldContext,
  desiredContent,
  TRACKED_FILES,
  type ScaffoldContext
} from "./scaffold.js";

function copyDirContents(srcDir: string, destDir: string): void {
  fs.mkdirSync(destDir, { recursive: true });
  for (const entry of fs.readdirSync(srcDir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    fs.copyFileSync(path.join(srcDir, entry.name), path.join(destDir, entry.name));
  }
}

/** Writes every tracked file's current desired content into `target`, and
 * mirrors the same content into `.chain/baseline/` — the ancestor `chain
 * update` will later 3-way-merge against. Called once, right after
 * create-tauri-app has produced the files "patched" kind entries start
 * from. */
function writeTrackedFiles(ctx: ScaffoldContext): void {
  const baselineDir = path.join(ctx.target, ".chain/baseline");

  for (const file of TRACKED_FILES) {
    const filePath = path.join(ctx.target, file.relPath);
    const baselinePath = path.join(baselineDir, file.relPath);

    if (file.kind === "binary-dir") {
      copyDirContents(path.join(chainRoot, file.sourceDir), filePath);
      copyDirContents(path.join(chainRoot, file.sourceDir), baselinePath);
      continue;
    }

    const currentContent = file.kind === "patched" ? fs.readFileSync(filePath, "utf8") : undefined;
    const content = desiredContent(file, currentContent, ctx);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
    fs.mkdirSync(path.dirname(baselinePath), { recursive: true });
    fs.writeFileSync(baselinePath, content);
  }
}

export async function init(projectName: string): Promise<void> {
  const target = path.resolve(process.cwd(), projectName);
  const name = path.basename(target);
  const parentDir = path.dirname(target);

  if (fs.existsSync(target)) {
    console.error(`Error: ${target} already exists. chain init only creates new projects.`);
    process.exit(1);
  }

  fs.mkdirSync(parentDir, { recursive: true });

  console.log(`Scaffolding a Chain app at ${target}\n`);
  console.log("Running create-tauri-app (React + TypeScript)...\n");
  execFileSync(
    "npx",
    [
      "--yes",
      "create-tauri-app@latest",
      name,
      "--manager",
      "npm",
      "--template",
      "react-ts",
      "-y",
      "--identifier",
      `dev.chain.${name}`
    ],
    { cwd: parentDir, stdio: "inherit" }
  );

  // create-tauri-app's default counter/greet assets aren't used by any of
  // our templates.
  fs.rmSync(path.join(target, "src/assets"), { recursive: true, force: true });

  console.log("\nWiring @chain/sdk, Tailwind CSS, react-router-dom, and chain-core...");
  const ctx = scaffoldContext(target);
  writeTrackedFiles(ctx);

  console.log("\nInstalling dependencies (npm install)...");
  execFileSync("npm", ["install"], { cwd: target, stdio: "inherit" });

  if (!fs.existsSync(path.join(target, ".git"))) {
    execFileSync("git", ["init"], { cwd: target, stdio: "ignore" });
    console.log("\n  add   .git (git init)");
  }

  console.log(`\nDone. @chain/sdk is linked via file:${ctx.sdkRelative}`);
  console.log(`Next: cd ${target} && npm run dev`);
  console.log(
    "Later, when chain-sdk's templates change, run `chain update` from inside this app to " +
      "pick them up without losing your edits."
  );

  if (!checkRustQuietly()) {
    console.log(
      "\nNote: no Rust toolchain found on this machine — you'll need it to build/run this app's " +
        "native (Tauri) side. Run `chain doctor` to check/install it."
    );
  }
}
