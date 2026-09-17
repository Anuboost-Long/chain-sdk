import fs from "node:fs";
import path from "node:path";

import { checkChainApp } from "./nativeProject.js";

/** Where a fresh app that hasn't adopted the db convention yet gets it
 * scaffolded — matches a plain `chain init` app's flat `src/lib/`. Once
 * an app restructures (e.g. into `src/shared/lib/db`), `findMigrationsDir`
 * finds it there instead; this path is only ever used on first run. */
const DEFAULT_DB_RELATIVE = path.join("src", "lib", "db");

/**
 * Finds the app's `db/migrations` folder by walking `src/` looking for a
 * directory named `migrations` whose parent is named `db` — chain
 * doesn't prescribe a fixed depth or parent path (a plain `chain init`
 * app keeps it at `src/lib/db/migrations`; an app that later reorganizes
 * into features, like `src/shared/lib/db/migrations`, keeps working
 * without chain needing to know about that restructuring). Returns
 * `undefined` if the app hasn't adopted the convention yet.
 */
export function findMigrationsDir(srcDir: string): string | undefined {
  if (!fs.existsSync(srcDir)) return undefined;
  const stack = [srcDir];
  while (stack.length) {
    const dir = stack.pop() as string;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory() || entry.name === "node_modules") continue;
      const full = path.join(dir, entry.name);
      if (entry.name === "migrations" && path.basename(dir) === "db") return full;
      stack.push(full);
    }
  }
  return undefined;
}

function dbIndexSource(): string {
  return `import { desktop } from "@chain/sdk";
import { migrations } from "./migrations";

export function initDb(): Promise<void> {
  return desktop.storage.migrate(migrations);
}

// Re-exports every table's current row shape — see \`./schema\`.
export type * from "./schema";
`;
}

function schemaIndexSource(): string {
  return `// One file per table lives in this folder, each exporting a plain
// interface documenting that table's current column list — the model
// layer, kept separate from \`../migrations\`, which documents how the
// database got there.
//
// Add a table:
//   1. Create \`<table>.ts\` here with an interface named \`<Table>Row\`,
//      one field per column, commented with its SQL type/constraints
//      and which migration added it.
//   2. Re-export it below.
//   3. In your feature's lib file, \`desktop.storage.query<XRow>(...)\`
//      the raw rows and map them onto your app-facing type (e.g. a
//      \`bookmarked\` 0/1 column -> \`boolean\`, a numeric enum column ->
//      its TS enum).
//
// \`chain migration <name>\` only generates the SQL migration file — it
// never touches this folder. Update the matching table here by hand
// after you write a migration that changes its shape.
//
// export type { CourseRow } from "./course";
export {};
`;
}

function scaffoldDb(cwd: string): string {
  const dbDir = path.join(cwd, DEFAULT_DB_RELATIVE);
  const migrationsDir = path.join(dbDir, "migrations");
  const schemaDir = path.join(dbDir, "schema");
  fs.mkdirSync(migrationsDir, { recursive: true });
  fs.mkdirSync(schemaDir, { recursive: true });
  fs.writeFileSync(path.join(dbDir, "index.ts"), dbIndexSource());
  fs.writeFileSync(path.join(schemaDir, "index.ts"), schemaIndexSource());
  return migrationsDir;
}

function slugify(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (!slug) {
    console.error("Usage: chain migration <name> — e.g. `chain migration add-status-column`.");
    process.exit(1);
  }
  return slug;
}

function camelCase(slug: string): string {
  const camel = slug
    .split("-")
    .filter(Boolean)
    .map((word, i) => (i === 0 ? word : word[0].toUpperCase() + word.slice(1)))
    .join("");
  // A slug that's all digits, or starts with one (e.g. "2fa-support"),
  // would otherwise produce an invalid JS identifier.
  return /^[0-9]/.test(camel) ? `m${camel}` : camel;
}

