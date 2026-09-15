# Chain CLI (`chain`)

## How it works

`chain` is a small TypeScript CLI, structured as a single-file command
router rather than a framework (no commander/yargs — deliberately
minimal, matches the "basic scaffolding" scope it was built for). It's
written in `.ts` and compiled to plain JS (`npm run build`, wired as the
package's `prepare` script so `npm install`/`npm link` build it
automatically) — the published/linked entry point is the compiled
`dist/bin.js`, never a `.ts` file directly. `src/bin.ts` reads its own
`package.json` for the version, parses `process.argv`, and switches on
the first argument to `init`, `update`, or `doctor`.

### `chain init <project-name>` — scaffold a new app

Resolves `<project-name>` against `process.cwd()`, so it always creates a
new folder relative to wherever you ran the command (like `npm create
vite`) — it does not take an arbitrary path. **It only creates new
projects** — it errors out if the target already exists, rather than
merging into it (an existing app is `chain update`'s job, not `init`'s).

Under the hood it shells out to `create-tauri-app` (React + TypeScript
template) — the same tool `apps/playground` was bootstrapped with — then
calls `writeTrackedFiles()` (`src/scaffold.ts`) to lay down every
framework-owned file on top of that: dependency/script wiring, Tailwind,
the router structure, the `chain-core` Rust bridge, and branded icons.
See `scaffold.ts`'s file list below for exactly what that covers — `init`
and `update` share the same generation logic, so this doc doesn't
duplicate it per-file.

One thing worth calling out because it's a real trap, not just detail:
`create-tauri-app` defaults `package.json`'s `dev`/`build` scripts to
frontend-only Vite. `init` renames those originals to `dev:web`/
`build:web` and makes `dev`/`build` run `tauri dev`/`tauri build` (the
real thing) — desirable, but it means `tauri.conf.json`'s
`beforeDevCommand`/`beforeBuildCommand` must be repointed at
`dev:web`/`build:web` too, or they recurse into `tauri dev`/`tauri build`
infinitely. Both patches live in `scaffold.ts` and are applied together;
don't change one without the other.

After writing every tracked file, `init` also snapshots them into
`.chain/baseline/` inside the new app — this is what makes `chain update`
possible later (see below). Commit `.chain/` to the app's own repo; it's
not build output.

### `chain update` — pull in chain-sdk changes without losing your edits

Resetting a project every time chain-sdk's templates change would erase
whatever the developer built since `init`. `chain update` instead does a
proper three-way merge, the same mechanism git itself uses for merge
conflicts:

- **baseline** = `.chain/baseline/<path>`, a snapshot of what the file
  looked like right after the last `init`/`update` — the merge ancestor.
- **ours** = the file's current content in the app (the developer's
  edits, if any).
- **theirs** = what the file would look like if generated fresh right
  now — for pure templates (`Home.tsx`, `AGENTS.md`, ...) that's just
  re-reading `packages/cli/templates/`; for patched files (`package.json`,
  `vite.config.ts`, `tauri.conf.json`, `Cargo.toml`) it's re-applying the
  _current_ patch function to the **baseline**, not to a fresh
  `create-tauri-app` run — `update` never re-scaffolds.

