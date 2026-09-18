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
the first argument to `init`, `dev`, `build`, `update`, `migration`,
`database`, or `doctor`.

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
`build:web` and makes `dev`/`build` run the real thing — `chain dev`/
`chain build` (which wrap `tauri dev`/`tauri build`, see below) —
desirable, but it means `tauri.conf.json`'s
`beforeDevCommand`/`beforeBuildCommand` must be repointed at
`dev:web`/`build:web` too, or they recurse into `tauri dev`/`tauri
build` infinitely. Both patches live in `scaffold.ts` and are applied
together; don't change one without the other.

`create-tauri-app` also always writes the native project to `src-tauri/`
— `init` immediately renames that to the hidden `.chain/native/` (see
below) before any tracked-file patching happens, so a developer scaffolding
a new app never sees a `src-tauri` folder even transiently.

After writing every tracked file, `init` also snapshots them into
`.chain/baseline/` inside the new app — this is what makes `chain update`
possible later (see below). Commit `.chain/` to the app's own repo; it's
not build output.

### The hidden native project: `.chain/native`

The Tauri/Rust side of an app isn't at the conventional `src-tauri/` —
it's at `.chain/native/`, dot-prefixed and nested under the same `.chain/`
directory `.chain/baseline/` already lives in. The reasoning (and the
Tauri-CLI mechanism that makes it possible) is worth understanding before
touching any of this:

- `crates/core` holds the real native logic; a scaffolded app's own
  `.chain/native/src/lib.rs` is just a thin generated wrapper `chain
  update` regenerates (see `templates/lib.rs`) — there's rarely a reason
  for an app developer to open it, so it's hidden the way `node_modules`
  is, not deleted or excluded from git.
- This only works because `@tauri-apps/cli` (v2.0.4+) does **not**
  hardcode the `src-tauri` name: it fast-checks
  `$CWD/src-tauri/tauri.conf.json`, then falls back to a tree walk — and
  it reads a `TAURI_APP_PATH` env var that points it at an arbitrary
  native-project directory instead. `nativeProject.ts`'s `tauriEnv()` sets
  `TAURI_APP_PATH` to `.chain/native`'s absolute path on every spawn — see
  its own comment, and `@tauri-apps/cli`'s `CHANGELOG.md` around v2.0.4
  ("Support custom project directory structure...") if you need to verify
  this against a newer Tauri CLI version later.
- `.chain/native/` sits two levels under the app root (`.chain/native/`
  vs. the old one-level `src-tauri/`) — the only content difference this
  causes is `tauri.conf.json`'s `frontendDist`, which is `"../../dist"`
  instead of `"../dist"` (`patchTauriConf` in `scaffold.ts`).
  `beforeDevCommand`/`beforeBuildCommand` stay plain strings (`"npm run
  dev:web"`) unchanged — `npm run` itself walks up looking for the
  nearest `package.json` regardless of the cwd it's invoked from, so the
  extra nesting level doesn't matter there.
- Running `tauri dev`/`tauri build` directly (bypassing `chain`) won't
  find `.chain/native` — `TAURI_APP_PATH` is what makes it discoverable,
  and only `chain dev`/`chain build` set it. This is intentional: it's
  the enforcement mechanism, not just documentation, that keeps the
  native project effectively "chain-owned."
- **Migrating an existing app** (one still on the old `src-tauri/`
  layout): `chain update` does this automatically as a one-time step
  before its normal tracked-file loop — see its section below. A plain
  `fs.renameSync` (same filesystem) preserves the Cargo `target/` build
  cache, so migrating doesn't trigger a full rebuild.

### `chain dev` — run the dev server behind condensed, branded output

