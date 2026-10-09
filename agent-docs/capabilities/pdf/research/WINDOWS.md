# PDF — Windows research (not implemented)

Nothing here has been run. Starting points for whoever implements it on
the same contract.

- Engine: WebView2's `ICoreWebView2_7::PrintToPdf(path, settings)`
  (Chromium's print path). Settings: `Orientation`, `PageWidth`/
  `PageHeight` (inches), `Margin*` (inches), `ShouldPrintBackgrounds`,
  `ScaleFactor`, `ShouldPrintHeaderAndFooter` (Chromium's own
  title/URL/date/page header — not ours; leave it off).
- Render webview: a hidden Tauri window again, so `asset://`/the app
  origin load the same way; `disable_javascript()` for `runScripts`.
  Readiness: the same script through `ExecuteScript` (no isolated world
  there — the page's scripts are off anyway unless `runScripts`).
- Header/footer: Chromium 131+ supports `@page` margin boxes with
  `counter(page)`/`counter(pages)`, so `header`/`footer` can be injected
  as CSS (`@page { @top-left { content: "..." } }`) instead of redrawn.
  Then `cssPageMarginBoxes` can be true. Check the WebView2 runtime's
  Chromium version.
- `@page { size }` is honoured by Chromium when the page sets it
  (`PreferCSSPageSize` in CDP; check what `PrintToPdf` does).
- Scale: Chromium prints 1 CSS px = 0.75 pt; no zoom needed.
- Metadata: `PrintToPdf` doesn't take a title/author. Options: CDP
  `Page.printToPDF` via `CallDevToolsProtocolMethod` (also returns
  bytes), or rewriting the Info dictionary afterwards.
- Complex scripts: Skia's PDF backend writes `ToUnicode` per cluster and
  `/ActualText`, so Khmer should copy and search correctly — verify,
  then report `complexScriptSearch: true`.
- Default paper: `GetLocaleInfoEx(LOCALE_NAME_USER_DEFAULT,
  LOCALE_IPAPERSIZE)` (1 = Letter, 9 = A4).

Checklist before marking Windows anything but not-started: every row of
`research/MACOS.md`'s "Verified" list, plus Khmer copy/search.