For each tracked file (`src/scaffold.ts`'s `TRACKED_FILES`):

- `theirs === baseline` → nothing changed upstream, skip silently (the
  common case — keeps output quiet).
- `ours === baseline` → developer never touched it, safe to overwrite
  with `theirs` directly.
- otherwise → both sides changed; shell out to `git merge-file` for a
  real three-way text merge. Non-overlapping changes merge cleanly with
  no developer involvement. Overlapping changes get real `<<<<<<< /
======= / >>>>>>>` conflict markers written directly into the file —
  the same thing developers already know how to resolve from git, no new
  tool to learn. See `mergeFile()` in `src/update.ts`.
- Icons (binary — can't be text-merged) use hash comparison instead:
  `ours === baseline` byte-for-byte → safe to replace; anything else →
  skip and leave the developer's custom icon alone.
- A file present in the new templates but absent from baseline (a
  capability added to `chain-sdk` after this app was scaffolded) is
  copied in directly — nothing to merge yet.
- A tracked file the developer deleted is left deleted, not resurrected.

After merging, the baseline is updated to `theirs` regardless of whether
a conflict occurred, so the _next_ `chain update` diffs from here forward
— an unresolved conflict doesn't get re-flagged forever once you fix it.

A project scaffolded before `chain update` existed has no
`.chain/baseline/` — the first `chain update` run detects this, records
the app's _current_ files as the starting baseline, changes nothing, and
explains that future runs will merge properly from that point on (there's
no way to reconstruct what the original template looked like, so a real
merge on that first run isn't possible).

### `chain doctor` — Rust toolchain check

Checks for the Rust toolchain (`cargo`) and, only with an explicit `y/N`
prompt (never silently), installs it via the official rustup.rs script.
It refuses to prompt when stdin isn't a TTY (e.g. run from a script or
CI) and just reports status instead — see `src/doctor.ts`'s `confirm()`.

## How to use it

```bash
# one-time, per clone of this repo
cd packages/cli && npm link

# from anywhere on the machine after that
chain init my-app       # scaffolds ./my-app in the current directory
chain update            # run from inside an existing app to merge in chain-sdk changes
chain doctor            # check (and optionally install) the Rust toolchain
chain --help / -h       # list commands
chain --version / -v    # print the CLI version
```

Inside a scaffolded app, `npm run dev`/`npm run build` are the real thing
(`tauri dev`/`tauri build`) — use `npm run dev:web`/`npm run build:web`
for frontend-only iteration when you don't need the native shell.

If `chain update` reports a conflict, resolve the `<<<<<<< / ======= /

> > > > > > > `markers by hand before running`npm install` or building —
> > > > > > > conflicted JSON/TOML won't parse until you do.

## Files to check

- `packages/cli/src/bin.ts` — argument parsing / command routing
  (`init`, `update`, `doctor`, `--help`, `--version`, unknown-command and
  no-args handling). Start here for anything about how a flag or
  subcommand is recognized.
- `packages/cli/src/scaffold.ts` — the shared source of truth for both
  `init` and `update`: `TRACKED_FILES` (every framework-owned path and
  how to regenerate it), the idempotent `patch*` functions
  (`package.json`, `vite.config.ts`, `tauri.conf.json`, `Cargo.toml`),
  and `desiredContent()`. **Add a new framework-owned file here, not
  directly in `init.ts`** — otherwise `chain update` won't know about it.
- `packages/cli/src/init.ts` — runs `create-tauri-app`, calls
  `writeTrackedFiles()`, snapshots `.chain/baseline/`, `npm install`,
  `git init`.
- `packages/cli/src/update.ts` — the three-way merge: `mergeFile()`
  (wraps `git merge-file`), `syncTextFile()` (per-file merge decision
  tree), `syncIconDir()` (binary hash-compare), and the no-baseline
  bootstrap path.
- `packages/cli/templates/` — the files `scaffold.ts` copies verbatim
  (with `{{name}}` substitution in `AGENTS.md`): `App.tsx`, `router.tsx`,
  `layouts/RootLayout.tsx`, `components/NavBar.tsx`, `Home.tsx` (the
  `platform.getInfo()` proof page), `About.tsx`, `App.css`, `AGENTS.md`,
  `lib.rs`. Edit these to change what a newly scaffolded app looks
  like — don't edit generated output by hand, and don't rename a
  template file without updating its `TRACKED_FILES` entry.
- `packages/cli/src/doctor.ts` — the Rust prerequisite check and the
  rustup install flow (`doctor()`), plus `checkRustQuietly()`, the silent
  check `init.ts` uses for its post-scaffold note.
- `packages/cli/tsconfig.json` — compiles `src/*.ts` to `dist/*.js`
  (`outDir: dist`, `rootDir: src`). TypeScript preserves the `#!/usr/bin/env
node` shebang line from `bin.ts` into `dist/bin.js` automatically.
- `packages/cli/package.json` — CLI package metadata. `bin` points at
  `./dist/bin.js` (compiled, not source) — that's what `npm link` exposes
  as the global `chain` command; `version` is what `chain --version`
  prints; `scripts.prepare` runs the TS build automatically on install/link.
- If `chain` isn't found on `PATH` after `npm link`, check that
  `npm config get prefix`'s `bin/` directory is on `$PATH`.
- `asset/app-icon.svg` and `asset/icons/` — the placeholder branding
  copied into new apps and re-synced by `chain update` (unless a
  developer has replaced their own).
- In a scaffolded app: `.chain/baseline/` — the merge ancestor `chain
update` needs. Don't delete or gitignore it.
