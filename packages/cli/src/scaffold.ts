import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { CHAIN_CORE_GIT_URL, CHAIN_CORE_REV, CHAIN_SDK_VERSION } from "./publishMeta.js";

// The root of @chain/cli's own installed package — packages/cli, whether
// that's this monorepo's copy (local dev / `npm link`) or an npm-installed
// node_modules/@chain/cli elsewhere. Never assume a chain-sdk monorepo
// sits above this — once published, it doesn't.
export const chainRoot = path.resolve(fileURLToPath(new URL(".", import.meta.url)), "..");

export function readTemplate(name: string): string {
  return fs.readFileSync(path.join(chainRoot, "templates", name), "utf8");
}

export interface ScaffoldContext {
  target: string;
  name: string;
  sdkVersion: string;
  cliVersion: string;
  /** The chain-sdk checkout a new app links to instead of npm and git. */
  checkout?: string;
}

/** `linkCheckout`: when this CLI runs from a chain-sdk checkout (`npm
 * link`), point @chain/sdk, @chain/cli and chain-core at it — only `init`
 * asks, so `update` never rewrites an existing app's dependencies. */
export function scaffoldContext(target: string, linkCheckout = false): ScaffoldContext {
  const name = path.basename(target);
  const ownPkg = JSON.parse(fs.readFileSync(path.join(chainRoot, "package.json"), "utf8")) as {
    version: string;
  };
  const checkout = linkCheckout ? localCheckout() : undefined;
  return { target, name, sdkVersion: CHAIN_SDK_VERSION, cliVersion: ownPkg.version, checkout };
}

/** The chain-sdk repo around this CLI, or undefined once it's installed from npm. */
function localCheckout(): string | undefined {
  const root = path.resolve(chainRoot, "../..");
  const installed = chainRoot.split(path.sep).includes("node_modules");
  return !installed && fs.existsSync(path.join(root, "crates/core/Cargo.toml")) ? root : undefined;
}

function relativeTo(from: string, to: string): string {
  return path.relative(from, to).split(path.sep).join("/");
}

/** A local link already in the app (from a local `init`, or added by hand)
 * is kept; otherwise the checkout's when linking one, else the npm range. */
function chainPackageSpec(current: string | undefined, ctx: ScaffoldContext, pkg: string, version: string): string {
  if (current?.startsWith("file:")) return current;
  if (ctx.checkout) return `file:${relativeTo(ctx.target, path.join(ctx.checkout, "packages", pkg))}`;
  return `^${version}`;
}

/**
 * Patchers below are applied either to create-tauri-app's fresh output
 * (by `init`) or to the last-synced baseline (by `update`) — they must
 * stay idempotent so re-applying one to its own prior output is a no-op,
 * since `update` regenerates "the current desired state" by re-running
 * these against whatever the baseline holds.
 */

export function patchPackageJson(raw: string, ctx: ScaffoldContext): string {
  const pkg = JSON.parse(raw);
  const scripts: Record<string, string> = { ...pkg.scripts };
  // Capture create-tauri-app's original Vite-only commands under :web
  // exactly once — on a re-patch, scripts.dev/build are already ours.
  if (!("dev:web" in scripts)) scripts["dev:web"] = pkg.scripts.dev;
  if (!("build:web" in scripts)) scripts["build:web"] = pkg.scripts.build;
  // `dev`/`build` go through `chain dev`/`chain build`, which wrap
  // `tauri dev`/`tauri build` behind condensed output and point Tauri at
  // the hidden `.chain/native` project via TAURI_APP_PATH (see
  // packages/cli/src/dev.ts, build.ts, nativeProject.ts).
  scripts.dev = "chain dev";
  scripts.build = "chain build";
  pkg.scripts = scripts;
  pkg.dependencies = {
    ...pkg.dependencies,
    "@chain/sdk": chainPackageSpec(pkg.dependencies?.["@chain/sdk"], ctx, "sdk", ctx.sdkVersion),
    "react-router-dom": "^7",
    clsx: "^2"
  };
  pkg.devDependencies = {
    ...pkg.devDependencies,
    tailwindcss: "^4",
    "@tailwindcss/vite": "^4",
    "@chain/cli": chainPackageSpec(pkg.devDependencies?.["@chain/cli"], ctx, "cli", ctx.cliVersion)
  };
  return JSON.stringify(pkg, null, 2) + "\n";
}

export function patchViteConfig(raw: string): string {
  if (raw.includes("@tailwindcss/vite")) return raw;
  return raw
    .replace(
      'import react from "@vitejs/plugin-react";',
      'import react from "@vitejs/plugin-react";\nimport tailwindcss from "@tailwindcss/vite";'
    )
    .replace("plugins: [react()]", "plugins: [react(), tailwindcss()]");
}

