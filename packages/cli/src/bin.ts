#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { init } from "./init.js";
import { doctor } from "./doctor.js";
import { update } from "./update.js";
import { dev } from "./dev.js";
import { build } from "./build.js";
import { clean } from "./clean.js";
import { inspect } from "./inspect.js";

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
  chain migration add <name>  Generate the next migration by diffing the
                               app's @Table schema classes against the
                               last migration's saved model (Up and Down
                               SQL), like \`dotnet ef migrations add\`.
                               --empty for one you write by hand.
  chain migration remove      Delete the latest migration, if it isn't
                               applied to your local database.
  chain migration list        Every migration and whether it's applied.
  chain migration script [from] [to]
                               Print the SQL between two versions.
  chain migration check       Exit 1 if the classes have changes no
                               migration covers (for CI).
  chain database update [target]
                               Apply pending migrations to the app's real
                               SQLite file, or revert down to a version or
                               name — without launching the app.
  chain database list         Same as \`chain migration list\`.
  chain database scaffold     Generate @Table classes from an app's
                               existing hand-written migrations, so
                               \`migration add\` can take over.
  chain clean                 Free disk space: remove this app's release
                               builds and its own crate from the shared
                               dev build cache (run from inside the app).
  chain clean --all           Remove the whole shared dev build cache
                               used by every Chain app on this machine.
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

  case "clean":
    clean(args);
    break;

  case "doctor":
    await doctor();
    break;

  case "update":
    await update();
    break;

  // Loaded on demand: they use node:sqlite, whose ExperimentalWarning
  // would otherwise print on every chain command.
  case "migration":
    await (await import("./migration.js")).migration(args);
    break;

  case "database":
    await (await import("./database.js")).database(args);
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
