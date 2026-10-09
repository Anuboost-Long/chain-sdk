# Share — Windows research (not implemented)

Nothing here has been run.

- The share UI: `DataTransferManager` through
  `IDataTransferManagerInterop::GetForWindow(hwnd)` +
  `ShowShareUIForWindow(hwnd)` (Win32 apps can't use
  `DataTransferManager.GetForCurrentView`). Handle `DataRequested`: set
  `Data.Properties.Title` (required, from `title` or the first file
  name), `SetText(text)`, `SetStorageItems` with `StorageFile`s from the
  staged copies (same staging as macOS: share.rs is portable).
- Anchor: Windows 11's share sheet takes `ShareUIOptions.SelectionRect`
  via `ShowShareUIForWindow` overloads on newer SDKs
  (`IDataTransferManagerInterop` + `DataTransferManager.ShowShareUI(options)`);
  check which is reachable from a Win32 window, else `anchor: false`.
- Result: `TargetApplicationChosen` gives the picked app's name; there's
  no cancel event — the sheet closing without a choice has to be inferred
  (e.g. window activation). `serviceName` likely true, cancel detection
  needs research.
- Delete timing: the target reads the files asynchronously; keep the
  staged folder until the 24 h sweep unless a completion signal exists
  (`DataPackage.ShareCompleted`).