Run from inside a scaffolded app (a scaffolded `package.json`'s `dev`
script is `chain dev`, wired by `patchPackageJson` in `scaffold.ts` — see
the trap called out above, this is the same script slot). Instead of
showing raw `tauri dev` output (cargo's `Compiling` spam, Vite's banner,
Tauri CLI's own `Info` lines all interleaved), `chain dev` spawns
`node_modules/.bin/tauri dev` itself (with `TAURI_APP_PATH` pointed at
`.chain/native`, see above), captures its stdout/stderr, and prints
condensed, colored status lines (`Frontend`/`Native`, each with a ● ready
/ ✘ error / → building glyph) as a normal scrolling log — the same
relationship `expo start` has to Metro's raw bundler output.

The path-resolution and output-condensing logic is shared with `chain
build` (below) rather than duplicated: `nativeProject.ts` has
`resolveTauriBin()`, `nativeProjectDir()`, `checkChainApp()` (the
"doesn't look like a Chain app" / "still on src-tauri, run `chain
update`" error paths), and `tauriEnv()` (sets `TAURI_APP_PATH` plus
`CARGO_TERM_COLOR`/`FORCE_COLOR`); `nativeOutput.ts` has the condensing
state/types and `processLine()`/`maybePrintStatus()` described next.
`dev.ts` itself only holds what's specific to the long-running
interactive case: the restart/quit process-tree-kill logic below.

**Deliberately not a full-screen redrawn dashboard.** An earlier version
cleared the screen and repainted a fixed dashboard on a timer
(`\x1b[2J\x1b[H` every ~120ms). That broke in terminals/panes that don't
honor screen-clear escapes (several IDE-embedded terminal panes don't) —
the clear was silently dropped and every repaint just appended a full
frame, so the pane filled with the header printed over and over. `dev.ts`
now only ever prints a new line when something real happened, and never
clears anything, so it degrades to plain scrolling text everywhere
instead of depending on real terminal emulation:

- `processLine()` (src/nativeOutput.ts) pattern-matches known cargo/Tauri
  CLI/Vite lines and prints via `maybePrintStatus()`, which only emits a
  line on
  an actual status change (starting → building → ready/error) or, while
  still `building`, at most once per `BUILDING_UPDATE_THROTTLE_MS` (400ms)
  — that's what collapses a stream of "Compiling X v1.2.3" lines into
  occasional progress updates instead of one line per crate.
- `warning:`/`error:` lines flip `inDiagnosticBlock` on, so every
  indented continuation line of that diagnostic (the `-->`, `|`, snippet
  lines cargo prints under it) is printed in full until the next blank
  line — condensing status never means hiding a real build failure.
  Lines that don't match anything known still print (dim) rather than
  being swallowed.
- Colors are only emitted when `process.stdout.isTTY`; keyboard shortcuts
  (`r` restart, `v` toggle raw passthrough, `q`/Ctrl+C quit — kills the
  child, restores stdin) only attach when `process.stdin.isTTY`. Without
  a readable-keys TTY (CI, a task runner, piped output) it still prints
  the same condensed log and exits with the child's own exit code when
  `tauri dev` exits — there's no separate "non-interactive fallback" path
  to keep in sync with the normal one.
- A `generation` counter bumped on every (re)spawn guards `r`-restart:
  it stops a just-killed child's late stdout/exit events from clobbering
  the freshly restarted state.
- **Restart/quit kill the whole process tree, not just the `tauri` pid.**
  `tauri dev` spawns Vite and, once built, the app binary as its own
  child processes; a plain `child.kill()` only signals the immediate
  `tauri` process and leaves those running — the old Vite dev server
  keeps holding its port, so the next spawn's Vite fails with `Port ...
  already in use` / `beforeDevCommand terminated with a non-zero status
  code`, and the old app window never closes. `spawnTauri()` spawns with
  `detached: true` so the child is the leader of its own process group;
  `killChildTree()` (src/dev.ts) signals the negative pid to reach that
  whole group on POSIX (`taskkill /pid <pid> /T /F` on Windows, which has
  no process groups for this), escalating to `SIGKILL` after a 3s grace
  period, and resolves only once the child has actually exited. Both `r`
  and `q` await it before doing anything else — `r` before calling
  `spawnTauri()` again, `q` before `process.exit()`.
- Extra args after `chain dev` are forwarded to `tauri dev` as-is (e.g.
  a scripted `chain dev -- --release`, though scaffolded apps don't need
  this).
- `@chain/cli` itself is added as a `file:`-linked devDependency of every
  scaffolded app (same pattern as `@chain/sdk`) so `node_modules/.bin/chain`
  resolves locally — `npm run dev`/`npm run build` don't depend on `chain`
  being installed/linked globally on the machine.

### `chain build` — release build behind the same condensed output

A scaffolded app's `build` script is `chain build`, the same relationship
`chain dev` has to `dev` (`patchPackageJson` in `scaffold.ts`). `build.ts`
reuses `nativeProject.ts`/`nativeOutput.ts` exactly like `dev.ts` does —
same `TAURI_APP_PATH` env, same `processLine()` condensing (this matters
more here, not less: `--release` builds with `lto = true`,
`codegen-units = 1` are slower and spammier than a dev build). It's
simpler than `dev.ts` because a build only ever spawns once and runs to
completion rather than staying interactive: no restart/quit keys, no
`generation` counter. It still spawns with `detached: true` and kills the
whole process group on `SIGINT`/`SIGTERM` (`killTree()` in `build.ts`) so
an aborted release build doesn't leave orphaned `rustc`/`cargo` processes
— the same class of bug `chain dev`'s restart handling had to fix, just
for the one-shot case. Exits with the child's own exit code.

### `chain update` — pull in chain-sdk changes without losing your edits

Resetting a project every time chain-sdk's templates change would erase
whatever the developer built since `init`. `chain update` instead does a
proper three-way merge, the same mechanism git itself uses for merge
conflicts.

**Before any of that**, `update()` runs a one-time migration: if
`src-tauri/` exists and `.chain/native/` doesn't, it moves the whole
directory with `fs.renameSync` (same filesystem — cheap, and it carries
the Cargo `target/` build cache along instead of forcing a full rebuild),
plus `.chain/baseline/src-tauri` → `.chain/baseline/.chain/native` if a
baseline snapshot already exists. After that, the normal loop below runs
against the new `.chain/native/...` paths exactly as it would for any
other tracked-file change — a developer's own edits to, say,
`tauri.conf.json`'s window size survive the merge the same way any other
non-overlapping customization would.

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

### `chain inspect` — dev-only automation bridge

Electron dev builds can be driven by Playwright's `_electron` launcher
because Electron bundles Chromium and exposes the Chrome DevTools
Protocol. Tauri renders through the OS's native webview (WKWebView on
macOS, WebView2 on Windows, WebKitGTK on Linux) — there's no CDP, and
Playwright has no Tauri equivalent. `chain inspect` is Chain's own
answer: a REPL (or one-shot flags — see below) that connects to a
dev-only bridge compiled into the app and lets you `eval`/`click`/`text`/
`wait`/`screenshot`/`drag` against the live window, the same job
Electron's remote-debugging port does for Playwright.

**Compiled in only during `chain dev`, never `chain build`.** `dev.ts`
passes `--features chain-dev-inspector` to `tauri dev`; `build.ts` never
does, so a release binary contains none of `.chain/native/src/dev_inspector.rs`'s
real logic — see that file's module doc for exactly what stays compiled
either way (a trivial, inert command stub) versus what's fully gated
(everything that actually listens on a socket or touches the webview).
Verified by grepping a release binary's strings: `__chain_inspector_report`
(the always-present stub) shows up, but none of the runtime bridge's
messages (`"listening on"`, `"unauthorized"`, etc.) do.

**How it works:** on `.setup()`, the app binds a `TcpListener` on
`127.0.0.1:0` (OS-assigned free port), generates a per-run random token
(a same-machine speed bump, not cryptographic auth), and writes
`{"port", "token"}` as JSON to `target/chain-inspector.json` — inside the
native project's own already-gitignored `target/`, found by walking up
from `std::env::current_exe()` so it doesn't depend on the process's
working directory. `chain inspect` polls for that file, connects over
plain TCP (deliberately not WebSocket — a browser page literally cannot
open a raw TCP socket, so this is immune to the "malicious webpage drives
your local dev server" class of attack a WS-based bridge would have), and
speaks newline-delimited JSON.

The wire protocol has exactly two commands: `eval` and `rect`. `click`/
`click-text`/`text`/`wait`/`type` are just JS snippets sent through
`eval` (same idea as wrapping `page.evaluate()` in a Playwright driver).
Tauri's `WebviewWindow::eval()` is fire-and-forget, so the requested code
is wrapped to call back into the `__chain_inspector_report` command with
its result; the TCP handler blocks on that callback (via an `mpsc`
channel in Tauri-managed state, 10s timeout) before replying. `rect`
returns the webview's physical-pixel geometry from `inner_position()`/
`inner_size()`/`scale_factor()` — no JS involved.

**`screenshot`/`drag` (macOS only) are deliberately Node-side, not Rust.**
Screen capture and synthetic input both go through macOS's TCC
permissions (Screen Recording, Accessibility, Automation), granted **per
binary identity**. If this logic ran inside `dev_inspector.rs` (the app's
own process), the grant would need re-approving on every `cargo build`
during `chain dev`, since dev binaries rebuild constantly. Doing it in
`chain inspect` itself (a stable, non-rebuilding process) means the
permission is granted once and stays granted. This also means zero new
Cargo dependencies and zero native interop in the shipped app binary —
the Rust side only gained `rect`; everything OS-specific lives in
`inspect.ts`.

- **`screenshot [path] [--selector <sel>]`** shells out to `screencapture
  -x -R x,y,w,h` (confirmed empirically: an N-point `-R` rect produces an
  N×scaleFactor-pixel image — points, not physical pixels). No selector
  captures the whole webview viewport; `--selector` crops to one
  element's `getBoundingClientRect()`.
- **`drag <x1> <y1> <x2> <y2> [--selector <sel>]`** shells out to
  `cliclick` (`dd:`/`dm:`/`du:` — **`dm:` is drag-move, not the plain
  `m:` move**; `m:` emits a `mouseMoved` event that native text/drag
  selection ignores, `dm:` emits `mouseDragged`, confirmed by testing
  both directly against a real drag-select) for a genuine OS-level
  mousedown → drag → mouseup, not `dispatchEvent`. First activates the
  app via `osascript`/System Events (best-effort) — a synthetic
  mousedown on a window that isn't frontmost (the common case: an
  agent's own terminal has focus) just raises the window instead of
  registering as a real drag start. `cliclick` isn't installed by
  default; `chain inspect` reuses `doctor.ts`'s exact pattern
  (`confirm()` — never installs silently, never prompts without a TTY)
  to offer `brew install cliclick`.
- **The window/element → screen-coordinate math is non-obvious — read
  `viewportRegion()` in `inspect.ts` before touching it.** `rect` reports
  physical pixels; screencapture/cliclick want points, so everything
  divides by `scaleFactor` once. More importantly: Tauri's
  `inner_position()`/`inner_size()` measure the window's *content view*,
  not the WKWebView's actual on-screen viewport — there's a real gap
  between them (confirmed against a live window: `window.innerHeight`
  read 32pt less than what `rect` implied) even with an overlay title
  bar. That gap isn't a portable constant, so `viewportRegion()` measures
  it fresh every call via `window.innerWidth`/`innerHeight` rather than
  hardcoding it — this is what makes selector-relative screenshots and
  drags land on the right pixels regardless of a given app's title-bar
  style.
- **Permissions are one-time OS grants to whatever terminal runs `chain
  inspect`** (Screen Recording for `screenshot`; Accessibility for
  `drag`'s synthetic input; Automation, for `drag`'s window-activation
  step, to let that terminal script System Events) — not to the app
  binary. macOS can't grant these to a headless/agent-driven process via
  a dialog it never sees clicked; if missing, expect a quiet failure
  (empty/undersized image, a drag that silently does nothing) rather
  than a loud crash. `screenshot` checks the output file size and raises
  a pointed error if it looks empty.

**One-shot invocation:** `chain inspect --eval "<js>"` / `--rect` /
`--screenshot <path> [--selector <sel>]` / `--drag <x1> <y1> <x2> <y2>
[--selector <sel>]` run exactly one command, print the result, and exit
with a real code — no REPL banner, safe to pipe/script. Omitting a flag
falls through to the REPL, unchanged.

**Non-goals:** Windows/Linux `screenshot`/`drag` — different native
mechanisms entirely (WebView2 `CapturePreview` + `SendInput` on Windows;
`grim`/`gnome-screenshot` + `xdotool`/`ydotool` depending on X11 vs
Wayland on Linux), unverified without those platforms; live console/error
tailing (would need injecting a `console.error`/`warn` override plus a
new streaming wire message type); no auto-reconnect after `chain dev`
restarts (exits with a clear message instead); the token is a
same-machine speed bump, not real auth; `rect`/screenshot/drag resolve
the `"main"` window only, same limitation `eval` already has.

```bash
# terminal 1, inside an app
chain dev

# terminal 2, same app directory
chain inspect
inspect> eval document.title
inspect> click-text About
inspect> text
inspect> wait #root
inspect> screenshot ./out.png --selector ".page-editor-content"
inspect> drag 0 10 400 10 --selector ".page-editor-content"
inspect> quit

# or one-shot, for scripting:
chain inspect --screenshot ./out.png --selector ".page-editor-content"
```

### `chain migration <name>` — scaffold the next SQLite migration file

`desktop.storage.migrate(migrations: Migration[])` (see the storage
capability's `CONTRACT.md`) is intentionally low-level: no ORM, no
generated typed models, no query builder — the app owns its schema and
writes its own SQL. What it doesn't own is the bookkeeping around that:
picking the next version number, naming the file consistently, and
keeping an index array in sync as migrations accumulate — exactly the
error-prone-by-hand part `rails generate migration` or `knex migrate:make`
automate in their frameworks. `chain migration <name>` is that, and
**only** that — it does not read your table definitions, diff a schema,
or generate SQL, unlike EF Core's `dotnet ef migrations add` (which
reflects over your C# model and writes the `Up()`/`Down()` diff for you).
Doing that here would mean chain owning a declarative schema DSL and a
diffing engine — real scope, and squarely the "no ORM" non-goal above.
`chain migration` stays on the file/numbering/wiring side of that line;
the SQL inside the generated file is always a `-- TODO`.

The convention it targets — established by Mneme, the first app to need
this — is a `db/` folder holding two siblings:

- `db/migrations/000N-<name>.ts` — one file per version, each exporting
  a single `{ version, sql }` `Migration` object, oldest first. Never
  edited by hand after it ships; the next `chain migration` run only
  ever adds a new one.
- `db/schema/<table>.ts` — one file per table, each a hand-written
  `<Table>Row` interface documenting that table's current full column
  list (with which migration added what) — the one place to see the
  whole database without replaying migration history. `chain migration`
  never touches this folder; updating it to match a migration you just
  wrote is the app developer's job, the same way writing the migration's
  SQL is.

`findMigrationsDir()` (`src/migration.ts`) locates `db/migrations` by
walking the app's `src/` tree looking for a directory named `migrations`
whose parent is named `db` — chain doesn't care how deep it is or what's
above `db/`, so a plain `chain init` app (`src/lib/db/migrations`) and an
app that later reorganized into features (e.g. Mneme's
`src/shared/lib/db/migrations`) both just work. **The convention isn't
baked into `chain init`** — deliberately: plenty of chain apps won't use
SQLite at all, so there's no default `db/` folder to find on a fresh app.
The first time `chain migration` runs in an app with no `db/migrations`
yet, it scaffolds one at the `chain init` default location
(`src/lib/db/`) — `db/index.ts` (the `initDb()` wrapper), and
`db/schema/index.ts` pre-filled with the convention above as a comment,
so the "instructions" travel with the code instead of living only in
this doc.

Every run (first or not) then:

1. Picks the next version by regexing existing `NNNN-*.ts` filenames in
   the migrations folder for their leading number and taking
   `max + 1` (or `1` if the folder was just created).
2. Slugifies `<name>` for the filename and derives a matching camelCase
   identifier for the exported `const` — purely from the string, no file
   parsing, so this is one deterministic transform used both when
   writing a new file and when regenerating the index below.
3. Writes `db/migrations/000N-<slug>.ts` with the `Migration` skeleton.
4. **Regenerates `db/migrations/index.ts` from scratch** by re-listing
   every `NNNN-*.ts` file in the folder and re-deriving each one's
   identifier the same way — this file is entirely chain-generated
   output, never hand-edited, so overwriting it wholesale each run is
   safe and avoids any brittle surgical-insert-into-existing-file logic.
   Each import uses an explicit `./NNNN-<slug>.ts` extension (not
   extensionless) — Vite/tsc's `allowImportingTsExtensions` (already on
   in the scaffold's tsconfig) accepts either, but `chain database`
   (below) loads this file directly via Node's native TypeScript support,
   whose ESM resolver has no extension-probing and needs the real one.

### `chain database update` / `chain database list` — operate on the real database

`chain migration` (above) only scaffolds a migration file — something has
to actually run it against a database. Normally that's the app itself,
via `desktop.storage.migrate()` on startup. `chain database update` is
the CLI-side equivalent: apply every pending migration straight to the
app's real SQLite file, without launching the app at all (useful in CI,
or right after writing a migration when you don't want to boot the whole
UI just to test it). `chain database list` is the read-only companion —
shows every migration with its applied/pending status, mirroring
`dotnet ef migrations list`.

**Both operate on literally the same file the running app would open —
not a copy, not a dev-only stand-in.** `resolveDbPath()`
(`nativeProject.ts`) replicates Tauri's own `app.path().app_data_dir()`
exactly (`dirs::data_dir().join(identifier)`, verified against tauri
2.11.5's and dirs 6.0.0's vendored source — macOS:
`~/Library/Application Support/<identifier>`; Windows: `%APPDATA%/
<identifier>`; Linux: `$XDG_DATA_HOME` or `~/.local/share`/`<identifier>`)
joined with `app.db`, matching `templates/lib.rs`'s `get_db()`. The
`<identifier>` comes from `.chain/native/tauri.conf.json`, so this only
works once an app has scaffolded/updated to that layout.

`database.ts` loads `db/migrations/index.ts` with a plain `import()` —
these files are simple object literals behind type-only imports, well
within what Node's native TS type-stripping (stable, unflagged, since
Node 22.18 — see the `engines` field in `packages/cli/package.json`)
handles, so no bundler or transpile step is needed just to read them.

`update`'s SQL execution deliberately mirrors `crates/core/src/storage.rs`'s
`Database::migrate` line for line — same `_chain_migrations` tracking
table (`CREATE TABLE IF NOT EXISTS ... version INTEGER PRIMARY KEY,
applied_at TEXT NOT NULL DEFAULT (datetime('now'))`), same "run the
migration's SQL as one batch, then insert its version row, no wrapping
transaction beyond that" behavior — so the CLI applying a migration and
the app applying it later (or vice versa) are indistinguishable to
`_chain_migrations`, whichever one runs second just sees its work already
done. Uses `node:sqlite`'s `DatabaseSync` (experimental as of Node 22.5,
prints one `ExperimentalWarning` per invocation — not worth suppressing
for a CLI tool; attaching a `process.on("warning", ...)` listener does
*not* stop Node's own default stderr print for this particular warning,
confirmed empirically, so don't try). `list` opens with `{ readOnly:
true }` and skips opening entirely if `app.db` doesn't exist yet, so it
never creates a file as a side effect of merely listing.

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
chain dev                # run from inside an app — condensed, branded `tauri dev`
chain build              # run from inside an app — condensed, branded `tauri build`
chain inspect            # run from inside an app while `chain dev` is running —
                         # REPL to eval/click/read the live window
chain update            # run from inside an existing app to merge in chain-sdk changes
chain migration <name>  # run from inside an app to scaffold the next db/migrations/000N-<name>.ts
chain doctor            # check (and optionally install) the Rust toolchain
chain --help / -h       # list commands
chain --version / -v    # print the CLI version
```

Inside a scaffolded app, `npm run dev`/`npm run build` run `chain dev`/
`chain build` (the real `tauri dev`/`tauri build`, behind the condensed
output described above, pointed at the hidden `.chain/native` project) —
use `npm run dev:web`/`npm run build:web` for frontend-only iteration
when you don't need the native shell.

If `chain update` reports a conflict, resolve the `<<<<<<< / ======= /

> > > > > > > `markers by hand before running`npm install` or building —
> > > > > > > conflicted JSON/TOML won't parse until you do.

## Files to check

- `packages/cli/src/bin.ts` — argument parsing / command routing
  (`init`, `dev`, `build`, `inspect`, `update`, `migration`, `database`,
  `doctor`, `--help`, `--version`, unknown-command and no-args handling).
  Start here for anything about how a flag or subcommand is recognized.
- `packages/cli/src/migration.ts` — `chain migration`: `findMigrationsDir()`
  (the `src/`-walk that locates `db/migrations` wherever an app keeps
  it), `scaffoldDb()` (first-run `db/index.ts` + `db/schema/index.ts`
  setup at the `chain init` default location), `slugify()`/`camelCase()`
  (the one naming transform used both for a new file and for
  regenerating the index), `nextVersion()`, and `regenerateIndex()`
  (rewrites `db/migrations/index.ts` wholesale from whatever `NNNN-*.ts`
  files are on disk — see the section above for why that's safe). Reuses
  `nativeProject.ts`'s `checkChainApp()` like every other command that
  must run from inside an app.
- `packages/cli/src/nativeProject.ts` — shared by `dev.ts`/`build.ts`/
  `inspect.ts`/`database.ts`: `resolveTauriBin()`, `nativeProjectDir()`
  (`.chain/native`), `checkChainApp()` (the "doesn't look like a Chain
  app" / "still on src-tauri, run `chain update`" errors), `tauriEnv()`
  (sets `TAURI_APP_PATH` — this is *the* mechanism that makes
  `.chain/native` discoverable to Tauri's CLI, see the section above),
  `inspectorInfoPath()` (where `chain inspect` finds the running bridge's
  port+token), `resolveDbPath()` (where `chain database` finds `app.db` —
  see its section above for the exact per-OS algorithm and why it has to
  match Tauri's own).
- `packages/cli/src/database.ts` — `chain database update`/`chain
  database list`: `loadMigrations()` (the `import()` of `db/migrations/
  index.ts`), `update()` (mirrors `crates/core/src/storage.rs`'s
  `Database::migrate` — see the section above), `list()` (read-only,
  never creates `app.db`). Reuses `findMigrationsDir()` from
  `migration.ts` and `checkChainApp()`/`resolveDbPath()` from
  `nativeProject.ts`.
- `packages/cli/src/nativeOutput.ts` — shared by `dev.ts`/`build.ts`: the
  condensed-output state/types, `processLine()`, `maybePrintStatus()`,
  `makeColor()`. Change cargo/Vite/Tauri-CLI output parsing here, not in
  either command file, or the two commands will drift.
- `packages/cli/src/dev.ts` — `chain dev`: spawns `tauri dev` via the
  shared helpers above and handles the `r`/`v`/`q` keys and the
  restart/quit process-tree-kill logic (`killChildTree()`, the
  `generation` counter, `suppressExitMessage`) — this file only holds
  what's specific to staying interactive; see the sections above for
  both mechanisms.
- `packages/cli/src/build.ts` — `chain build`: spawns `tauri build` via
  the same shared helpers, simpler than `dev.ts` (one spawn, runs to
  completion, no restart/quit keys), but still process-group-kills on
  `SIGINT`/`SIGTERM` (`killTree()`) so an aborted build doesn't leave
  orphaned `rustc`/`cargo` processes.
- `packages/cli/src/inspect.ts` — `chain inspect`: waits for the info file,
  connects, and runs a serialized `for await...of` REPL over the readline
  interface (plain `repl.on("line", async ...)` would race ahead of a
  command's socket round trip on piped/scripted input) — see its own
  comment for why, plus one-shot flag dispatch (`--eval`/`--rect`/
  `--screenshot`/`--drag`) before the REPL is even set up. `js.*` builds
  the JS snippets for `click`/`click-text`/`text`/`wait`/`type`, all sent
  through the `eval` wire command. `viewportRegion()` is the shared
  window/element → screen-coordinate math `screenshotRegion()` and
  `performDrag()` both build on — read its comment before changing either;
  the title-bar/traffic-light inset it corrects for is measured live, not
  a constant (see the `chain inspect` section above for why that matters).
- `packages/cli/templates/dev_inspector.rs` (mirrored by hand in
  `apps/playground/src-tauri/src/dev_inspector.rs`) — the actual bridge:
  `TcpListener` on a `chain-dev-inspector`-feature-gated thread, the
  eval-wrap-and-await-callback round trip, `rect`'s window-geometry
  query, the info-file writer (port/token/pid — pid is read by
  `inspect.ts`'s `drag` to activate the app window via `osascript` before
  synthesizing input). Its own module doc explains what stays compiled
  unconditionally (a trivial command stub) versus what's feature-gated
  (everything real) and why.
- `packages/cli/src/scaffold.ts` — the shared source of truth for both
  `init` and `update`: `TRACKED_FILES` (every framework-owned path and
  how to regenerate it — the native-project entries are `.chain/native/...`,
  not `src-tauri/...`), the idempotent `patch*` functions (`package.json`,
  `vite.config.ts`, `tauri.conf.json`, `Cargo.toml`), and
  `desiredContent()`. `patchPackageJson` is what wires a scaffolded app's
  `dev`/`build` scripts to `chain dev`/`chain build` and adds the
  `@chain/cli` `file:` devDependency that makes them resolvable locally.
  `scaffoldContext()`'s `coreRelative` and `patchTauriConf()`'s
  `frontendDist` are both computed relative to `.chain/native`, not
  `src-tauri` — see the depth note above before changing either.
  `patchTauriConf()` also sets `app.security.assetProtocol` (`enable:
  true`, `scope: ["$APPDATA/files/*"]`) and `patchCargoToml()` adds the
  `"protocol-asset"` Cargo feature to the `tauri` dependency — both
  required for `desktop.files.url()` to actually work (see the `files`
  capability's `AGENTS.md` for the real bug this fixes: without either
  one, `convertFileSrc()` produces a syntactically valid `asset://` URL
  that the webview refuses outright). **Add a new framework-owned file
  here, not directly in `init.ts`** — otherwise `chain update` won't know
  about it.
- `packages/cli/src/init.ts` — runs `create-tauri-app` (which still
  writes `src-tauri/`), immediately renames that to `.chain/native/`,
  then calls `writeTrackedFiles()`, snapshots `.chain/baseline/`, `npm
  install`, `git init`.
- `packages/cli/src/update.ts` — the one-time `src-tauri` →
  `.chain/native` migration (see above), then the three-way merge:
  `mergeFile()` (wraps `git merge-file`), `syncTextFile()` (per-file
  merge decision tree), `syncIconDir()` (binary hash-compare), and the
  no-baseline bootstrap path.
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
