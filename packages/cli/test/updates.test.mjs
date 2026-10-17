// The update feed `chain build` writes (desktop.updates). Runs on the
// built output: `npm run build && npm test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { mergeFeed, platformKeys, updateArch } from "../dist/updates.js";

const build = (version, keys, notes) => ({ version, notes, date: "2026-10-17T00:00:00.000Z", keys, signature: `sig-${keys[0]}`, url: `https://x/${keys[0]}` });

test("two architectures of one version share a feed", () => {
  const arm = mergeFeed(undefined, build("1.2.0", ["darwin-aarch64"], "Fixes"));
  const both = mergeFeed(arm, build("1.2.0", ["darwin-x86_64"]));
  assert.deepEqual(Object.keys(both.platforms).sort(), ["darwin-aarch64", "darwin-x86_64"]);
  assert.equal(both.notes, "Fixes");
  assert.equal(both.platforms["darwin-aarch64"].signature, "sig-darwin-aarch64");
});

test("a new version starts the feed afresh", () => {
  const old = mergeFeed(undefined, build("1.2.0", ["darwin-x86_64"], "Old notes"));
  const next = mergeFeed(old, build("1.3.0", ["darwin-aarch64"]));
  assert.deepEqual(Object.keys(next.platforms), ["darwin-aarch64"]);
  assert.equal(next.notes, undefined);
});

test("architectures come from --target, else this Mac", () => {
  assert.deepEqual(updateArch(["--target", "x86_64-apple-darwin"]), { triple: "x86_64-apple-darwin", arch: "x86_64" });
  assert.deepEqual(updateArch(["--target", "universal-apple-darwin"]), { triple: "universal-apple-darwin", arch: "universal" });
  assert.equal(updateArch([]).arch, process.arch === "x64" ? "x86_64" : "aarch64");
  assert.deepEqual(platformKeys("universal"), ["darwin-aarch64", "darwin-x86_64"]);
});
