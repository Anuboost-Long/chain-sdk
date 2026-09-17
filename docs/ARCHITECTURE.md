# Chain Architecture

See [`MNEME_DESKTOP_FRAMEWORK.md`](MNEME_DESKTOP_FRAMEWORK.md) for the full
reasoning behind these decisions. This file is the terse, current-state
reference.

## Layers

```
Application (e.g. Mneme)
        │
        ▼
Chain SDK            packages/sdk    — public TypeScript API, the ONLY thing apps import
        │
        ▼
Chain Core (Rust)     crates/core    — coordination, IPC, permission enforcement,
        │                              module lifecycle, error normalization
        ▼
Chain Runtime                        — Tauri initially; must remain swappable
        │
   ┌────┴────┐
   ▼         ▼
 macOS     Windows
 Swift     C# / .NET
```

Linux is a future target, added once macOS + Windows contracts are proven.

## Repository layout

```
chain-sdk/
├── AGENTS.md                    root agent memory — rules that must not be broken
├── docs/
│   ├── ARCHITECTURE.md          this file
│   ├── CAPABILITY_MATRIX.md     what's implemented, per platform
│   ├── FRAMEWORK_CANDIDATES.md  what's generalizable vs. app-specific
│   └── MNEME_DESKTOP_FRAMEWORK.md  original working notes / rationale
├── capabilities/                code + structured metadata only, no prose
│   └── <name>/
│       ├── contract.ts          structural contract (exact API, types)
│       ├── component.json       capability metadata/status
│       └── tests/                shared contract tests
├── agent-docs/
│   └── capabilities/            mirrors capabilities/<name>/ by name — all its prose
│       └── <name>/
│           ├── AGENTS.md        capability-scoped agent memory
│           ├── CONTRACT.md      semantic contract (what it means)
│           ├── README.md        how it works / how to use it / files to check
│           └── research/        per-platform native API research
├── packages/
│   ├── sdk/                     Chain SDK (TypeScript, published as @chain/sdk)
│   └── cli/                     `chain` CLI — `chain init <path>` scaffolds a Chain-consuming app
├── crates/
│   └── core/                    Chain Core (Rust)
└── apps/
    └── playground/              minimal app used to exercise the SDK end-to-end
```

## Responsibility boundaries

- **TypeScript/React** (application + `packages/sdk`): UI, state, orchestration,
  calling Chain SDK. Never talks to Tauri or the OS directly.
- **Rust** (`crates/core`): cross-platform coordination, IPC, serialization,
  permission enforcement, error normalization, routing to the right native
  adapter.
- **Native adapters** (Swift on macOS, C#/.NET on Windows): OS integration,
  performance-critical or platform-best-API work. Communicate with Rust
  through a stable boundary — plain data only (strings, numbers, booleans,
  byte buffers, serialized structs, opaque handles). Never leak native
  objects (Swift objects, managed .NET objects) across the boundary.

## Contract = three layers

1. `CONTRACT.md` — semantic contract, human/AI readable meaning.
2. `contract.ts` — structural contract, compiler-enforced shape.
3. `tests/` — behavioral contract, verifies every native implementation
   actually obeys 1 and 2.

## Current milestone

The only capability being built right now is `platform` —
`desktop.platform.getInfo()` — to prove the full vertical slice works
end to end. No other capability should be started until that slice is
verified. See `agent-docs/capabilities/platform/AGENTS.md` for what's left to do.
