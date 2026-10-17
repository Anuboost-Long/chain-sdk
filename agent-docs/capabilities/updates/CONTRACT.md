# Updates — Semantic Contract

`desktop.updates` lets a built app replace itself with a newer version
from a public feed: check, download with progress, then install and
relaunch — every step only when the app asks. Each update is proved to
come from the app's owner by a signature of the owner's own, **not**
Apple's code signing, so it works for ad-hoc signed apps and keeps
working unchanged once the app is signed with a Developer ID.
Structural contract: `capabilities/updates/contract.ts`.

Requested by Lazify (request 22,
`docs/chain-sdk-requests/22-update-the-app-in-place.md` in lazify-chain).

## Setting it up

1. **A signing key, once.** `chain updates key <file>` creates a key
   pair: the private key goes to `<file>` (it refuses a path inside the
   app's folder — the private key never belongs in the repo) and the
   public key is printed. Keep the private key, and its password if you
   gave one, somewhere safe: losing it means already-installed copies
   can never update again.
2. **package.json** declares the feed and the public key:

   ```json
   "chain": {
     "updates": {
       "feed": "https://github.com/owner/app/releases/latest/download/latest.json",
       "publicKey": "dW50cnVzdGVkIGNvbW1lbnQ6…"
     }
   }
   ```

   `feed` is read anonymously, so a public GitHub release works and no
   token ships in the app. It must be `https`; plain `http` is accepted
   only for a loopback host (local testing).
3. **`chain build`** with `CHAIN_UPDATE_KEY` set to the private key's
   path (and `CHAIN_UPDATE_KEY_PASSWORD` if it has one) also writes the
   update files to `.chain/native/target/updates/`, beside the app and
   its disk image (ignored by git; `chain clean` removes them with the
   rest of the build):
   - `<Name>_<version>_<arch>.app.tar.gz` — the update itself
     (`aarch64`, `x86_64` or `universal`);
   - `….app.tar.gz.sig` — its signature (kept for reference; the feed
     carries it);
   - `latest.json` — the feed. It describes every architecture built
     **for the same version**: build for Apple silicon, then for Intel
     (`chain build --target x86_64-apple-darwin`), and the one feed
     covers both. A build of a different version starts the feed afresh.
   `--notes <file>` puts that file's text in the feed as the release
   notes; the release date is the build's. A release is: build, then
   upload `latest.json` and the archives side by side — the feed points
   at each archive by its name, next to the feed's own address.
   Without `CHAIN_UPDATE_KEY`, `chain build` stops before building.

## Methods

### `status()`

Never rejects. `supported` is false, with a `reason` and a plain
`message`, when:
- `not-configured` — no `chain.updates` in package.json;
- `dev-build` — a `chain dev` run: there's no built app to replace;
- `read-only` — this user can't replace the app where it is (installed
  into `/Applications` by another account, say, or run from a mounted
  disk image);
- `unsupported-platform` — Windows and Linux today.

Every other method rejects `UNSUPPORTED`, with the same message, in
those cases. `currentVersion` is the running app's; `downloaded` is set
while a downloaded update waits.

### `check()`

Fetches the feed and resolves with `{ version, date, notes }` when it
offers a version newer than the running one for this Mac's
architecture, or `null`. It never downloads the update. Rejects
`UNAVAILABLE` when there's no network, the feed can't be reached, or
the feed isn't a valid feed.

### `download()`

Downloads the update the last `check()` found, reporting progress to
`onProgress` (`downloaded` bytes, `total` or null), and **verifies its
signature against the app's public key before resolving**. The
signature also names the version it was made for; an update whose
signed version differs from the feed's rejects too, so a tampered feed
can't pass an older genuine release off as newer. Rejects:
- `NOT_FOUND` — no `check()` has found an update yet;
- `UNAVAILABLE` — the download failed (network, server);
- `INTEGRITY_FAILED` — the signature doesn't verify: the file didn't
  come from the key's owner, or was changed.

Nothing is installed by `download()`. The verified update waits in
memory until `installAndRelaunch()`, or until the app quits.

