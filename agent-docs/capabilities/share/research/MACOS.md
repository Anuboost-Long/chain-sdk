# Share — macOS research

macOS 27.0.1, 8 October 2026, `apps/playground` dev build.

- `NSSharingServicePicker(items:)` with file `URL`s (and the text as a
  `String` item), `show(relativeTo:of:preferredEdge:)` on the calling
  page's WKWebView. The anchor converts from CSS px with
  `pageZoom × magnification`; WKWebView is flipped, so `.maxY` puts the
  menu below the anchor.
- `NSSharingServicePickerDelegate`:
  - `sharingServicePicker(_:didChoose:)` — the service, or nil when the
    menu is dismissed. `service.title` is the name the menu showed.
    There's no public stable identifier, so the contract returns the
    display name.
  - `sharingServicePicker(_:delegateFor:)` — set `service.subject` (Mail's
    subject) and return ourselves as the service's delegate.
- `NSSharingServiceDelegate`: `didShareItems` / `didFailToShareItems`
  say the service finished, **not** that it's done with the files: Copy
  calls `didShareItems` right after writing the file URLs to the
  pasteboard, and pasting reads them later. Deleting the staged folder
  there made paste fail (reported from mneme, 8 October 2026).
  `sourceWindowForShareItems`
  returns the app window so services attach their UI to it.
- Files: the managed files have random names (`24eab7d4f0929834.pdf`),
  which is what the recipient would see. share.rs copies each one (APFS
  clone via `std::fs::copy`) into `Caches/<app>/chain-share/<id>/` under
  the app's name for it, which also frees the app to delete its own file
  immediately. The folder goes on cancel; after a pick it's swept 24 h
  later on the next `show()`.

## Verified

- `availability()` all true.
- Menu shown anchored under the given element, header "Cell biology
  summary · PDF Document · 11 KB", services AirDrop, Mail, Messages,
  Notes, Freeform, Copy (screenshot, deleted after viewing).
- The app's file deleted right after `show()`: the staged copy stayed.
- Second `show()` while open → `UNAVAILABLE`; `[]` → `INVALID_ARGUMENT`;
  unknown reference → `NOT_FOUND`.
- Dismissing the menu (Mission Control) → `{ status: "cancelled" }`,
  staged folder deleted.

## Not verified yet

- Picking a service (needs a person): `{ status: "picked", service:
  "AirDrop" }`, Mail's subject from `title`, the body from `text`.
- Copy, then paste into Finder and Messages (the fix for the report
  above).
