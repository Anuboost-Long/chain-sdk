#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { init } from "./init.js";
import { doctor } from "./doctor.js";
import { update } from "./update.js";

const cliDir = fileURLToPath(new URL(".", import.meta.url));
const pkg = JSON.parse(readFileSync(path.join(cliDir, "../package.json"), "utf8")) as {
  version: string;
};

function printHelp(): void {
  console.log(`chain — Chain framework CLI (v${pkg.version})

Usage:
  chain init <project-name>   Scaffold a basic app wired to @chain/sdk,
                               created in the directory you run this from.
  chain update                Merge chain-sdk template changes into the
                               app in the current directory, preserving
                               your edits (run from inside the app).
  chain doctor                Check (and optionally install) the Rust
                               toolchain a Chain app needs to build.
  chain --help, -h            Show this help.
  chain --version, -v         Print the CLI version.
`);
}

const [command, ...args] = process.argv.slice(2);

switch (command) {
  case "init": {
    const [projectName] = args;
    if (!projectName) {
      console.error("Usage: chain init <project-name>");
      process.exit(1);
    }
    await init(projectName);
    break;
  }

  case "doctor":
    await doctor();
    break;

  case "update":
    await update();
    break;

  case "--help":
  case "-h":
  case "help":
    printHelp();
    break;

  case "--version":
  case "-v":
  case "version":
    console.log(pkg.version);
    break;

  case undefined:
    printHelp();
    process.exit(1);
    break;

  default:
    console.error(`Unknown command: ${command}\n`);
    printHelp();
    process.exit(1);
}
