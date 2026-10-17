# Packages Capability (`desktop.packages`)

## How it works

Downloads an archive the app runs from (a VS Code extension, for its
language server), streamed to a temp file while hashing, checks its
sha256, unpacks it with only plain files and folders allowed, and swaps
it into `<app data>/packages/<id>/` atomically. Nothing is made
executable. It's `desktop.models`' store code in a folder of its own,
laid out exactly like the archive, and it hands the app real paths so
`processRunner` can start what's inside with the user's own `java` or
`node`. Progress comes as `chain://packages-progress` events keyed by id;
a download with no data for 30 s fails. See `CONTRACT.md`.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const installed = await desktop.packages.install(
  { id: "tailwind", url: vsixUrl, sha256, archive: "zip" }, // .vsix isn't inferred as zip
  (received, total) => setProgress(total ? received / total : null)
);
// installed.path: absolute folder, laid out as the archive is

const server = await desktop.packages.resolve("tailwind", "extension/dist/tailwindServer.js"); // NOT_FOUND if missing
const ls = await desktop.processRunner.run("node", [server, "--stdio"], onBytes, { output: "bytes", keepStdinOpen: true });

await desktop.packages.cancel("sonarlint"); // its install rejects CANCELLED, nothing kept
await desktop.packages.list();              // [{ id, path, sizeBytes, installedAt }]
await desktop.packages.remove("tailwind");  // idempotent; stop processes running from it first
```

Every scaffolded app gets the commands through `chain update`
(`packages/cli/templates/lib.rs`).

## Files to check

- `agent-docs/capabilities/packages/CONTRACT.md` — semantics, errors,
  non-goals (nothing executable, nothing run, no registry).
- `capabilities/packages/contract.ts` — the types.
- `crates/core/src/packages.rs` — the thin wrapper adding paths.
- `crates/core/src/models.rs` — the shared store: download (stall
  timeout), safe unpack, atomic replace, `open_store`'s lift flag. Its
  tests cover hostile archives and the unlifted zip layout.
- `packages/sdk/src/packages.ts` — SDK wrapper and error mapping.
- `packages/cli/templates/lib.rs` (`PackagesState`, `packages_*`
  commands) and `apps/playground/src-tauri/src/lib.rs`, kept identical.
