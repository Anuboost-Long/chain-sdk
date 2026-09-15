# Agent Docs

One folder per shipped feature of this repo (`chain-sdk`), written for
whoever — human or AI agent — needs to use, extend, or debug it without
re-deriving how it works from scratch. This is a maintenance/reference
index across the whole repo; it's separate from
`capabilities/<name>/AGENTS.md`, which is the narrower build-time memory
used _while implementing_ a capability (see root `AGENTS.md`).

## Convention

Every feature gets `agent-docs/<feature-name>/README.md` with exactly
these three sections:

1. **How it works** — the mechanism, in plain terms. Not a copy of the
   code; the _why_ and the _shape_.
2. **How to use it** — the actual commands / imports / API calls a
   developer runs.
3. **Files to check** — where to look first when something breaks or
   needs extending, one line each on what that file is responsible for.

**When you add or materially change a feature, add or update its
`agent-docs/<feature-name>/README.md` in the same change, and add it to
the index below.** This is a standing instruction, not a one-time task —
it repeats for every feature added to this SDK.

## Index

| Feature             | Folder                                                  | What it is                                              |
| ------------------- | ------------------------------------------------------- | ------------------------------------------------------- |
| Chain CLI           | [`command/`](command/README.md)                         | `chain init`/`update`/`doctor`, `--help`, `--version`   |
| Chain SDK           | [`sdk/`](sdk/README.md)                                 | `@chain/sdk` — the public TS API apps import            |
| Visual identity     | [`brand/`](brand/README.md)                             | Chain logo assets and usage                             |
| Chain Core          | [`core/`](core/README.md)                               | `crates/core` — the Rust coordination crate             |
| Platform capability | [`platform-capability/`](platform-capability/README.md) | `desktop.platform.getInfo()` end-to-end slice           |
| Storage capability  | [`storage-capability/`](storage-capability/README.md)   | `desktop.storage` — SQLite-backed migrate/query/execute |