### `installAndRelaunch()`

Replaces the app with the downloaded update and starts the new version.
The app's windows close and everything it runs stops, as on quit —
warn the user first. The swap is a single atomic exchange on the same
disk: at every moment either the old or the new app is in place, never
neither and never half of one. Rejects `NOT_FOUND` when nothing is
downloaded, `UNSUPPORTED` as above, and `NATIVE_FAILURE` when the swap
fails — the running app is then still installed and unchanged — or
`UNAVAILABLE` when there isn't enough disk space to unpack it.

### Installing on quit

When the user quits the app (any normal quit) while a verified update
waits, Chain installs it on the way out; the next launch is the new
version. A crash or a forced quit installs nothing.

### `onProgress(listener)`

Progress of the running `download()`.

## What an update does to the app's permissions (macOS)

macOS remembers a permission (microphone, controlling another app with
Apple Events, Files and Folders such as Documents) for an app's **code
identity** — its designated requirement — not its name. Verified live on
macOS 27 with a test app, granting all three and then updating it:

| After an update, the app… | ad-hoc signed (Chain's default) | signed with a stable certificate |
| --- | --- | --- |
| keeps microphone, Apple Events, Files and Folders | **no — macOS asks again** | **yes, no prompts** |
| keeps folders the user picked (`desktop.folders`) | yes | yes |
| opens without an "unidentified developer" prompt | yes | yes |

- **Ad-hoc signing ties the identity to the exact build** (`designated
  => cdhash H"…"`), so every update is a new app to macOS: each
  permission prompt shows once more after each update. Nothing an app
  can do at runtime changes this.
- **A stable certificate keeps them.** Signed with the same code-signing
  identity every time, the identity is `identifier "<bundle id>" and
  certificate leaf = H"<the certificate's hash>"` — the same across
  builds, and no other software can claim it without the certificate's
  private key. A **self-signed** certificate is enough; no Apple
  Developer membership. Set `CHAIN_SIGNING_IDENTITY` to its name (or its
  SHA-1, from `security find-identity -p codesigning`) when running
  `chain build`. Create one once in Keychain Access → Certificate
  Assistant → Create a Certificate (Identity Type "Self Signed Root",
  Certificate Type "Code Signing"); keep it — and back it up — like the
  update key: a new certificate is a new identity, so every permission
  is asked for once more. Moving from ad-hoc to a certificate, or later
  to a Developer ID, likewise asks once per permission after that
  update. Signing with an identity also turns on the hardened runtime;
  the entitlements microphone and Apple Events need come from
  package.json `"chain.permissions"` (verified working).
- **Picked folders keep working either way**: macOS records the user's
  choice on the folder itself (`com.apple.macl`), and Chain keeps the
  grant in the app's data, which an update doesn't touch.
- **No new Gatekeeper prompt.** The first copy, downloaded in a browser,
  is quarantined, so its first launch needs "Open Anyway" in System
  Settings → Privacy & Security, as for any app that isn't notarized (not
  re-tested here — Apple's rule). Chain downloads updates itself, so
  nothing it installs is quarantined, and the updated app opens straight
  away (four updates verified, ad-hoc and self-signed).

## Errors

| Code | When |
| --- | --- |
| `UNSUPPORTED` | updating isn't possible for this run (`status()` says why) |
| `NOT_FOUND` | `download()` before a `check()` found one; `installAndRelaunch()` before a `download()` |
| `UNAVAILABLE` | offline, feed unreachable or invalid, download failed, not enough disk space |
| `INTEGRITY_FAILED` | the update's signature doesn't verify |
| `NATIVE_FAILURE` | anything else |

## Non-goals

- Downloading or installing on its own, on a schedule, or in the
  background without being asked.
- Delta updates, release channels, staged rollouts, rollback.
- Hosting or publishing the feed; signing with Apple's Developer ID or
  notarizing (separate, and not required by this).
- Windows and Linux (not started).
- Asking for an administrator's password to replace an app this user
  can't write to — that's `read-only`, not a prompt.
