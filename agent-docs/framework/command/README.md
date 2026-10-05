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

**Shared build cache.** `chain dev` sets `CARGO_TARGET_DIR` to
`nativeProject.ts`'s `sharedDevTargetDir()` — `~/Library/Caches/chain/target`
on macOS, `$XDG_CACHE_HOME/chain/target` (or `~/.cache/…`) on Linux,
`%LOCALAPPDATA%\chain\cache\target` on Windows — unless the developer
already set their own `CARGO_TARGET_DIR`. The ~1 GB of compiled Tauri
dependencies is identical across Chain apps, so this is the same move as
Electron's shared Chromium cache: measured, the first app's debug build
costs ~1.1 GB and a second app adds ~250 MB (and built in 3 s, not 30 s).
It's a pure cache — deleting it only costs a rebuild. Two apps building at
once wait on Cargo's lock ("Blocking waiting for file lock") rather than
corrupting anything. `chain build` deliberately doesn't share it, so
release bundles still land in the app's own `.chain/native/target/release/bundle`.
The shared dir is also why the dev inspector's info file is no longer
located relative to the running binary (see `chain inspect` below).

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
- The Tauri CLI prints its own diagnostics as `Error <msg>` / `Warn <msg>`
  (capital, no colon), which the rustc patterns above miss. `^\s*Error\s`
  prints red, counts as an error and flips `Native` to ✘ even after a
  `Finished` (a bundling failure happens after cargo is done);
  `^\s*Warn\s` prints yellow. Both are single-line banners, so they
  don't open a diagnostic block. Every error line is kept in
  `state.lastErrorLine` for `printFailureSummary()`.
- When `tauri dev` exits non-zero, `printFailureSummary()` prints a red
  `✘ tauri dev failed (exit N).` line and repeats the last error line
  before the usual `── tauri dev exited ──` line.
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
- **Closing the terminal also tears down the child tree.** `dev.ts`
  listens for `SIGTERM`/`SIGINT`/`SIGHUP` and runs the same `quit()` ->
  `killChildTree()` path for all three. `SIGHUP` matters specifically
  because that's what closing the controlling terminal (window/tab
  closed, SSH session dropped) sends to the foreground process group —
  without a handler, Node's default disposition for `SIGHUP` is to
  terminate immediately, which skips `killChildTree()` and orphans the
  detached `tauri`/Vite/app process tree exactly like the bug the
  process-group kill above was written to fix.
- Extra args after `chain dev` are forwarded to `tauri dev` as-is (e.g.
  a scripted `chain dev -- --release`, though scaffolded apps don't need
  this).
- `@chain/cli` itself is added as a versioned devDependency of every
  scaffolded app (same pattern as `@chain/sdk`, see "Publishing" below) so
  `node_modules/.bin/chain` resolves locally — `npm run dev`/`npm run build`
  don't depend on `chain` being installed/linked globally on the machine.

### Publishing `@chain/sdk` and `@chain/cli`, and how `chain-core` is wired

Early on, `chain init` wired a scaffolded app's `@chain/sdk`/`@chain/cli`
dependencies as `file:<relative path back to this chain-sdk clone>`, and
`chain-core` as a Cargo `path = "..."` dependency the same way. Both only
ever worked on the machine that ran `chain init` — the relative path
assumed chain-sdk was cloned as a sibling folder at a fixed offset, so
`npm install`/`cargo build` broke outright anywhere else (a teammate's
machine, CI, or even this same machine after moving a folder). That's
exactly the gap that blocks `chain build` from producing something you can
actually hand off, so this is now fixed:

- **`chain-core`** (`patchCargoToml` in `scaffold.ts`) is a pinned **git**
  dependency: `chain-core = { git = "<chain-sdk's origin URL>", rev =
"<commit>" }`. Cargo resolves a named crate from anywhere in a cloned
  repo's workspace, so no `path:`/subdirectory trick is needed — this works
  from any machine with network access, verified against the real
  `github.com/Anuboost-Long/chain-sdk` remote (see that PR's verification
  notes).
- **`@chain/sdk`/`@chain/cli`** are ordinary versioned npm dependencies
  (`^<version>`) — unlike Cargo, npm's git-dependency spec has no supported
  way to install a single package out of a subdirectory of a monorepo (only
  the unrelated `repository.directory` metadata field), so the only real
  fix was publishing both to the public npm registry.
