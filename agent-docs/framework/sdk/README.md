# Chain SDK (`@chain/sdk`)

## How it works

The SDK is the only thing an application is allowed to import — never
Tauri, Rust, or native APIs directly (root `AGENTS.md` rule 1).
`packages/sdk/src/index.ts` exports a single `desktop` object that
aggregates every capability (currently just `platform`). Each capability
module (e.g. `src/platform.ts`) implements the structural contract
defined in `capabilities/<name>/contract.ts` — importing those types
across the package boundary keeps the contract as the single source of
truth instead of duplicating type definitions inside the SDK.

Right now every capability implementation is a stub that throws a
normalized `ChainError` (`UNSUPPORTED`) — there is no Rust/Tauri bridge
wired up yet. See the platform-capability doc for what's next.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const info = await desktop.platform.getInfo();
```

A consuming app depends on it as an ordinary versioned npm dependency
(published to the public registry — this is what `chain init` sets up
automatically; see the `command` doc's "Publishing" section for how the
version is pinned and why a `file:` link back to this repo doesn't work
once `@chain/cli` itself is installed from npm):

```json
"dependencies": { "@chain/sdk": "^0.1.0" }
```

## Files to check

- `packages/sdk/src/index.ts` — the `desktop` object; add a new
  capability here once it has a contract under `capabilities/`.
- `packages/sdk/src/native.ts` — the only file that imports
  `invoke` from `@tauri-apps/api/core`; every capability imports it from
  here. It records calls for `chain inspect --trace` while one runs (see
  the inspector section of `agent-docs/framework/command/README.md`).
- `packages/sdk/src/errors.ts` — the shared `ChainErrorCode` union and
  `ChainError` shape every capability must throw.
- `packages/sdk/src/platform.ts` — the platform capability's SDK-side
  implementation (currently a stub — see `platform-capability` doc).
- `packages/sdk/scripts/sync-contracts.mjs` — copies each
  `capabilities/<name>/contract.ts` into `packages/sdk/src/contracts/`
  (gitignored, generated) so a published `@chain/sdk` tarball is
  self-contained; every source file imports from `./contracts/<name>`
  rather than reaching across the package boundary directly. Runs via
  `prepare`/`prepublishOnly` — never edit `src/contracts/*.ts` by hand.
  `capabilities/<name>/contract.ts` stays the canonical source (root
  `AGENTS.md` rule 1); this only ever copies it.
