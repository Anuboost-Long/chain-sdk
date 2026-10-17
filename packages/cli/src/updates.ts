// The build side of desktop.updates (agent-docs/capabilities/updates):
// `chain build` signs the update archive and writes the feed, and
// `chain updates key` makes the signing key pair.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { readAppIdentity } from "./native.js";
import { nativeProjectDir, resolveTauriBin } from "./nativeProject.js";

export interface UpdatesConfig {
  feed: string;
  publicKey: string;
}

/** package.json's "chain.updates", or undefined. Exits on a malformed one. */
export function readUpdatesConfigOrExit(cwd: string): UpdatesConfig | undefined {
  const pkg = JSON.parse(fs.readFileSync(path.join(cwd, "package.json"), "utf8"));
  const updates = pkg?.chain?.updates;
  if (updates === undefined) return undefined;
  if (typeof updates?.feed !== "string" || typeof updates?.publicKey !== "string" || !updates.publicKey.trim()) {
    console.error('Error: package.json "chain.updates" needs a "feed" URL and a "publicKey" (`chain updates key` prints one).');
    process.exit(1);
  }
  return { feed: updates.feed, publicKey: updates.publicKey };
}

/** Removes `--notes <file>` from the args passed on to Tauri, and returns its text. */
export function takeNotesOrExit(args: string[]): string | undefined {
  const index = args.indexOf("--notes");
  if (index === -1) return undefined;
  const file = args[index + 1];
  args.splice(index, 2);
  if (!file || !fs.existsSync(file)) {
    const missing = file ? `; ${file} doesn't exist` : "";
    console.error(`Error: --notes needs a file of release notes${missing}.`);
    process.exit(1);
  }
  return fs.readFileSync(file, "utf8").trim();
}

/** Tauri's signing settings, from Chain's own variables. Exits when the key isn't given. */
export function signingEnvOrExit(): NodeJS.ProcessEnv {
  const keyPath = process.env.CHAIN_UPDATE_KEY;
  if (!keyPath || !fs.existsSync(keyPath)) {
    console.error(
      'Error: package.json has "chain.updates", so the build must sign its update. Set CHAIN_UPDATE_KEY to the ' +
        "private key file `chain updates key` made (and CHAIN_UPDATE_KEY_PASSWORD if it has one)." +
        (keyPath ? `\n${keyPath} doesn't exist.` : "")
    );
    process.exit(1);
  }
  return {
    TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(keyPath, "utf8").trim(),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.CHAIN_UPDATE_KEY_PASSWORD ?? ""
  };
}

/** The --config that makes Tauri build and sign the update archive. */
export function updaterBuildConfig(config: UpdatesConfig): string[] {
  const conf = {
    bundle: { createUpdaterArtifacts: true },
    plugins: { updater: { pubkey: config.publicKey, endpoints: [config.feed] } }
  };
  return ["--config", JSON.stringify(conf)];
}

/** `CHAIN_SIGNING_IDENTITY` (a keychain code-signing identity's name or
 * SHA-1) as the identity Tauri signs the app with. A stable identity keeps
 * the app's permissions across updates; ad-hoc signing doesn't. */
export function signingIdentityConfig(): string[] {
  const identity = process.env.CHAIN_SIGNING_IDENTITY;
  return identity ? ["--config", JSON.stringify({ bundle: { macOS: { signingIdentity: identity } } })] : [];
}

/** The architecture an update is for, from `--target`, else this Mac's. */
export function updateArch(args: string[]): { triple?: string; arch: "aarch64" | "x86_64" | "universal" } {
  const index = args.indexOf("--target");
  const triple = index === -1 ? undefined : args[index + 1];
  if (triple === "universal-apple-darwin") return { triple, arch: "universal" };
  if (triple?.startsWith("x86_64")) return { triple, arch: "x86_64" };
  if (triple?.startsWith("aarch64")) return { triple, arch: "aarch64" };
  return { triple, arch: process.arch === "x64" ? "x86_64" : "aarch64" };
}

/** The feed's platform keys an architecture's archive serves. */
export function platformKeys(arch: "aarch64" | "x86_64" | "universal"): string[] {
  return arch === "universal" ? ["darwin-aarch64", "darwin-x86_64"] : [`darwin-${arch}`];
}

