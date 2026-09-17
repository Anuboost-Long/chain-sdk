#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { init } from "./init.js";
import { doctor } from "./doctor.js";
import { update } from "./update.js";
import { dev } from "./dev.js";
import { build } from "./build.js";
import { inspect } from "./inspect.js";
import { migration } from "./migration.js";
import { database } from "./database.js";

const cliDir = fileURLToPath(new URL(".", import.meta.url));
const pkg = JSON.parse(readFileSync(path.join(cliDir, "../package.json"), "utf8")) as {
  version: string;
};

function printHelp(): void {
  console.log(`chain — Chain framework CLI (v${pkg.version})

Usage:
  chain init <project-name>   Scaffold a basic app wired to @chain/sdk,
                               created in the directory you run this from.
  chain dev                   Run the app's dev server (wraps tauri dev)
                               behind condensed, branded output instead of
                               raw Tauri/Vite/cargo output. Run from
                               inside the app — this is what a scaffolded
                               app's \`npm run dev\` calls.
  chain build                 Build the app for release (wraps tauri
                               build) behind the same condensed output.
                               This is what \`npm run build\` calls.
  chain inspect               Connect to a running \`chain dev\`'s dev-only
                               automation bridge and drive its window (eval
                               JS, click, read text, screenshot, real
                               OS-level drag — macOS only for the latter
                               two) from a REPL, or one-shot via --eval/
                               --rect/--screenshot/--drag. Run from inside
                               the app while \`chain dev\` is running.
  chain update                Merge chain-sdk template changes into the
                               app in the current directory, preserving
                               your edits (run from inside the app).
  chain migration <name>      Scaffold the next numbered SQLite migration
                               file (db/migrations/000N-<name>.ts) and
                               rewire its index. First use in an app sets
                               up db/migrations + db/schema. Run from
                               inside the app.
  chain database update       Apply every pending migration straight to
                               the app's real SQLite file (the same one
                               desktop.storage.migrate() would use) —
                               without launching the app. Run from
                               inside the app.
  chain database list         List every migration with its applied/
                               pending status against that same file.
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

  case "dev":
    await dev(args);
    break;

  case "build":
    await build(args);
    break;

  case "inspect":
    await inspect(args);
    break;

  case "doctor":
    await doctor();
    break;

  case "update":
    await update();
    break;

  case "migration":
    migration(args);
    break;

  case "database":
    await database(args);
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
