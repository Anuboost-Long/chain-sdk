#!/usr/bin/env node
// Copies the app-icon/icons chain init/update stamp into scaffolded apps
// from the repo-root asset/ directory into packages/cli/asset/, so they
// ship inside the @chain/cli package itself. Once @chain/cli is installed
// from npm there's no repo-root asset/ next to it — chainRoot (see
// scaffold.ts) resolves to packages/cli's own installed location, so the
// icons have to physically live there too.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const cliDir = path.resolve(fileURLToPath(new URL(".", import.meta.url)), "..");
const repoRoot = path.resolve(cliDir, "../..");

function copyFile(rel) {
  const dest = path.join(cliDir, "asset", rel);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(path.join(repoRoot, "asset", rel), dest);
}

function copyDir(rel) {
  const src = path.join(repoRoot, "asset", rel);
  const dest = path.join(cliDir, "asset", rel);
  fs.rmSync(dest, { recursive: true, force: true });
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src)) {
    fs.copyFileSync(path.join(src, entry), path.join(dest, entry));
  }
}

copyFile("app-icon.svg");
copyDir("icons");
console.log("sync-assets: asset/app-icon.svg, asset/icons/ -> packages/cli/asset/");
