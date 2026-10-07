// The app's `db/` folder: `schema/` holds the @Table classes, `migrations/`
// holds one `NNNN-<name>.ts` per migration plus, for every migration made by
// `chain migration add` (or baselined by `chain database scaffold`), a
// `NNNN-<name>.model.json` — the whole schema right after that migration,
// like EF's per-migration model snapshot. The next `add` diffs against it.

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import type { Migration } from "./history.js";
import { canonicalize, type SchemaModel } from "./model.js";

/** Where an app that hasn't adopted the db convention yet gets it. */
const DEFAULT_DB_RELATIVE = path.join("src", "lib", "db");

export interface DbDirs {
  dbDir: string;
  migrationsDir: string;
  schemaDir: string;
}

/**
 * Finds `db/migrations` anywhere under `src/` — a plain `chain init` app
 * keeps it at `src/lib/db`, an app that reorganized (e.g. mneme's
 * `src/shared/lib/db`) keeps working.
 */
export function findDbDirs(cwd: string): DbDirs | undefined {
  const srcDir = path.join(cwd, "src");
  if (!fs.existsSync(srcDir)) return undefined;
  const stack = [srcDir];
  while (stack.length) {
    const dir = stack.pop() as string;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory() || entry.name === "node_modules") continue;
      const full = path.join(dir, entry.name);
      if (entry.name === "migrations" && path.basename(dir) === "db") {
        return { dbDir: dir, migrationsDir: full, schemaDir: path.join(dir, "schema") };
      }
      stack.push(full);
    }
  }
  return undefined;
}

const dbIndexSource = `import { desktop } from "@chain/sdk";
import { migrations } from "./migrations";

export function initDb(): Promise<void> {
  return desktop.storage.migrate(migrations);
}

// Every table's class, which is also its row type: desktop.storage.query<Course>(...).
export * from "./schema";
`;

const schemaIndexSource = `// The database schema: one @Table class per table, one file per class,
// re-exported below. Change a class, then run \`chain migration add <name>\`
// to generate the migration — like EF Core's \`dotnet ef migrations add\`.
//
//   import { Table, PrimaryKey, Column } from "@chain/sdk/schema";
//
//   @Table()
//   export class Note {
//     @PrimaryKey({ autoIncrement: true }) id!: number;
//     title!: string;              // TEXT NOT NULL
//     body!: string | null;        // TEXT, nullable
//     @Column({ defaultSql: "datetime('now')" }) created_at!: string;
//   }
//
// export { Note } from "./note";
export {};
`;

/** Creates `src/lib/db/{schema,migrations}` for an app's first `chain migration`. */
export function scaffoldDbDirs(cwd: string): DbDirs {
  const dbDir = path.join(cwd, DEFAULT_DB_RELATIVE);
  const dirs = { dbDir, migrationsDir: path.join(dbDir, "migrations"), schemaDir: path.join(dbDir, "schema") };
  fs.mkdirSync(dirs.migrationsDir, { recursive: true });
  fs.mkdirSync(dirs.schemaDir, { recursive: true });
  fs.writeFileSync(path.join(dbDir, "index.ts"), dbIndexSource);
  fs.writeFileSync(path.join(dirs.schemaDir, "index.ts"), schemaIndexSource);
  regenerateIndex(dirs.migrationsDir);
  return dirs;
}

export interface MigrationFile {
  version: number;
  slug: string;
  fileName: string;
  modelFile?: string;
}

export function migrationFiles(migrationsDir: string): MigrationFile[] {
  const entries = fs.readdirSync(migrationsDir);
  return entries
    .map((file) => /^(\d{4})-(.+)\.ts$/.exec(file))
    .filter((match): match is RegExpExecArray => match !== null)
    .map(([fileName, version, slug]) => {
      const modelFile = `${version}-${slug}.model.json`;
      return {
        version: Number(version),
        slug,
        fileName,
        ...(entries.includes(modelFile) ? { modelFile } : {})
      };
    })
    .sort((a, b) => a.version - b.version);
}

/**
 * Loads `migrations/index.ts` with Node's own TypeScript support — the files
 * are object literals behind type-only imports. Hand-written migrations from
 * before `name` existed get their file's name.
 */