// The exact subdirectory name the `files` capability writes to — see
// `dir.join("files")` in templates/lib.rs's `with_files`. Kept in sync by
// hand between the two; there's no shared constant to import across the
// Rust/TS boundary for a single string like this.
const FILES_CAPABILITY_SCOPE = "$APPDATA/files/*";

export function patchTauriConf(raw: string): string {
  const conf = JSON.parse(raw);
  conf.build.beforeDevCommand = "npm run dev:web";
  conf.build.beforeBuildCommand = "npm run build:web";
  // .chain/native/ is one level deeper than src-tauri/ used to be.
  conf.build.frontendDist = "../../dist";

  // Without this, desktop.files.url()'s asset:// URLs produce a
  // syntactically valid string that WebKit/WebView2 refuse outright
  // ("Load failed", not a 404) — Tauri's asset protocol requires an
  // explicit scope allowlist, default-empty. Additive: preserves any
  // scope entries the developer added themselves.
  conf.app.security ??= {};
  const assetProtocol = (conf.app.security.assetProtocol ??= { enable: false, scope: [] });
  assetProtocol.enable = true;
  assetProtocol.scope ??= [];
  if (!assetProtocol.scope.includes(FILES_CAPABILITY_SCOPE)) {
    assetProtocol.scope.push(FILES_CAPABILITY_SCOPE);
  }

  // chain-core's speech bridge is Swift (crates/core/build.rs), and Swift
  // concurrency only links from the OS's /usr/lib/swift when the app
  // targets macOS 12+ — below that the binary points at an @rpath copy
  // it can't find and won't launch. Raises an older minimum, keeps a higher one.
  conf.bundle ??= {};
  conf.bundle.macOS ??= {};
  const minimum = conf.bundle.macOS.minimumSystemVersion;
  if (!minimum || compareVersions(minimum, MIN_MACOS) < 0) {
    conf.bundle.macOS.minimumSystemVersion = MIN_MACOS;
  }

  return JSON.stringify(conf, null, 2) + "\n";
}

const MIN_MACOS = "12.0";

