// `.chain/native` is generated, the way Next.js generates `.next`: every
// file comes from chain-sdk's templates plus the app's package.json, so the
// same chain-sdk and package.json always give the same project. `chain dev`,
// `chain build` and `chain update` regenerate it, which brings back a
// deleted folder and repairs a stale or broken file. Only Cargo.lock is the
// app's (committed, so crate versions stay pinned).

import fs from "node:fs";
import path from "node:path";

import { CHAIN_CORE_GIT_URL, CHAIN_CORE_REV } from "./publishMeta.js";
import { chainRoot, readTemplate } from "./scaffold.js";
import { nativeProjectDir } from "./nativeProject.js";

export interface AppIdentity {
  /** The app's name: its bundle and default window title. */
  name: string;
  /** Sets where the app's data lives — never change it once users have data. */
  identifier: string;
  version: string;
  /** Cargo package name, from package.json's name. */
  crate: string;
}

interface PackageJson {
  name?: string;
  version?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  chain?: { app?: { name?: string; identifier?: string } };
}

function readPackageJson(cwd: string): PackageJson {
  return JSON.parse(fs.readFileSync(path.join(cwd, "package.json"), "utf8"));
}

/** What an earlier native project, generated or create-tauri-app's, said. */
function previousConf(cwd: string): { productName?: string; identifier?: string } {
  for (const dir of [nativeProjectDir(cwd), path.join(cwd, "src-tauri")]) {
    try {
      return JSON.parse(fs.readFileSync(path.join(dir, "tauri.conf.json"), "utf8"));
    } catch {
      // missing or unreadable: try the next, else defaults
    }
  }
  return {};
}

/** create-tauri-app's crate name for a project name. */
export function crateName(packageName: string): string {
  const words = packageName.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").split("-").filter(Boolean);
  return words.join("-") || "app";
}

/** The app's identity: package.json "chain.app", else what the last native
 * project used, else defaults from package.json's name. */
export function readAppIdentity(cwd: string): AppIdentity {
  const pkg = readPackageJson(cwd);
  const packageName = pkg.name ?? path.basename(cwd);
  const previous = pkg.chain?.app?.name && pkg.chain?.app?.identifier ? {} : previousConf(cwd);
  return {
    name: pkg.chain?.app?.name ?? previous.productName ?? packageName,
    identifier: pkg.chain?.app?.identifier ?? previous.identifier ?? `dev.chain.${packageName}`,
    version: pkg.version ?? "0.1.0",
    crate: crateName(packageName)
  };
}

/** Writes the identity into package.json "chain.app" when it isn't there,
 * so it can never drift — the identifier decides where users' data is. */
export function pinAppIdentity(cwd: string): AppIdentity {
  const identity = readAppIdentity(cwd);
  const pkgPath = path.join(cwd, "package.json");
  const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
  if (pkg.chain?.app?.name !== identity.name || pkg.chain?.app?.identifier !== identity.identifier) {
    pkg.chain = { ...pkg.chain, app: { ...pkg.chain?.app, name: identity.name, identifier: identity.identifier } };
    fs.writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);
    console.log(`  pin      package.json "chain.app": ${identity.name} (${identity.identifier})`);
  }
  return identity;
}

function relativeTo(from: string, to: string): string {
  return path.relative(from, to).split(path.sep).join("/");
}

/** chain-core from the same place @chain/sdk comes from: a linked checkout's
 * crates/core, else the git revision this CLI was published with. */
export function chainCoreDependency(cwd: string): string {
  const pkg = readPackageJson(cwd);
  const sdk = pkg.dependencies?.["@chain/sdk"];
  if (sdk?.startsWith("file:")) {
    const checkout = path.resolve(cwd, sdk.slice("file:".length), "../..");
    return `chain-core = { path = "${relativeTo(nativeProjectDir(cwd), path.join(checkout, "crates/core"))}" }`;
  }
  return `chain-core = { git = "${CHAIN_CORE_GIT_URL}", rev = "${CHAIN_CORE_REV}" }`;
}

// The `files` capability's folder — `dir.join("files")` in templates/lib.rs.
const FILES_CAPABILITY_SCOPE = "$APPDATA/files/*";

