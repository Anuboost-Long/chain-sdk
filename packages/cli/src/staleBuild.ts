import fs from "node:fs";
import path from "node:path";

/**
 * Source files changed since `npm run build` — newer than their compiled
 * `dist/` file, or with none — relative to `src/`. Empty for an npm
 * install, which ships `dist/` without `src/`.
 */
export function staleSources(cliRoot: string): string[] {
  const src = path.join(cliRoot, "src");
  if (!fs.existsSync(src)) return [];
  return fs
    .readdirSync(src, { recursive: true, encoding: "utf8" })
    .filter((file) => file.endsWith(".ts") && !file.endsWith(".d.ts"))
    .filter((file) => {
      const compiled = path.join(cliRoot, "dist", file.replace(/\.ts$/, ".js"));
      return !fs.existsSync(compiled) || fs.statSync(path.join(src, file)).mtimeMs > fs.statSync(compiled).mtimeMs;
    })
    .sort((a, b) => a.localeCompare(b));
}
