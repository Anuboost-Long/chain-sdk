# Framework Candidates

Tracks which capabilities are generalizable framework modules vs.
application-specific logic that only looks reusable. A capability only
moves from "used by one app" to "framework module" after a second real
application needs the same thing and the contract survives a second
platform implementation. See rule 7 in the root `AGENTS.md`.

## Platform / System

Used by:

- Mneme (real consumer — wired and running)

Generalizable:
Yes — every app needs basic OS/arch/runtime info.

Contract:
Draft (`agent-docs/capabilities/platform/CONTRACT.md`, `capabilities/platform/contract.ts`)

macOS:
Implemented, verified (`apps/playground` and `mneme`)

Windows:
Not started

Possible package:
`@chain/sdk` (bundled in core, not a separate package — too small to split out)

## Storage

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/01-local-storage.md` in the mneme repo)

Generalizable:
Yes — SQLite-backed local storage is a near-universal desktop-app need,
not mneme-specific (the actual schema on top of it is mneme-specific).

Contract:
Draft (`agent-docs/capabilities/storage/CONTRACT.md`, `capabilities/storage/contract.ts`)

macOS:
Implemented, verified end to end in `mneme`'s running window (including
persistence across restarts)

Windows:
Not started — see `agent-docs/capabilities/storage/research/WINDOWS.md` for
specific risks to check before implementing/verifying (rusqlite's
`bundled` feature is itself cross-platform, so the Rust code needs no
changes; only verification is pending)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)