export function tauriConf(identity: AppIdentity): string {
  const conf = {
    $schema: "https://schema.tauri.app/config/2",
    productName: identity.name,
    version: identity.version,
    identifier: identity.identifier,
    build: {
      beforeDevCommand: "npm run dev:web",
      devUrl: "http://localhost:1420",
      beforeBuildCommand: "npm run build:web",
      frontendDist: "../../dist"
    },
    app: {
      windows: [{ title: identity.name, width: 800, height: 600 }],
      security: {
        csp: null,
        // Tauri's asset protocol needs an explicit scope for desktop.files.url().
        assetProtocol: { enable: true, scope: [FILES_CAPABILITY_SCOPE] }
      }
    },
    bundle: {
      active: true,
      targets: "all",
      icon: ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
      // chain-core's Swift speech bridge links the OS's Swift concurrency only on 12+.
      macOS: { minimumSystemVersion: "12.0" }
    }
  };
  return `${JSON.stringify(conf, null, 2)}\n`;
}

// Native sources that are templates as they are.
const SOURCE_TEMPLATES = [
  "lib.rs",
  "browser.rs",
  "browser_view.rs",
  "updates.rs",
  "window.rs",
  "pdf.rs",
  "folders.rs",
  "terminal.rs",
  "attention.rs",
  "dev_inspector.rs"
];

/** Every generated file's path in `.chain/native` and its content. */
export function nativeFiles(cwd: string, identity: AppIdentity): Map<string, string | Buffer> {
  const lib = `${identity.crate.replaceAll("-", "_")}_lib`;
  const files = new Map<string, string | Buffer>([
    [
      "Cargo.toml",
      readTemplate("native/Cargo.toml")
        .replace("{{crate}}", identity.crate)
        .replace("{{lib}}", lib)
        .replace("{{chainCore}}", chainCoreDependency(cwd))
    ],
    ["tauri.conf.json", tauriConf(identity)],
    ["build.rs", readTemplate("build.rs")],
    ["src/main.rs", readTemplate("native/main.rs").replace("{{lib}}", lib)],
    ["capabilities/default.json", readTemplate("native/capabilities/default.json")],
    [".gitignore", readTemplate("native/gitignore")]
  ]);
  for (const name of SOURCE_TEMPLATES) files.set(`src/${name}`, readTemplate(name));
  // The app's own icons (asset/icons) when it has them, else Chain's.
  const appIcons = path.join(cwd, "asset/icons");
  const iconDir = fs.existsSync(path.join(appIcons, "icon.icns")) ? appIcons : path.join(chainRoot, "asset/icons");
  for (const entry of fs.readdirSync(iconDir, { withFileTypes: true })) {
    if (entry.isFile()) files.set(`icons/${entry.name}`, fs.readFileSync(path.join(iconDir, entry.name)));
  }
  return files;
}

function sameContent(file: string, content: string | Buffer): boolean {
  try {
    return fs.readFileSync(file).equals(Buffer.from(content));
  } catch {
    return false;
  }
}

/** Brings `.chain/native` up to date, rewriting only files whose content
 * differs (an unchanged mtime spares Cargo a rebuild). Returns what it wrote. */
export function generateNativeProject(cwd: string): string[] {
  const identity = pinAppIdentity(cwd);
  const dir = nativeProjectDir(cwd);
  const written: string[] = [];
  for (const [relPath, content] of nativeFiles(cwd, identity)) {
    const file = path.join(dir, relPath);
    if (sameContent(file, content)) continue;
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, content);
    written.push(relPath);
  }
  return written;
}

const GITIGNORE_BLOCK =
  "# Chain: .chain/native is generated by chain dev/build — only its Cargo.lock\n" +
  "# is kept. Commit .chain/baseline/ (chain update's merge ancestor).\n" +
  "/.chain/native/*\n" +
  "!/.chain/native/Cargo.lock";

/** The app's .gitignore with Chain's rules: a blanket `.chain/` rule (it
 * hid the baseline and Cargo.lock) gives way to them. Idempotent. */
export function patchGitignore(raw: string): string {
  if (raw.includes("/.chain/native/*")) return raw;
  const without = raw
    .split("\n")
    .filter((line) => !/^\/?\.chain\/?$/.test(line.trim()))
    .join("\n");
  return `${without.trimEnd()}\n\n${GITIGNORE_BLOCK}\n`;
}

export function ensureGitignore(cwd: string): boolean {
  const file = path.join(cwd, ".gitignore");
  const raw = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
  const patched = patchGitignore(raw);
  if (patched === raw) return false;
  fs.writeFileSync(file, patched);
  return true;
}