interface Feed {
  version: string;
  notes?: string;
  pub_date: string;
  platforms: Record<string, { signature: string; url: string }>;
}

/** The feed with this build's platforms added: kept from `previous` when
 * it's the same version, started afresh when it isn't. */
export function mergeFeed(
  previous: Feed | undefined,
  build: { version: string; notes?: string; date: string; keys: string[]; signature: string; url: string }
): Feed {
  const kept = previous?.version === build.version ? previous : undefined;
  const platforms = { ...kept?.platforms };
  for (const key of build.keys) platforms[key] = { signature: build.signature, url: build.url };
  const notes = build.notes ?? kept?.notes;
  return { version: build.version, ...(notes ? { notes } : {}), pub_date: build.date, platforms };
}

/** Copies the signed archive into target/updates/ under a versioned name and writes the feed. */
export function writeUpdateFiles(cwd: string, config: UpdatesConfig, args: string[], notes: string | undefined): string[] {
  const { name, version } = readAppIdentity(cwd);
  const { triple, arch } = updateArch(args);
  const target = path.join(nativeProjectDir(cwd), "target");
  const bundleDir = path.join(target, ...(triple ? [triple] : []), "release", "bundle", "macos");
  const archive = path.join(bundleDir, `${name}.app.tar.gz`);
  if (!fs.existsSync(archive) || !fs.existsSync(`${archive}.sig`)) {
    throw new Error(`the signed update archive isn't at ${archive}(.sig)`);
  }
  const outDir = path.join(target, "updates");
  fs.mkdirSync(outDir, { recursive: true });
  const fileName = `${name.replace(/\s+/g, "-")}_${version}_${arch}.app.tar.gz`;
  fs.copyFileSync(archive, path.join(outDir, fileName));
  fs.copyFileSync(`${archive}.sig`, path.join(outDir, `${fileName}.sig`));

  const feedPath = path.join(outDir, "latest.json");
  const previous = fs.existsSync(feedPath) ? (JSON.parse(fs.readFileSync(feedPath, "utf8")) as Feed) : undefined;
  const feed = mergeFeed(previous, {
    version,
    notes,
    date: new Date().toISOString(),
    keys: platformKeys(arch),
    signature: fs.readFileSync(`${archive}.sig`, "utf8").trim(),
    url: new URL(encodeURIComponent(fileName), config.feed).toString()
  });
  fs.writeFileSync(feedPath, `${JSON.stringify(feed, null, 2)}\n`);
  return [path.join(outDir, fileName), feedPath];
}

/** `chain updates key <file>`: a new signing key pair, the private half kept out of the app. */
export function updatesKey(args: string[]): void {
  const cwd = process.cwd();
  const [file] = args;
  if (!file) {
    console.error("Usage: chain updates key <file>   (the private key's path, outside the app's folder)");
    process.exit(1);
  }
  const keyPath = path.resolve(file);
  if (!path.relative(cwd, keyPath).startsWith("..")) {
    console.error(`Error: ${keyPath} is inside this folder. Keep the private key out of the app's repo.`);
    process.exit(1);
  }
  if (fs.existsSync(keyPath)) {
    console.error(`Error: ${keyPath} already exists. Choose another path; a key can't be replaced without stranding installed apps.`);
    process.exit(1);
  }
  const tauriBin = resolveTauriBin(cwd);
  if (!fs.existsSync(tauriBin)) {
    console.error("Error: Tauri CLI not found in node_modules/.bin — run this inside a Chain app after `npm install`.");
    process.exit(1);
  }
  const made = spawnSync(tauriBin, ["signer", "generate", "--write-keys", keyPath], { stdio: "inherit" });
  if (made.status !== 0) process.exit(made.status ?? 1);
  const publicKey = fs.readFileSync(`${keyPath}.pub`, "utf8").trim();
  console.log(`
Private key: ${keyPath}  — keep it safe and out of any repo; losing it means installed copies can't update.
Add to package.json:

  "chain": {
    "updates": {
      "feed": "https://…/latest.json",
      "publicKey": "${publicKey}"
    }
  }

Then build with CHAIN_UPDATE_KEY=${keyPath} chain build`);
}
