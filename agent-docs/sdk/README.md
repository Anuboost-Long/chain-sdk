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

A consuming app depends on it via a relative `file:` path (this is what
`chain init` sets up automatically — see the `command` doc):

```json
"dependencies": { "@chain/sdk": "file:../chain-sdk/packages/sdk" }
```

## Files to check

- `packages/sdk/src/index.ts` — the `desktop` object; add a new
  capability here once it has a contract under `capabilities/`.
- `packages/sdk/src/errors.ts` — the shared `ChainErrorCode` union and
  `ChainError` shape every capability must throw.
- `packages/sdk/src/platform.ts` — the platform capability's SDK-side
  implementation (currently a stub — see `platform-capability` doc).
- `packages/sdk/tsconfig.json` — `rootDir` is deliberately set to the
  repo root (not `src`) so it can type-check the cross-package import of
  `capabilities/*/contract.ts`. Don't "fix" that back to `src` without
  re-reading why — `tsc --noEmit` will fail with a `TS6059` rootDir error
  if you do.
