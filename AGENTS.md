# Chain — Root Agent Memory

Chain is a reusable cross-platform desktop framework: a Rust orchestration
layer (**Chain Core**) exposed to applications through a stable TypeScript
API (**Chain SDK**), running on a replaceable native runtime (Tauri
initially) with platform-native adapters underneath.

Full background and reasoning: [`docs/MNEME_DESKTOP_FRAMEWORK.md`](docs/MNEME_DESKTOP_FRAMEWORK.md).
Read that before making any architectural decision here — this file only
summarizes the rules that must not be broken.

## Names

- **Chain** — the framework (this repo).
- **Chain SDK** — the public JS/TS API (`packages/sdk`).
- **Chain Core** — the Rust orchestration/bridge/permissions layer (`crates/core`).
- **Chain Runtime** — the replaceable native runtime beneath Chain Core (Tauri initially).
- **Capability** — a reusable module (Files, Audio, Clipboard, Platform, ...) under `capabilities/`.
- **Mneme** — application #1, lives in a separate repo, consumes Chain SDK. It is not part of this repo.

## Architecture (never bypass this)

```
Application
   ↓
Chain SDK          (packages/sdk)   — public TS API
   ↓
Chain Core (Rust)  (crates/core)    — coordination, IPC, permissions, error normalization
   ↓
Chain Runtime                       — Tauri today; must stay swappable
   ↓
Native Adapter / OS
```

An application must never import Tauri, a native adapter, or an OS API
directly. Everything goes through Chain SDK.

## Rules that must never be broken

1. **Contract first.** A capability gets a `CONTRACT.md` (meaning) and
   `contract.ts` (types) before any implementation code.
