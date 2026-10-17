// The stale-build check behind `chain init`/`chain update` refusing to run
// when dist/ is older than src/. Runs on the built output: `npm run build && npm test`.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { staleSources } from "../dist/staleBuild.js";

function cli(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "chain-stale-"));
  for (const [file, mtime] of Object.entries(files)) {
    const full = path.join(root, file);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, "");
    fs.utimesSync(full, mtime, mtime);
  }
  return root;
}

test("an npm install has no src/ and is never stale", () => {
  assert.deepEqual(staleSources(cli({ "dist/bin.js": 1 })), []);
});

test("sources newer than their build, or never built, are stale", () => {
  const root = cli({
    "src/bin.ts": 100,
    "dist/bin.js": 200,
    "src/scaffold.ts": 300,
    "dist/scaffold.js": 200,
    "src/schema/table.ts": 100,
    "src/types.d.ts": 999
  });
  assert.deepEqual(staleSources(root), ["scaffold.ts", path.join("schema", "table.ts")]);
});
