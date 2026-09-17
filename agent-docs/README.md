# Agent Docs

One folder per shipped feature of this repo (`chain-sdk`), written for
whoever — human or AI agent — needs to use, extend, or debug it without
re-deriving how it works from scratch. Split into two groups:

- **`framework/`** — the fixed pieces of Chain itself: the CLI, the SDK,
  Chain Core, visual identity. There's one of each.
- **`capabilities/`** — the pluggable modules (`platform`, `storage`, ...),
  mirroring `capabilities/<name>/` by name. Each capability folder here
  also holds its `AGENTS.md` (build-time memory) and `CONTRACT.md`
  (semantic contract) — a different *kind* of doc from the `README.md`
  below (see root `AGENTS.md`), kept alongside it so `capabilities/<name>/`
  itself stays code-only (`contract.ts`, `component.json`, `tests/`).

## Convention

Every feature gets a `README.md` (under `framework/<name>/` or
`capabilities/<name>/`, whichever it is) with exactly these three
sections:

1. **How it works** — the mechanism, in plain terms. Not a copy of the
   code; the _why_ and the _shape_.
2. **How to use it** — the actual commands / imports / API calls a
   developer runs.
3. **Files to check** — where to look first when something breaks or
   needs extending, one line each on what that file is responsible for.

**When you add or materially change a feature, add or update its
`README.md` in the same change, and add it to the index below.** This is
a standing instruction, not a one-time task — it repeats for every
feature added to this SDK.

## Index

| Feature              | Folder                                                       | What it is                                               |
| --------------------- | -------------------------------------------------------------- | ----------------------------------------------------------- |
| Chain CLI            | [`framework/command/`](framework/command/README.md)         | `chain init`/`dev`/`build`/`update`/`doctor`, `--help`, `--version` |
| Chain SDK            | [`framework/sdk/`](framework/sdk/README.md)                 | `@chain/sdk` — the public TS API apps import             |
| Visual identity      | [`framework/brand/`](framework/brand/README.md)             | Chain logo assets and usage                               |
| Chain Core           | [`framework/core/`](framework/core/README.md)               | `crates/core` — the Rust coordination crate               |
| Platform capability  | [`capabilities/platform/`](capabilities/platform/README.md) | `desktop.platform.getInfo()` end-to-end slice              |
| Storage capability   | [`capabilities/storage/`](capabilities/storage/README.md)   | `desktop.storage` — SQLite-backed migrate/query/execute    |
