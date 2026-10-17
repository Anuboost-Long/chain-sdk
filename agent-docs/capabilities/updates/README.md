# Updates Capability (`desktop.updates`)

## How it works

A built app replacing itself from a public feed. Requested by Lazify
(request 22, `docs/chain-sdk-requests/22-update-the-app-in-place.md` in
lazify-chain). macOS only today.

- **Trust is Chain's own, not Apple's.** Each update archive is signed
  with the owner's Ed25519 (minisign) key; the public half sits in
  package.json `"chain.updates".publicKey`, compiled in. Code signing
  doesn't enter into it, so ad-hoc, self-signed and Developer ID builds
  all update the same way.
- **tauri-plugin-updater (~2.12)** does the feed, the download and the
  signature check. `updates::configure` hands it package.json's settings
  as `plugins.updater` (with `requireSignedVersion`, so the signature's
  own version must match the feed's — no downgrade by a tampered feed,
  and `dangerousInsecureTransportProtocol` only for a loopback http
  feed). Pinned to ~2.12: 2.13+ needs tauri 2.12, newer than apps lock.
- **Chain does the install, not the plugin.** The plugin's macOS
  install moves the running app into a temp dir that's deleted at the
  end — a failed second move loses the app — and asks for an admin
  password through AppleScript when it can't write. Instead,
  `chain_core::updates::install` unpacks next to the bundle and swaps the
  two with `renamex_np(RENAME_SWAP)`, one atomic exchange, then deletes
  the old one; `replaceable_bundle` reports `read-only` up front.
- **Install on quit**: the verified archive waits in `UpdatesState`
  (memory); `RunEvent::Exit` installs it (`updates::on_exit`).
- **`chain build`** (with `"chain.updates"`) needs `CHAIN_UPDATE_KEY`
  (mapped to Tauri's signing variables), turns on
  `bundle.createUpdaterArtifacts`, then `writeUpdateFiles` copies the
  archive + signature to `.chain/native/target/updates/` as
  `<Name>_<version>_<arch>.app.tar.gz` and merges `latest.json` (same
  version: platforms add up; new version: fresh). `--notes <file>` fills
  `notes`. `CHAIN_SIGNING_IDENTITY` signs with a stable certificate —
  what keeps macOS permissions across updates (CONTRACT.md).
- **`chain updates key <file>`** runs `tauri signer generate`, refusing a
  path inside the app folder or an existing file.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const status = await desktop.updates.status();
if (!status.supported) return show(status.message);

const update = await desktop.updates.check(); // { version, date, notes } | null
if (update && (await ask(`Download ${update.version}?`))) {
  const off = desktop.updates.onProgress(({ downloaded, total }) => bar(downloaded, total));
  await desktop.updates.download(); // verified before it resolves
  off();
  if (await ask("Restart now? Running agents stop.")) await desktop.updates.installAndRelaunch();
  // otherwise it installs when the user quits
}
```

```bash
chain updates key ~/keys/myapp-updates.key          # once
CHAIN_UPDATE_KEY=~/keys/myapp-updates.key CHAIN_SIGNING_IDENTITY="My App Signing" \
  chain build --notes CHANGELOG-1.2.0.md            # Apple silicon
CHAIN_UPDATE_KEY=… chain build --target x86_64-apple-darwin   # adds Intel to the same feed
# upload .chain/native/target/updates/{latest.json,*.app.tar.gz} next to the feed URL
```

## Files to check

- `capabilities/updates/contract.ts`, `agent-docs/capabilities/updates/CONTRACT.md`
- `crates/core/src/updates.rs` — config, `replaceable_bundle`, atomic `install` (unit-tested on a real disk)
- `packages/cli/templates/updates.rs` (+ playground copy) — plugin setup, commands, install on quit
- `packages/cli/templates/lib.rs` — `updates::configure`, the plugin, commands, `on_exit`
- `packages/cli/src/updates.ts` — feed writing, signing env, `chain updates key`; `build.ts` calls it
- `packages/cli/src/scaffold.ts` — adds `tauri-plugin-updater` to apps' Cargo.toml, tracks updates.rs
- `packages/sdk/src/updates.ts` — SDK wrapper