- Both of those values — the git URL/rev and `@chain/sdk`'s version — can't
  be read at `chain init` runtime once `@chain/cli` is installed from npm
  (there's no chain-sdk checkout sitting next to it any more). They're
  baked into `@chain/cli`'s own build via `packages/cli/scripts/sync-meta.mjs`,
  which writes `packages/cli/src/publishMeta.ts` (gitignored, generated —
  never hand-edit it) from the _actual_ chain-sdk repo state, and only ever
  runs while this really is that repo (`prepare`/`prepublishOnly`, both
  no-ops on a registry install). `sync-meta.mjs --strict` (what
  `prepublishOnly` uses) refuses to run against a dirty working tree or a
  commit that hasn't been pushed to `origin` — publishing `@chain/cli`
  pinned to a commit nobody else can fetch would silently break every
  future `chain init`/`chain update`.
- `@chain/cli` also needs its own `templates/` and icon `asset/` bundled
  inside the published package (they used to be read via a repo-root-
  relative `chainRoot`, computed by walking three directories up from
  `dist/` — correct only when `packages/cli` sits inside this monorepo).
  `chainRoot` (`scaffold.ts`) now resolves to `@chain/cli`'s own installed
  location; `packages/cli/scripts/sync-assets.mjs` copies the icons in from
  the repo-root `asset/` into `packages/cli/asset/` (gitignored, generated)
  so they ship with the package. Both scripts run automatically via
  `prepare`/`prepublishOnly` — see `packages/cli/package.json`.
- `@chain/sdk` itself couldn't be published as-is either: every SDK module
  imported its capability's structural types straight from
  `../../../capabilities/<name>/contract.ts`, which only exists inside this
  monorepo — a published tarball only contains `packages/sdk`'s own
  directory. `packages/sdk/scripts/sync-contracts.mjs` copies each
  `capabilities/<name>/contract.ts` into `packages/sdk/src/contracts/`
  (gitignored, generated — `capabilities/<name>/contract.ts` stays the
  canonical source, per root `AGENTS.md` rule 1), and every SDK source file
  imports from there instead. Runs via the same `prepare`/`prepublishOnly`
  pattern.
- **To actually publish a new version**: bump `version` in
  `packages/sdk/package.json` and/or `packages/cli/package.json`, commit
  and push to `origin`, then `npm publish` from inside each package
  (`prepublishOnly` regenerates `publishMeta.ts`/`sync-contracts`/
  `sync-assets` and rejects an unpushed commit automatically). The next
  `chain init`/`chain update` picks up the new pins automatically — nothing
  else to wire by hand.
- Verified end to end without touching the real registry: `npm pack` both
  packages, installed the tarballs into a throwaway project with no
  chain-sdk checkout present, and confirmed `@chain/sdk` type-checks
  standalone and `chain init` (run from the packed `@chain/cli`) writes a
  `Cargo.toml`/`package.json` with the correct git/version pins — it only
  stops at `npm install` because `@chain/sdk` isn't actually published yet.
  Separately confirmed the `chain-core` git dependency itself resolves and
  builds against the real pushed `origin` remote.

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
for the one-shot case. Exits with the child's own exit code, after a
closing line: green `● Build finished` on success, or red
`✘ Build failed (exit N).` plus the last captured error line (via
`printFailureSummary()`) so the cause isn't lost in the scrollback —
e.g. Tauri's `Error failed to build app: Target x86_64-apple-darwin is
not installed …` when a `--target` triple's Rust target is missing.

### GPL opt-in — `package.json` "chain.gpl"

`"chain": { "gpl": true }` makes `chain dev`/`chain build` add
`--features chain-core/tts` (`src/features.ts`), which links sherpa-onnx's
TTS build including espeak-ng (GPL-3.0) and turns on `desktop.tts`. The
app's releases then carry GPL-3.0 obligations. Absent or `false`: no GPL
code. Any other value fails the run. See
`agent-docs/capabilities/models/research/LICENSING.md`.

### macOS 12 minimum and the app `build.rs`

chain-core's speech capability includes Swift (SpeechAnalyzer), which
links Swift Concurrency from `/usr/lib/swift`. Two tracked pieces make
that load in every build: `.chain/native/build.rs` (template `build.rs`)
adds `-rpath /usr/lib/swift` to the app binary — `tauri dev` builds for
Rust's default deployment target (11.0), where the linker records an
`@rpath` install name — and `patchTauriConf` raises
`bundle.macOS.minimumSystemVersion` to `12.0` (a higher value is kept).
Without the rpath the app dies at launch with `dyld: Library not loaded:
@rpath/libswift_Concurrency.dylib`. Existing apps get both from `chain
update`.

### App permissions — `package.json` "chain.permissions"

An app declares OS permissions in its own `package.json`, never in
`.chain/native/`:

```json
"chain": {
  "permissions": {
    "microphone": "Why the app records audio.",
    "speechRecognition": "Why the app transcribes audio.",
    "systemAudio": "Why the app records what the computer plays."
  }
}
```

Each value is the sentence the OS prompt shows. On every run, `chain dev`
and `chain build` (`src/permissions.ts`) write `.chain/native/Info.plist`
with the matching `NS…UsageDescription` keys (Tauri embeds it in the dev
binary and merges it into the bundle) and, for `microphone`,
`.chain/native/Entitlements.plist` with
`com.apple.security.device.audio-input`, which `chain build` passes to
`tauri build` as a `--config` override. Both files are marked GENERATED
and removed when nothing is declared; a hand-written `Info.plist` is left
alone with a warning. An unknown key fails the run. Changing a
declaration needs no `chain update`. `systemAudio` becomes
`NSAudioCaptureUsageDescription` and needs no entitlement. See
`agent-docs/capabilities/microphone/`, `speech/` and `audio-recorder/`.

### Permission prompts under `chain dev`

macOS asks for a privacy-gated permission (microphone, speech
recognition, ...) on behalf of the _responsible_ process. A binary that
`tauri dev` starts inherits responsibility from whatever terminal or
editor ran `chain dev`, so without help the request is judged as that
app's: no prompt, a silent `NotAllowedError`, and the Chain app never
listed in System Settings → Privacy & Security.

So the template's `run()` calls
`chain_core::dev_launch::become_responsible_for_itself()` first thing,
under the `chain-dev-inspector` feature (only `chain dev` passes it, so
`chain build` output never contains the call). If the process isn't
already responsible for itself, it relaunches its own binary with
`responsibility_spawnattrs_setdisclaim` — what terminals and editors do
for their own children — and waits, exiting with the relaunched copy's
status. The relaunched app then prompts with its own name and the
embedded Info.plist sentence, and gets its own Privacy row. Nothing
changes for the developer: same one command, same log stream (stdio is
inherited), same hot reload. The relaunched app holds the read end of a
pipe only the waiter can write to and exits on EOF, because `tauri dev`
SIGKILLs the pid it started on every Rust rebuild and a SIGKILL can't
be forwarded. Both functions are private libSystem SPI looked up with
`dlsym`, so if a future macOS drops them this falls back to the old
behavior instead of failing to launch. No-op on other OSes.

TCC records a dev binary's answer by its **path** (e.g.
`<cache>/target/debug/mneme`), not by the bundle identifier, and keeps
it across Rust rebuilds. `tccutil reset Microphone <id>` only takes
bundle identifiers, so it can't target the dev entry — to be asked
again, remove the row with the – button in System Settings → Privacy &
Security → Microphone (or `tccutil reset Microphone`, which resets every
app). In testing, switching the row _off_ in Settings didn't stick
across the next relaunch (macOS prompted again); answering the prompt
"Don't Allow" did. The built `.app` is a separate entry, keyed by its
bundle identifier.

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
native project's own already-gitignored `target/`, located via the
compiled-in `CARGO_MANIFEST_DIR` (dev builds only). Not next to the
binary: `chain dev` builds into a target dir shared by every Chain app
(see "Shared build cache" above), where two running apps would overwrite
each other's info file. Apps whose `dev_inspector.rs` predates this need
`chain update` before `chain inspect` finds them again. `chain inspect` polls for that file, connects over
plain TCP (deliberately not WebSocket — a browser page literally cannot
open a raw TCP socket, so this is immune to the "malicious webpage drives
your local dev server" class of attack a WS-based bridge would have), and
speaks newline-delimited JSON.

The wire protocol has four commands: `eval`, `rect`, `focus` and
`trace` (the last two below). `click`/
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
  `inner_position()`/`inner_size()` measure the window's _content view_,
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
falls through to the REPL, unchanged. So do `--focus` and
`--trace start|dump|stop [--json]`, below.

**Performance tracing — `--trace` and `--focus` (mneme request 28).**
`chain inspect --trace start`, then `--trace dump` (or `--trace stop`,
which also ends it) prints every native call made since start, grouped
by command or SQL text (count, total/median/p95 JS time, native handler
time, SQLite time, rows), frame gaps, and app/webview memory and CPU;
`--json` prints the full report instead (`TraceReport` in
`packages/cli/src/trace.ts`). It's assembled from three places:

- **The SDK** (`packages/sdk/src/native.ts`) — every capability imports
  `invoke` from there, not from `@tauri-apps/api/core`. While the page
  has `window.__chainTrace` (only ever set by `--trace start`), each
  call is recorded with its JS duration, argument and result sizes, and
  a summary (SQL text, or `METHOD url`), and sent with a `chain-trace`
  header carrying its id. Without a trace a call costs one property
  read. Tauri's `__TAURI_INTERNALS__.invoke` is non-writable, so this
  can't be done by wrapping it — and the SDK is the one place every
  app call goes through anyway.
- **The app** — `dev_inspector::traced()` wraps `generate_handler!`, so
  every command runs inside `chain_core::dev_trace::command()`, which
  records its handler time under the header's id; storage's
  `query`/`execute` file each statement's SQLite time and row count
  against the command running on that thread (`record_sql`). A sync
  command's handler time is its whole native cost; an **async**
  command (http, tts, models, speech, vision, files pick/save) returns
  once its future is spawned, so its handler time is only dispatch —
  read its JS time instead. Tauri has no public hook for when an async
  command responds; its `tracing` feature has one, but it re-serializes
  every request body on every call, trace or not, which would slow every
  `chain dev`. A sampler thread records RSS and CPU every 250 ms for the
  app process and the webview's: on macOS the WKWebView's WebContent
  process (WebKit's private `_webProcessIdentifier`, dev-only), which
  isn't the app's child; elsewhere the app's child processes (WebView2's).
  All of it is `chain-core`'s `dev-trace` feature (with its optional
  `sysinfo` dependency), which only `chain dev` passes, so none of it is
  in a release binary.
- **The page** — `--trace start` evals a recorder (`startScript` in
  `trace.ts`) that counts `requestAnimationFrame` gaps over 25/33.4/
  50 ms (25, not 16.7: WKWebView rounds `performance.now()` to whole
  milliseconds, so on-time 60 Hz frames measure 16–17 ms — the first
  run counted 690 of 1166 frames as "late"; for the same reason JS-side
  call times are whole ms on macOS, native/SQLite times are precise), keeps gaps over 50 ms with the calls in flight during them,
  logs visibility changes (and restarts gap timing after a hidden
  stretch, since hidden pages don't render), and observes long tasks
  where the engine has the API (WebView2 yes, WKWebView no — frame gaps
  stand in for it there).

`--focus` brings the window forward with Tauri's own
`unminimize`/`show`/`set_focus`, so a hidden window renders again
without the macOS Automation/Screen Recording prompts osascript or
screencapture would raise. `--trace start` warns when the page is
hidden.

Verified on macOS in mneme's running window (1 October 2026): read-only
navigation across courses, recordings and Recently deleted traced 44
calls, all 44 joined to their native records with SQL time and rows
(e.g. 0.26 ms handler, 0.05 ms SQLite for one grouped `SELECT`), the
WebContent process was found and sampled, `--focus` raised the window
with no prompt, and `dump` after `stop` reported no running trace.
Verified on macOS only; the Windows paths (child-process webview
sampling, WebView2 long tasks) compile from shared code but haven't run.

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

### `chain migration …` / `chain database …` — model-first migrations, EF Core style

Driven by the user's explicit ask for `dotnet ef`-equivalent tooling: the
schema is **declared as classes**, and migrations (Up **and** Down SQL)
are **generated** from changes to them, with a full applied-history
table. Command-for-command:

| Chain                                          | EF Core                                               |
| ---------------------------------------------- | ----------------------------------------------------- |
| `chain migration add <name>`                   | `dotnet ef migrations add`                            |
| `chain migration add <name> --empty`           | an empty migration to hand-write                      |
| `chain migration remove`                       | `dotnet ef migrations remove`                         |
| `chain migration list` / `chain database list` | `dotnet ef migrations list`                           |
| `chain migration script [from] [to]`           | `dotnet ef migrations script`                         |
| `chain migration check`                        | `dotnet ef migrations has-pending-model-changes`      |
| `chain database update [target]`               | `dotnet ef database update [target]` (up **or down**) |
| `chain database scaffold [--force]`            | `dotnet ef dbcontext scaffold`                        |

**The model.** `db/schema/*.ts` holds `@Table` classes (decorators from
`@chain/sdk/schema`, `packages/sdk/src/schema.ts`). The decorators do
nothing at runtime — `src/schema/read-classes.ts` reads the files with the
**TypeScript compiler**, the way EF reads C# types: `string` → TEXT,
`number`/`boolean`/`bigint` → INTEGER, `Uint8Array` → BLOB, `| null` or `?`
→ nullable, every property is a column unless `@NotMapped()`, a property
named `id` is the key unless `@PrimaryKey` says otherwise, a class name
maps to a snake_case table. Decorator arguments must be literals (they're
read, not run); errors carry `file:line`. The classes double as row types
(`desktop.storage.query<Course>(…)`), replacing the old hand-written
`XRow` interfaces. Supported: types, nullability, literal and SQL-expression
defaults, single and composite primary keys, AUTOINCREMENT, unique
(column and composite), column and table CHECKs, single-column foreign
keys with actions, plain/unique/partial indexes, and triggers (`@Trigger`,
full `CREATE TRIGGER` text — tracked because a table rebuild drops a
table's triggers). Not supported: views, expression indexes, generated
columns, composite foreign keys — `scaffold` reports these; manage them
with `--empty` migrations.

**Snapshots.** Every generated migration `NNNN-<name>.ts` gets a sibling
`NNNN-<name>.model.json`: the whole schema right after it (EF's per-
migration model snapshot, canonicalized by `src/schema/model.ts`). `add`
diffs the classes against the latest one; `remove` deletes both files.
Commit them.

**The diff** (`src/schema/diff.ts`) emits Up, and Down by running the same
diff in reverse. In-place when SQLite allows it (CREATE/DROP TABLE,
ADD COLUMN with a constant default or nullable, RENAME COLUMN/TABLE,
DROP COLUMN when nothing references the column, indexes, triggers);
otherwise the documented 12-step **table rebuild** (create
`__chain_new_<t>`, copy, drop, rename, recreate indexes and triggers).
Like EF, a new NOT NULL column without a default backfills existing rows
with the type's empty value (`''`, `0`, `X''`), and nullable → NOT NULL
coalesces NULLs; every such data-affecting step is listed as a warning in
the command output and in the migration file's header. Renames can't be
inferred from a diff: in a terminal, `add` asks when a table/column
disappears and a same-typed one appears (drizzle-kit style); otherwise
pass `--rename-table old=new` / `--rename-column table.old=new`. Each
migration is wrapped in `BEGIN … COMMIT`.

**History.** `_chain_migrations` gained `name` and `checksum` (FNV-1a 64
of the Up SQL; `crates/core/src/storage.rs` and `src/schema/history.ts`
compute it identically and upgrade older tables in place). `list` flags
`changed` (edited after it ran) and `missing` (applied, file gone).
Hand-written migrations without `name` show their file name.

**The runner** — both the app's (`storage.rs`) and the CLI's
(`history.ts`) — turns `foreign_keys` off (a rebuild's DROP would fire
ON DELETE actions) and `legacy_alter_table` on (so SQLite ≥ 3.26 doesn't
re-validate other tables' triggers mid-rebuild), restores both afterwards,
and rolls back a migration that fails part-way. The `legacy_alter_table`
part also fixed a real bug: mneme's hand-written migration 6 failed on
every **fresh** database (both SQLite 3.46 in the app and 3.51 in Node),
so a new mneme install couldn't create its database.

**Down migrations** run only from the CLI (`chain database update
<earlier>`); the app only migrates up. Hand-written migrations without a
`down` can't be reverted, and `update` refuses before touching anything.

**Adopting an existing app** (`chain database scaffold`): replays every
migration into an in-memory database (never the real one), introspects it
(`src/schema/introspect.ts` — PRAGMAs plus the stored CREATE statements
for AUTOINCREMENT/CHECK/UNIQUE/partial WHERE), writes the classes
(`src/schema/emit-classes.ts`), **verifies they read back as the identical
model** (it fails loudly otherwise), and saves the model as the latest
migration's snapshot. Old migrations stay as they are. Refuses to
overwrite existing schema files without `--force`.

Verified: `npm test` in `packages/cli` (16 engine tests on real SQLite —
each change applied up, compared with what the database actually
contains, applied down, compared again, with data checks); mneme's full
history (15 migrations, 12 tables with CHECKs and triggers) scaffolds and
round-trips exactly; a scratch app went through add → update → check →
rename-with-data → revert → remove → re-add → script; the decorator syntax
builds under the app template's `tsc` and Vite.

### `chain clean` — reclaim build-cache disk space

Cargo never deletes stale artifacts itself (no stable target-dir GC as
of cargo 1.98 — `-Z gc` is nightly-only and only covers `~/.cargo`), so
build caches only grow. Driven by mneme's request 15 (its `.chain/`
reached 6.2 GB, 1.7 GB of it stale duplicate hashes).

- **`chain clean`** (inside an app) runs `cargo clean` on
  `.chain/native/Cargo.toml` twice: once without `CARGO_TARGET_DIR`
  (removes the app's own `.chain/native/target` — release builds from
  `chain build`), and once as `cargo clean -p <package>` against the
  shared dev cache (see "Shared build cache"), which drops only this
  app's own crate and keeps the compiled dependencies other Chain apps
  reuse. The package name comes from the manifest's first `name =`.
- **`chain clean --all`** deletes the whole shared dev cache
  (`fs.rmSync`) — every Chain app's next `chain dev` recompiles its
  dependencies once. Runs from anywhere.
- **Size hint:** `chain dev` (for the shared cache, at startup) and
  `chain build` (for the app's own target, after a successful build)
  print one gray line when the cache is over 8 GB, naming the command to
  run. Nothing is ever deleted unless the developer runs `chain clean`
  (request 15 asked for exactly that).
- `cargo` is run from rustup's own location (`$CARGO_HOME/bin` or
  `~/.cargo/bin`) when present, falling back to `PATH` for non-rustup
  installs.

Verified in a throwaway app: `chain clean` took the shared cache from
1.4 GB to 965 MB (app crate gone, dependencies kept) and removed
`.chain/native/target`; `--all` removed the cache; the hint fired for a
sparse 9 GB file and stayed silent for small/missing directories.
Code: `packages/cli/src/clean.ts`.

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
chain migration add <name>  # inside an app: generate a migration from the schema classes
chain database update   # inside an app: apply (or revert to a target) against the real app.db
chain clean             # inside an app: free build-cache disk space (--all: shared cache)
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
  `clean`, `doctor`, `--help`, `--version`, unknown-command and no-args handling).
  Start here for anything about how a flag or subcommand is recognized.
- `packages/cli/src/migration.ts` — `chain migration add/remove/list/
script/check`: rename detection (`--rename-*` flags, then prompts),
  `latestModel()` (the snapshot `add` diffs against), `migrationStatus()`.
- `packages/cli/src/schema/` — the migration engine, one job per file:
  `model.ts` (the schema model and its canonical form), `read-classes.ts`
  (`@Table` classes → model, via the TypeScript compiler), `diff.ts`
  (model diff → Up/Down SQL, including table rebuilds), `introspect.ts`
  (real SQLite → model), `emit-classes.ts` (model → class files, for
  `scaffold`), `history.ts` (`_chain_migrations` + the runner, mirroring
  `storage.rs`), `migrations-dir.ts` (finding `db/`, loading and writing
  migration files and snapshots, `regenerateIndex()`).
- `packages/cli/test/schema.test.mjs` — `npm test` (after `npm run
build`): the engine against real SQLite. Add a case here for any new
  kind of schema change before supporting it.
- `packages/cli/src/nativeProject.ts` — shared by `dev.ts`/`build.ts`/
  `inspect.ts`/`database.ts`: `resolveTauriBin()`, `nativeProjectDir()`
  (`.chain/native`), `checkChainApp()` (the "doesn't look like a Chain
  app" / "still on src-tauri, run `chain update`" errors), `tauriEnv()`
  (sets `TAURI_APP_PATH` — this is _the_ mechanism that makes
  `.chain/native` discoverable to Tauri's CLI, see the section above),
  `inspectorInfoPath()` (where `chain inspect` finds the running bridge's
  port+token), `sharedDevTargetDir()` (`chain dev`'s cross-app Cargo
  build cache), `resolveDbPath()` (where `chain database` finds `app.db` —
  see its section above for the exact per-OS algorithm and why it has to
  match Tauri's own).
- `packages/cli/src/database.ts` — `chain database update [target]`
  (up or down, via `history.ts`'s runner) and `chain database scaffold`
  (replay → introspect → emit classes → verify round trip → save
  snapshot). `chain database list` is `migration.ts`'s `list()`.
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
  through the `eval` wire command; `trace()` and `focusWindow()` back
  `--trace`/`--focus`. `viewportRegion()` is the shared
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
  synthesizing input), `focus`, `trace`, and `traced()` — the command
  handler wrapper behind `--trace`. Its own module doc explains what stays compiled
  unconditionally (a trivial command stub) versus what's feature-gated
  (everything real) and why.
- `packages/cli/src/trace.ts` — `--trace`'s page scripts (frame
  recorder, dump, stop) and `buildReport()`/`formatReport()`, which join
  the page's and the app's records by call id. Tested by
  `packages/cli/test/trace.test.mjs` (the scripts run in a stand-in page).
- `crates/core/src/dev_trace.rs` — the app's side of `--trace`: command
  records, `record_sql`, the process sampler. An empty `record_sql` stub
  is all that exists without the `dev-trace` feature.
- `packages/cli/src/scaffold.ts` — the shared source of truth for both
  `init` and `update`: `TRACKED_FILES` (every framework-owned path and
  how to regenerate it — the native-project entries are `.chain/native/...`,
  not `src-tauri/...`), the idempotent `patch*` functions (`package.json`,
  `vite.config.ts`, `tauri.conf.json`, `Cargo.toml`), and
  `desiredContent()`. `patchPackageJson` is what wires a scaffolded app's
  `dev`/`build` scripts to `chain dev`/`chain build` and adds the
  versioned `@chain/cli` devDependency that makes them resolvable locally
  (see "Publishing" above). `patchCargoToml`'s `chain-core` git dependency
  and `patchTauriConf()`'s `frontendDist` are both specific to
  `.chain/native`, not `src-tauri` — see the depth note above before
  changing either. `patchTauriConf()` also sets `app.security.assetProtocol` (`enable:
true`, `scope: ["$APPDATA/files/*"]`) and `patchCargoToml()` adds the
  `"protocol-asset"` Cargo feature to the `tauri` dependency — both
  required for `desktop.files.url()` to actually work (see the `files`
  capability's `AGENTS.md` for the real bug this fixes: without either
  one, `convertFileSrc()` produces a syntactically valid `asset://` URL
  that the webview refuses outright). `patchCargoToml()` also appends a
  `[profile.dev]` that drops debug info for dependencies and keeps only
  line tables for the app's own code — a fresh debug `target/` measured
  2.5 GB → 1.4 GB with it (same profile lives in the chain-sdk root
  `Cargo.toml` for the playground). It's skipped if the app already has
  its own `[profile.dev]`. It also narrows `create-tauri-app`'s
  `crate-type = ["staticlib", "cdylib", "rlib"]` to `["rlib"]` —
  staticlib/cdylib only serve iOS/Android, and dropping them took the
  same build to 1.1 GB. Re-add them if Chain ever targets mobile.
  **Add a new framework-owned file
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
- `packages/cli/scripts/sync-meta.mjs` / `sync-assets.mjs` — see
  "Publishing" above. Generate `packages/cli/src/publishMeta.ts` and
  `packages/cli/asset/` respectively (both gitignored); wired into
  `prepare`/`prepublishOnly` in `packages/cli/package.json`, never run by
  hand except to debug them.
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