2. **No platform-specific naming leaks.** Native method names (Swift,
   C#, WinRT, AVFoundation, ...) never dictate the public SDK shape. Ask
   "what does this mean to an app developer?", not "what is it called
   natively?".
3. **No single-platform contracts.** Never mark a contract stable based on
   only one operating system's behavior.
4. **Capability detection, not platform detection.** Application-facing
   code checks `desktop.capabilities.x`, not `os === "windows"`.
5. **Normalized errors.** Native errors are converted to a shared
   `ChainErrorCode` at the Rust boundary; native detail stays available
   only for debugging.
6. **Events over polling.** Expose `on*` subscriptions when the OS can
   notify us; avoid `setInterval` polling for state that has a native
   change event.
7. **Minimum framework first.** Do not build capabilities speculatively.
   Every capability must be driven by a real requirement from a real
   application (Mneme first). See `docs/FRAMEWORK_CANDIDATES.md`. **When
   you're handed a capability request file** (e.g. a path under a
   consuming app's own `docs/chain-sdk-requests/`), follow
   `docs/CAPABILITY_WORKFLOW.md` step by step — research → contract →
   implement → wire → propagate via `chain update` → verify for real →
   document. Don't improvise this from scratch each time.
8. **Research becomes permanent knowledge.** Findings about platform
   behavior are written into `agent-docs/capabilities/<name>/research/`,
   not left in conversation history.
9. **Small, bounded agent tasks.** An agent implementing a Windows adapter
   should only need: this file, `docs/ARCHITECTURE.md`, the capability's
   `AGENTS.md`/`CONTRACT.md`/research (all in
   `agent-docs/capabilities/<name>/`), and the relevant source
   (`capabilities/<name>/contract.ts`, `crates/core/src/<name>.rs`) — not
   the whole repository.

## Tooling

`packages/cli` provides the `chain` CLI, the same role `npm create
tauri-app` or `expo init` play for their frameworks:

```bash
chain init <project-name>   # scaffold ./<project-name> in the current directory
chain dev                   # run from inside an app — condensed, branded `tauri dev`
chain build                 # run from inside an app — condensed, branded `tauri build`
chain inspect               # run from inside an app while `chain dev` is running — REPL
                             # to eval/click/read the live window (dev-only automation bridge,
                             # never compiled into `chain build` output)
chain update                # merge chain-sdk template changes into an existing app, run from inside it
chain migration <name>      # scaffold the next db/migrations/000N-<name>.ts (SQLite apps only),
                             # run from inside an app — see agent-docs/framework/command/README.md
chain database update       # apply every pending migration straight to the app's real
                             # SQLite file, without launching the app — run from inside an app
chain database list         # list every migration with its applied/pending status
chain doctor                # check/install the Rust toolchain a Chain app needs to build
chain --help, -h            # list commands
chain --version, -v         # print the CLI version
```

`init` scaffolds a full Tauri + React + TypeScript app (via
`create-tauri-app`) in a new folder named `<project-name>`, relative to
wherever you ran the command — it does not take an arbitrary path, and it
only creates new projects (errors if the target already exists; `update`
is how an existing one changes). It comes with Tailwind CSS, basic
navigation (`react-router-dom`), `@chain/sdk` wired up, and Chain's
placeholder branding/icons already applied — see
`agent-docs/framework/command/README.md` for exactly what gets generated.
The scaffolded app's `dev`/`build` scripts run `chain dev`/`chain build`,
which point Tauri at the native project hidden away in `.chain/native/`
(not the conventional `src-tauri/` — see that same doc for why and how).
`update` does a real three-way merge (via `git merge-file`, baseline
snapshot in `.chain/baseline/`) so developer edits survive — files the
developer never touched get the new template silently; files that
changed on both sides merge, or get real conflict markers if they
overlap; nothing is ever reset. `doctor` never installs anything without
an explicit `y/N` confirmation (and never prompts at all outside a TTY)
— Rust is a build-time-only dependency for whoever builds a Chain app,
never for the end user (Tauri ships a compiled binary). More subcommands
are expected later, each only once a real requirement drives it — see
rule 7.

On a fresh clone of this repo, make the `chain` command available once:

```bash
cd packages/cli && npm link
```

It always links the scaffolded app's `@chain/sdk` dependency back to
_this_ clone's `packages/sdk` via a relative `file:` path, so `npm link`
needs to be redone after moving or re-cloning this repo.

## Feature docs

[`agent-docs/`](agent-docs/README.md) has one folder per shipped feature,
each with a short README covering how it works, how to use it, and which
files to check for debugging or maintenance. Check there first when
working on or debugging an existing feature. It's split in two:

- `agent-docs/framework/<name>/` — the fixed pieces of Chain itself
  (`command` = the CLI, `sdk`, `core`, `brand`). One of each.
- `agent-docs/capabilities/<name>/` — mirrors `capabilities/<name>/` by
  name, and also holds that capability's `AGENTS.md` (build-time memory)
  and `CONTRACT.md` (semantic contract) — see "Context-loading rules"
  below for how those two differ from the `README.md`. This is why
  `capabilities/<name>/` itself holds no prose, only code and structured
  metadata (`contract.ts`, `component.json`, `tests/`).

**Every feature you add or materially change must get (or update) its
`README.md` in the same change, in whichever of the two folders above it
belongs** — this is a standing instruction, it repeats for every feature
added to this SDK, not a one-time setup task.

## Testing expectations

Every capability's contract ships with contract tests that any native
implementation must pass (`capabilities/<name>/tests/`, run against every
platform adapter). A capability is not "done" on one platform until its
contract tests pass there.

## Context-loading rules for agents

When asked to work on a capability, read in this order and stop once you
have enough:

1. This file (`AGENTS.md`)
2. `docs/ARCHITECTURE.md`
3. `agent-docs/capabilities/<name>/AGENTS.md`
4. `agent-docs/capabilities/<name>/CONTRACT.md` + `capabilities/<name>/contract.ts`
   (the semantic and structural contract live in different trees now —
   see "Feature docs" above for why)
5. `agent-docs/capabilities/<name>/research/<PLATFORM>.md` (if it exists)
6. The specific source/test files for the task

That's for a capability that already exists. Starting a **new** one from
a request file — `capabilities/<name>/` doesn't exist yet — read
`docs/CAPABILITY_WORKFLOW.md` instead of guessing the order yourself.

Do not read the whole repository "to be safe" — that is exactly the token
waste this project is designed to avoid.

## Current status

This repo currently holds only the minimal skeleton for the first
vertical slice (`desktop.platform.getInfo()`). See
`docs/CAPABILITY_MATRIX.md` for what exists vs. what's still TODO, and
`agent-docs/capabilities/platform/AGENTS.md` for the next concrete steps.
