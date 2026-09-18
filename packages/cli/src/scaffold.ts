import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const chainRoot = path.resolve(fileURLToPath(new URL(".", import.meta.url)), "../../..");

export function readTemplate(name: string): string {
  return fs.readFileSync(path.join(chainRoot, "packages/cli/templates", name), "utf8");
}

export interface ScaffoldContext {
  target: string;
  name: string;
  sdkRelative: string;
  coreRelative: string;
  cliRelative: string;
}

export function scaffoldContext(target: string): ScaffoldContext {
  const name = path.basename(target);
  const sdkRelative = path.relative(target, path.join(chainRoot, "packages/sdk"));
  const coreRelative = path.relative(
    path.join(target, ".chain/native"),
    path.join(chainRoot, "crates/core")
  );
  const cliRelative = path.relative(target, path.join(chainRoot, "packages/cli"));
  return { target, name, sdkRelative, coreRelative, cliRelative };
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
    "@chain/sdk": `file:${ctx.sdkRelative}`,
    "react-router-dom": "^7",
    clsx: "^2"
  };
  pkg.devDependencies = {
    ...pkg.devDependencies,
    tailwindcss: "^4",
    "@tailwindcss/vite": "^4",
    "@chain/cli": `file:${ctx.cliRelative}`
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

  return JSON.stringify(conf, null, 2) + "\n";
}

const DEV_INSPECTOR_FEATURE =
  '[features]\n' +
  '# Only `chain dev` passes --features chain-dev-inspector; `chain build`\n' +
  '# never does, so a release binary contains none of dev_inspector.rs.\n' +
  'chain-dev-inspector = []';

export function patchCargoToml(raw: string, ctx: ScaffoldContext): string {
  const depLine = `chain-core = { path = "${ctx.coreRelative}" }`;
  let out = /^chain-core = .*/m.test(raw)
    ? raw.replace(/^chain-core = .*/m, depLine)
    : raw.replace('serde_json = "1"', `serde_json = "1"\n${depLine}`);
  if (!/^\[features\]/m.test(out)) {
    out = out.replace(depLine, `${depLine}\n\n${DEV_INSPECTOR_FEATURE}`);
  }
  // "protocol-asset" isn't one of Tauri's default Cargo features — without
  // it, the "asset:" URI scheme handler is compiled out entirely (not a
  // scope/config issue, a missing-handler one), so desktop.files.url()'s
  // convertFileSrc() output silently fails to load in the webview.
  out = out.replace(
    /^tauri = \{ version = "2", features = \[([^\]]*)\] \}$/m,
    (line: string, featuresRaw: string) => {
      const features = featuresRaw
        .split(",")
        .map((f) => f.trim())
        .filter(Boolean);
      if (!features.includes('"protocol-asset"')) features.push('"protocol-asset"');
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
  { relPath: ".chain/native/tauri.conf.json", kind: "patched", patch: (raw) => patchTauriConf(raw) },
  { relPath: ".chain/native/Cargo.toml", kind: "patched", patch: patchCargoToml },
  { relPath: ".chain/native/src/lib.rs", kind: "template", templateName: "lib.rs" },
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
