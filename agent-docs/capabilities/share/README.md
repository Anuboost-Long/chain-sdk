# Share Capability (`desktop.share`)

## How it works

`show(files, options)` → `share_show` (template `lib.rs`):

1. `chain_core::share` parses the files and options, resolves each
   reference, and copies the files into a new folder under
   `Caches/<app>/chain-share/` with the recipient's names (`stage`),
   after sweeping folders older than a day.
2. In the calling page's `with_webview`, `chain_core::share::show` hands
   the WKWebView and the copies to `crates/core/swift/ChainShare.swift`,
   which opens `NSSharingServicePicker` at the anchor.
3. When the menu closes Swift reports picked (with the service's name)
   or cancelled; the command resolves with that. The folder is deleted on
   cancel; after a pick it stays a day (Copy's pasteboard holds only its
   URL) and the next `show()` sweeps it.

## How to use it

```ts
const { available } = await desktop.share.availability();
if (!available) return saveInstead();

const { x, y, width, height } = button.getBoundingClientRect();
const result = await desktop.share.show(
  [{ reference, name: "Cell biology summary" }],
  { anchor: { x, y, width, height }, title: "Cell biology summary" }
);
await desktop.files.delete(reference); // safe straight away
if (result.status === "picked") console.log(result.service);
```

## Files to check

- `capabilities/share/contract.ts`, `agent-docs/capabilities/share/CONTRACT.md`
- `crates/core/src/share.rs` — names, staging, sweep, FFI
- `crates/core/swift/ChainShare.swift` — the picker and its delegates
- `packages/cli/templates/lib.rs` — `share_availability`, `share_show`
- `packages/sdk/src/share.ts` — API and error codes
- `research/MACOS.md`, `research/WINDOWS.md`