function nextVersion(migrationsDir: string): number {
  const versions = fs
    .readdirSync(migrationsDir)
    .map((file) => /^(\d{4})-/.exec(file)?.[1])
    .filter((v): v is string => Boolean(v))
    .map(Number);
  return versions.length ? Math.max(...versions) + 1 : 1;
}

function migrationFileSource(identifier: string, version: number): string {
  return `import type { Migration } from "@chain/sdk";

export const ${identifier}: Migration = {
  version: ${version},
  sql: \`
    -- TODO: write this migration's SQL.
  \`,
};
`;
}

/** `migrations/index.ts` is entirely chain-generated — never hand-edit
 * it, the next `chain migration` run rewrites it from whatever
 * `NNNN-*.ts` files are on disk. */
function regenerateIndex(migrationsDir: string): void {
  const files = fs
    .readdirSync(migrationsDir)
    .filter((file) => /^\d{4}-.+\.ts$/.test(file))
    .sort();
  const entries = files.map((file) => {
    const slug = file.replace(/^\d{4}-/, "").replace(/\.ts$/, "");
    return { module: `./${file.replace(/\.ts$/, "")}`, identifier: camelCase(slug) };
  });
  const imports = entries.map((e) => `import { ${e.identifier} } from "${e.module}";`).join("\n");
  const list = entries.map((e) => `  ${e.identifier},`).join("\n");
  fs.writeFileSync(
    path.join(migrationsDir, "index.ts"),
    `import type { Migration } from "@chain/sdk";
${imports}

// Generated by \`chain migration\` — ordered oldest to newest, one file
// per version. Never edit a shipped migration in place; running
// \`chain migration <name>\` again adds the next one and regenerates this
// file. \`desktop.storage.migrate\` applies whichever versions haven't run
// yet, so this is safe to call on every app startup.
export const migrations: Migration[] = [
${list}
];
`
  );
}

/**
 * Scaffolds the next numbered SQLite migration file — the file/numbering/
 * wiring equivalent of `rails generate migration` or `knex migrate:make`,
 * not EF Core's model-diffing `dotnet ef migrations add` (deliberately:
 * the storage capability's contract is "no ORM, no generated typed
 * models, no query builder" — the app still writes its own SQL and, if
 * its shape changed, its own `schema/<table>.ts` update by hand). See
 * `agent-docs/framework/command/README.md` for the full design.
 */
export function migration(args: string[]): void {
  const [rawName] = args;
  if (!rawName) {
    console.error("Usage: chain migration <name>");
    process.exit(1);
  }

  const slug = slugify(rawName);
  const identifier = camelCase(slug);

  const cwd = process.cwd();
  checkChainApp(cwd);

  let migrationsDir = findMigrationsDir(path.join(cwd, "src"));
  let scaffolded = false;
  if (!migrationsDir) {
    migrationsDir = scaffoldDb(cwd);
    scaffolded = true;
  }

  const version = nextVersion(migrationsDir);
  const fileName = `${String(version).padStart(4, "0")}-${slug}.ts`;
  fs.writeFileSync(path.join(migrationsDir, fileName), migrationFileSource(identifier, version));
  regenerateIndex(migrationsDir);

  const relMigrationsDir = path.relative(cwd, migrationsDir);
  const relSchemaDir = path.relative(cwd, path.join(path.dirname(migrationsDir), "schema"));
  if (scaffolded) {
    console.log(`Set up ${path.relative(cwd, path.dirname(migrationsDir))}/ (first use of \`chain migration\` in this app) — see its schema/index.ts for the table-model convention.\n`);
  }
  console.log(`Created ${relMigrationsDir}/${fileName} (version ${version}).`);
  console.log(`Updated ${relMigrationsDir}/index.ts.`);
  console.log(`\nNext: write this migration's SQL, then update the matching table's row type in ${relSchemaDir}/ if its shape changed.`);
}
