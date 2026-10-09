# Folders Capability (`desktop.folders`)

## How it works

The user grants a folder (or a file) by picking it in the OS picker or
dropping it on the window. From then on the app works with real absolute
paths under it: list, read, write, move, delete and watch. Grants live in
`<app data>/folder-grants.json`, so they survive restarts. An app can
also declare folders it reads without asking, in its own `package.json`:

```json
"chain": { "readOnlyFolders": ["~/.claude/projects", "~/.codex/sessions"] }
```

`chain dev`/`chain build` validate that list and compile it into the app
(`CHAIN_READ_ONLY_FOLDERS`). Declared folders are read-only. Finally the
app has two folders of its own, `appFolder("data")` (persistent, at
`<app data>/app`) and `appFolder("temp")` (`$TMPDIR/<identifier>`).
They're always read-write and never listed by `grants()`.

Every call resolves its path natively first (`Folders::resolve` in
`crates/core/src/folders.rs`). The path must be absolute with no `..`.
The longest existing part is canonicalised (symlinks, `/var` →
`/private/var`, on-disk case), and the result must sit inside a grant
giving the needed access, or the call rejects with `NOT_GRANTED`.
Symlinks are followed only when their target is inside a grant too.

Writes are atomic (temp file beside the target, `fsync`, rename).
Watching is the `notify` crate (FSEvents on macOS). The picker is `rfd`,
as a sheet. Tauri commands live in `packages/cli/templates/folders.rs`
and run off the main thread. `process-runner` and `terminal` use the
same grants to check a `cwd`.

## How to use it

```ts
import { desktop } from "@chain/sdk";

// Grant: the user picks one or more folders. Remembered across restarts.
const [project] = await desktop.folders.pick();
if (!project) return; // cancelled

const all = await desktop.folders.grants(); // picked, dropped and declared
await desktop.folders.revoke(project.id);

// Drops grant only while a handler is subscribed.
const stop = desktop.folders.onDrop((grants) => openProjects(grants));

// Index a tree. .gitignore handling is yours; skipFolders is just names.
const entries = await desktop.folders.list(project.path, {
  recursive: true,
  skipFolders: ["node_modules", ".git", "dist"]
});

// Read and write.
const env = await desktop.folders.readText(`${project.path}/.env`);
await desktop.folders.writeText(`${project.path}/.env`, updated); // atomic
const preview = await desktop.folders.readBytes(imagePath, { maxBytes: 16 * 1024 * 1024 });
await desktop.folders.createFolder(`${project.path}/.lazify/api-studio`);
await desktop.folders.move(oldPath, newPath);
await desktop.folders.delete(path, { toTrash: true });

// Incremental read of a growing log in a declared folder (Mneme request 11).
const { size } = await desktop.folders.stat(log);
const fresh = await desktop.folders.readBytes(log, { offset: lastOffset });

// The app's own folders: real paths for process arguments and cwd.
const data = await desktop.folders.appFolder("data"); // persistent
const temp = await desktop.folders.appFolder("temp"); // the OS may clear it
const gitDir = `${data.path}/shadow-repos/${runId}/${name}`;
await desktop.folders.createFolder(gitDir);
await desktop.processRunner.run("git", [`--git-dir=${gitDir}`, `--work-tree=${project.path}`, "init"], onOutput);

// Watch. Reject NOT_FOUND means "skip it", e.g. an agent that never ran.
const watch = await desktop.folders.watch(dir, (changes) => schedule(changes), { recursive: true });
await watch.stop();
```

Errors: `NOT_GRANTED` (outside grants, or a write under a read-only one),
`PERMISSION_DENIED` (the OS refused, e.g. macOS privacy protection),
`NOT_FOUND`, `INVALID_ARGUMENT`, `UNAVAILABLE` (file locked),
`TOO_LARGE`, `UNSUPPORTED`. See `CONTRACT.md`.

Every scaffolded app gets this through `chain update`
(`.chain/native/src/folders.rs`).

## Files to check

- `agent-docs/capabilities/folders/CONTRACT.md` — the path rules, the
  grant model (the request 01/11 design fork), errors and non-goals.
- `capabilities/folders/contract.ts` — the exact types.
- `crates/core/src/folders.rs` — grants, `resolve()`, every operation,
  `watch()`, the picker, and the unit tests.
- `packages/cli/templates/folders.rs` (copy in
  `apps/playground/src-tauri/src/`) — Tauri commands, the
  `CHAIN_READ_ONLY_FOLDERS` read, drop handling, and per-page cleanup of
  watches and drop subscriptions.
- `packages/cli/templates/lib.rs` — registers the commands, chains
  `folders::on_window_event` after `window::on_window_event`, and calls
  `folders::release_page` on page load.
- `packages/cli/src/readOnlyFolders.ts` and `nativeProject.ts`'s
  `tauriEnv()` — validate and pass `chain.readOnlyFolders`.
- `packages/sdk/src/folders.ts` — the SDK wrapper; one shared listener for
  change and drop events; the raw `writeBytes` body.
- `agent-docs/capabilities/folders/research/` — macOS findings (case,
  TCC, listing speed) and the Windows checklist.