export async function loadMigrations(migrationsDir: string): Promise<Migration[]> {
  const indexPath = path.join(migrationsDir, "index.ts");
  const url = `${pathToFileURL(indexPath).href}?t=${Date.now()}`;
  const mod = (await import(url)) as { migrations: Migration[] };
  const files = migrationFiles(migrationsDir);
  return [...mod.migrations]
    .map((m) => ({ ...m, name: m.name ?? files.find((f) => f.version === m.version)?.slug }))
    .sort((a, b) => a.version - b.version);
}

export function readModel(migrationsDir: string, file: MigrationFile): SchemaModel {
  return JSON.parse(fs.readFileSync(path.join(migrationsDir, file.modelFile!), "utf8")) as SchemaModel;
}

export function writeModel(migrationsDir: string, file: MigrationFile, model: SchemaModel): string {
  const modelFile = `${String(file.version).padStart(4, "0")}-${file.slug}.model.json`;
  fs.writeFileSync(path.join(migrationsDir, modelFile), `${JSON.stringify(canonicalize(model), null, 2)}\n`);
  return modelFile;
}

export function slugify(name: string): string {
  // Runs of other characters collapse to one "-", so at most one sits at each end.
  return name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-/, "").replace(/-$/, "");
}

export function camelCase(slug: string): string {
  const camel = slug
    .split("-")
    .filter(Boolean)
    .map((word, i) => (i === 0 ? word : word[0].toUpperCase() + word.slice(1)))
    .join("");
  // "2fa-support" would otherwise be an invalid identifier.
  return /^\d/.test(camel) ? `m${camel}` : camel;
}

function template(sql: string): string {
  return `\`${sql.replaceAll("\\", "\\\\").replaceAll("`", "\\`").replaceAll("${", "\\${")}\``;
}

/** A generated migration: Up and Down SQL, as EF writes Up() and Down(). */
export function generatedMigrationSource(version: number, slug: string, up: string, down: string, warnings: string[]): string {
  const reviewLines = warnings.map((w) => "//   - " + w).join("\n");
  const review = warnings.length ? `//\n// Review before shipping:\n${reviewLines}\n` : "";
  return `import type { Migration } from "@chain/sdk";

// Generated by \`chain migration add ${slug}\` from the classes in ../schema.
// Never edit a migration once it has shipped; change the classes and add
// another. \`down\` is what \`chain database update <earlier version>\` runs.
${review}export const ${camelCase(slug)}: Migration = {
  version: ${version},
  name: ${JSON.stringify(slug)},
  sql: ${template(up)},
  down: ${template(down)},
};
`;
}

/** A hand-written migration skeleton (`chain migration add <name> --empty`). */
export function emptyMigrationSource(version: number, slug: string): string {
  return `import type { Migration } from "@chain/sdk";

// Written by hand (\`chain migration add ${slug} --empty\`), for changes the
// schema classes can't express, like moving data. Fill in both directions.
export const ${camelCase(slug)}: Migration = {
  version: ${version},
  name: ${JSON.stringify(slug)},
  sql: \`
    -- TODO: the change.
  \`,
  down: \`
    -- TODO: undo it.
  \`,
};
`;
}

/** Rewrites `migrations/index.ts` from the files on disk — it's entirely generated, never edited by hand. */
export function regenerateIndex(migrationsDir: string): void {
  const entries = migrationFiles(migrationsDir).map((file) => ({
    // Explicit .ts extension: `chain database` loads this with Node's
    // type stripping, which has no extension probing.
    module: `./${file.fileName}`,
    identifier: camelCase(file.slug)
  }));
  const imports = entries.map((e) => `import { ${e.identifier} } from "${e.module}";`).join("\n");
  const list = entries.map((e) => `  ${e.identifier},`).join("\n");
  fs.writeFileSync(
    path.join(migrationsDir, "index.ts"),
    `import type { Migration } from "@chain/sdk";
${imports}

// Generated by \`chain migration\` — oldest to newest. Never edit by hand.
// \`desktop.storage.migrate\` applies whichever versions haven't run yet,
// so this is safe to call on every app startup.
export const migrations: Migration[] = [
${list}
];
`
  );
}
