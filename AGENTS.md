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
   application (Mneme first). See `docs/FRAMEWORK_CANDIDATES.md`.
8. **Research becomes permanent knowledge.** Findings about platform
   behavior are written into `capabilities/<name>/research/`, not left in
   conversation history.
9. **Small, bounded agent tasks.** An agent implementing a Windows adapter
   should only need: this file, `docs/ARCHITECTURE.md`, the capability's
   `AGENTS.md`/`CONTRACT.md`/research, and the relevant source — not the
   whole repository.

## Tooling

`packages/cli` provides the `chain` CLI, the same role `npm create
tauri-app` or `expo init` play for their frameworks:

```bash
chain init <project-name>   # scaffold ./<project-name> in the current directory
chain update                # merge chain-sdk template changes into an existing app, run from inside it
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
`agent-docs/command/README.md` for exactly what gets generated. `update`
does a real three-way merge (via `git merge-file`, baseline snapshot in
`.chain/baseline/`) so developer edits survive — files the developer
never touched get the new template silently; files that changed on both
sides merge, or get real conflict markers if they overlap; nothing is
ever reset. `doctor` never installs anything without an explicit `y/N`
confirmation (and never prompts at all outside a TTY) — Rust is a
build-time-only dependency for whoever builds a Chain app, never for the
end user (Tauri ships a compiled binary). More subcommands (`add`, ...)
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

[`agent-docs/`](agent-docs/README.md) has one folder per shipped feature
(CLI, SDK, each capability, ...), each with a short README covering how
it works, how to use it, and which files to check for debugging or
maintenance. Check there first when working on or debugging an existing
feature. **Every feature you add or materially change must get (or
update) its `agent-docs/<feature-name>/README.md` in the same change** —
this is a standing instruction, it repeats for every feature added to
this SDK, not a one-time setup task.

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
3. `capabilities/<name>/AGENTS.md`
4. `capabilities/<name>/CONTRACT.md` + `contract.ts`
5. `capabilities/<name>/research/<PLATFORM>.md` (if it exists)
6. The specific source/test files for the task

Do not read the whole repository "to be safe" — that is exactly the token
waste this project is designed to avoid.

## Current status

This repo currently holds only the minimal skeleton for the first
vertical slice (`desktop.platform.getInfo()`). See
`docs/CAPABILITY_MATRIX.md` for what exists vs. what's still TODO, and
`capabilities/platform/AGENTS.md` for the next concrete steps.
