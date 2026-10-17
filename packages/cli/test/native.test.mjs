// .chain/native is generated from chain-sdk's templates and package.json
// (mneme request 43). Runs on the built output: `npm run build && npm test`.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { crateName, generateNativeProject, patchGitignore, readAppIdentity } from "../dist/native.js";

function app(pkg, nativeConf) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "chain-native-"));
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify(pkg, null, 2));
  if (nativeConf) {
    fs.mkdirSync(path.join(dir, ".chain/native"), { recursive: true });
    fs.writeFileSync(path.join(dir, ".chain/native/tauri.conf.json"), JSON.stringify(nativeConf));
  }
  return dir;
}

const quiet = (f) => {
  const log = console.log;
  console.log = () => {};
  try {
    return f();
  } finally {
    console.log = log;
  }
};

test("a deleted native project comes back, and an unchanged one isn't rewritten", () => {
  const dir = app({ name: "mneme", version: "1.2.3", dependencies: { "@chain/sdk": "file:../chain-sdk/packages/sdk" } });
  const written = quiet(() => generateNativeProject(dir));
  assert.ok(written.includes("Cargo.toml") && written.includes("tauri.conf.json") && written.includes("src/lib.rs"));
  const conf = JSON.parse(fs.readFileSync(path.join(dir, ".chain/native/tauri.conf.json"), "utf8"));
  assert.equal(conf.identifier, "dev.chain.mneme");
  assert.equal(conf.version, "1.2.3");
  const cargo = fs.readFileSync(path.join(dir, ".chain/native/Cargo.toml"), "utf8");
  assert.match(cargo, /^name = "mneme"$/m);
  assert.match(cargo, /chain-core = \{ path = "\.\.\/\.\.\/\.\.\/chain-sdk\/crates\/core" \}/);
  assert.match(fs.readFileSync(path.join(dir, ".chain/native/src/main.rs"), "utf8"), /mneme_lib::run\(\)/);
  assert.deepEqual(quiet(() => generateNativeProject(dir)), []);
});

test("a broken generated file is regenerated", () => {
  const dir = app({ name: "mneme" });
  quiet(() => generateNativeProject(dir));
  const cargo = path.join(dir, ".chain/native/Cargo.toml");
  fs.writeFileSync(cargo, "<<<<<<< yours\n[package\n=======\n>>>>>>> new\n");
  assert.deepEqual(quiet(() => generateNativeProject(dir)), ["Cargo.toml"]);
  assert.match(fs.readFileSync(cargo, "utf8"), /^\[package\]$/m);
});

test("the identity is pinned in package.json, keeping an existing native identifier", () => {
  const dir = app({ name: "renamed-app" }, { productName: "Mneme", identifier: "dev.chain.mneme" });
  assert.deepEqual(readAppIdentity(dir), { name: "Mneme", identifier: "dev.chain.mneme", version: "0.1.0", crate: "renamed-app" });
  quiet(() => generateNativeProject(dir));
  const pkg = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  assert.deepEqual(pkg.chain.app, { name: "Mneme", identifier: "dev.chain.mneme" });
  fs.rmSync(path.join(dir, ".chain"), { recursive: true });
  assert.equal(readAppIdentity(dir).identifier, "dev.chain.mneme", "kept after .chain/ is deleted");
});

test("Chain's ignore rules replace a blanket .chain/ rule, once", () => {
  const patched = patchGitignore("node_modules\n.chain/\ndist\n");
  assert.doesNotMatch(patched, /^\.chain\/$/m);
  assert.match(patched, /^\/\.chain\/native\/\*$/m);
  assert.match(patched, /^!\/\.chain\/native\/Cargo\.lock$/m);
  assert.equal(patchGitignore(patched), patched);
});

test("crate names follow create-tauri-app's", () => {
  assert.equal(crateName("lazify-chain"), "lazify-chain");
  assert.equal(crateName("My App!"), "my-app");
});