function compareVersions(a: string, b: string): number {
  const [pa, pb] = [a, b].map((v) => v.split(".").map(Number));
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

const DEV_INSPECTOR_FEATURE =
  "[features]\n" +
  "# Only `chain dev` passes --features chain-dev-inspector; `chain build`\n" +
  "# never does, so a release binary contains none of dev_inspector.rs.\n" +
  "chain-dev-inspector = []";

const DEV_PROFILE =
  "# Full debug info for Tauri's ~400 dependencies roughly doubles target/.\n" +
  "# Your own code keeps file:line in backtraces.\n" +
  "[profile.dev]\n" +
  'debug = "line-tables-only"\n' +
  "\n" +
  '[profile.dev.package."*"]\n' +
  "debug = false";

export function patchCargoToml(raw: string, ctx: ScaffoldContext): string {
  // A pinned git dependency, not a local path: chain-core lives in the
  // chain-sdk repo, not next to a scaffolded app, so the build has to work
  // on any machine (CI included), not just one with chain-sdk cloned as a
  // sibling folder. CHAIN_CORE_REV is baked in at @chain/cli's own build
  // time — see scripts/sync-meta.mjs. The exception is a local link, kept
  // like chainPackageSpec's: it has to match the checkout's templates.
  const nativeDir = path.join(ctx.target, ".chain/native");
  const depLine =
    raw.match(/^chain-core = \{ path = .*/m)?.[0] ??
    (ctx.checkout
      ? `chain-core = { path = "${relativeTo(nativeDir, path.join(ctx.checkout, "crates/core"))}" }`
      : `chain-core = { git = "${CHAIN_CORE_GIT_URL}", rev = "${CHAIN_CORE_REV}" }`);
  let out = /^chain-core = .*/m.test(raw)
    ? raw.replace(/^chain-core = .*/m, depLine)
    : raw.replace('serde_json = "1"', `serde_json = "1"\n${depLine}`);
  if (!/^\[features\]/m.test(out)) {
    out = out.replace(depLine, `${depLine}\n\n${DEV_INSPECTOR_FEATURE}`);
  }
  // create-tauri-app's staticlib/cdylib only exist for iOS/Android builds;
  // desktop links the rlib, and the two extra link outputs cost ~300 MB
  // of target/ and link time on every dev rebuild.
  out = out.replace(/^crate-type = \["staticlib", "cdylib", "rlib"\]$/m, 'crate-type = ["rlib"]');
  if (!/^\[profile\.dev\]/m.test(out)) {
    out = `${out.trimEnd()}\n\n${DEV_PROFILE}\n`;
  }
  // "protocol-asset" isn't one of Tauri's default Cargo features — without
  // it, the "asset:" URI scheme handler is compiled out entirely (not a
  // scope/config issue, a missing-handler one), so desktop.files.url()'s
  // convertFileSrc() output silently fails to load in the webview.
  // "unstable" makes Window::add_child public: desktop.browser's window
  // holds a toolbar webview above the page (agent-docs/capabilities/browser).
  out = out.replace(
    /^tauri = \{ version = "2", features = \[([^\]]*)\] \}$/m,
    (line: string, featuresRaw: string) => {
      const features = featuresRaw
        .split(",")
        .map((f) => f.trim())
        .filter(Boolean);
      for (const feature of ['"protocol-asset"', '"unstable"']) {
        if (!features.includes(feature)) features.push(feature);
      }
      return `tauri = { version = "2", features = [${features.join(", ")}] }`;
    }
  );
  return out;
}

export type TrackedFile =
  | { relPath: string; kind: "patched"; patch: (baseline: string, ctx: ScaffoldContext) => string }
  | { relPath: string; kind: "template"; templateName: string }
  | { relPath: string; kind: "copy"; sourcePath: string }
  | { relPath: string; kind: "binary-dir"; sourceDir: string };

// Every framework-owned file `init` writes and `update` later re-syncs.
// "patched" files are real create-tauri-app output we modify in place —
// their generator re-applies the patch to the baseline, not to a fresh
// create-tauri-app run (update never re-scaffolds). "template"/"copy"
// files are entirely ours, so the generator just re-reads the source.
export const TRACKED_FILES: TrackedFile[] = [
  { relPath: "package.json", kind: "patched", patch: patchPackageJson },
  { relPath: "vite.config.ts", kind: "patched", patch: (raw) => patchViteConfig(raw) },
  {
    relPath: ".chain/native/tauri.conf.json",
    kind: "patched",
    patch: (raw) => patchTauriConf(raw)
  },
  { relPath: ".chain/native/Cargo.toml", kind: "patched", patch: patchCargoToml },
  { relPath: ".chain/native/src/lib.rs", kind: "template", templateName: "lib.rs" },
  { relPath: ".chain/native/build.rs", kind: "template", templateName: "build.rs" },
  { relPath: ".chain/native/src/browser.rs", kind: "template", templateName: "browser.rs" },
  { relPath: ".chain/native/src/window.rs", kind: "template", templateName: "window.rs" },
  {
    relPath: ".chain/native/src/dev_inspector.rs",
    kind: "template",
    templateName: "dev_inspector.rs"
  },
  { relPath: "src/App.css", kind: "template", templateName: "App.css" },
  { relPath: "src/App.tsx", kind: "template", templateName: "App.tsx" },
  { relPath: "src/router.tsx", kind: "template", templateName: "router.tsx" },
  {
    relPath: "src/layouts/RootLayout.tsx",
    kind: "template",
    templateName: "layouts/RootLayout.tsx"
  },
  { relPath: "src/components/NavBar.tsx", kind: "template", templateName: "components/NavBar.tsx" },
  { relPath: "src/pages/Home.tsx", kind: "template", templateName: "Home.tsx" },
  { relPath: "src/pages/About.tsx", kind: "template", templateName: "About.tsx" },
  { relPath: "AGENTS.md", kind: "template", templateName: "AGENTS.md" },
  { relPath: "asset/app-icon.svg", kind: "copy", sourcePath: "asset/app-icon.svg" },
  { relPath: "asset/icons", kind: "binary-dir", sourceDir: "asset/icons" },
  { relPath: ".chain/native/icons", kind: "binary-dir", sourceDir: "asset/icons" }
];

/** What a tracked text file's content should be right now, given its
 * current baseline (the ancestor for a 3-way merge; ignored for
 * template/copy files, which have no meaningful "baseline" state). */
export function desiredContent(
  file: TrackedFile & { kind: "patched" | "template" | "copy" },
  baseline: string | undefined,
  ctx: ScaffoldContext
): string {
  if (file.kind === "template") {
    const content = readTemplate(file.templateName);
    return file.relPath === "AGENTS.md" ? content.replaceAll("{{name}}", ctx.name) : content;
  }
  if (file.kind === "copy") {
    return fs.readFileSync(path.join(chainRoot, file.sourcePath), "utf8");
  }
  if (baseline === undefined) {
    throw new Error(`patched file ${file.relPath} requires a baseline to regenerate from`);
  }
  return file.patch(baseline, ctx);
}
