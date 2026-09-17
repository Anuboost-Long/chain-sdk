# Chain visual identity

## How it works

The mark uses two interlocking sections that form a single C for Chain.
The fitted joint expresses a stable interface connecting replaceable native
implementations. The C remains recognizable at desktop icon sizes, and the
joint remains visible in the monochrome version.

This direction follows the framework's [purpose](../../docs/MNEME_DESKTOP_FRAMEWORK.md)
and [architecture](../../docs/ARCHITECTURE.md): applications use one public
TypeScript API, with Rust coordination and a replaceable runtime underneath.
The symbol is a conceptual connection, not a literal count of architecture layers.

The app icon uses electric lime (`#C5F74F`) and a pale neutral (`#EEF2E4`) on charcoal
(`#171B24`). `asset/app-icon.svg` is the source of truth for the geometry
and palette. The fixed-color transparent mark and SDK lockup use those same
lime and pale colors, without substituting a different lower-section color.
Electric lime gives the identity its bold, youthful accent. Charcoal is the
preferred presentation background; use the monochrome mark when the lime
section lacks contrast at small sizes on a light surface.
These assets are authored directly as SVG, with no embedded raster images.

## How to use it

- Use `asset/chain-sdk.svg` for the SDK name and symbol on dark backgrounds.
- The standalone logo has exactly two variants, both with transparent backgrounds:
  `asset/chain-mark.svg` keeps the fixed lime and pale logo colors;
  `asset/chain-mark-mono.svg` fills both sections with `currentColor`.
- Inline the monochrome SVG and set CSS `color` to recolor it. An external
  SVG loaded through an `img` element does not inherit the page's `color`;
  use it as a CSS mask with `background-color: currentColor` instead.
- Present the fixed-color logo on a dark surface, or use the app icon's
  built-in charcoal background. Use the recolorable version on light surfaces.
- Keep the viewBox proportions and built-in clear space. Prefer the standalone
  mark at 32 px or larger; the lockup is intended for headers and larger placements.
- The lockup retains editable text and requests Inter, Helvetica Neue, then Arial.
  Text metrics vary with the installed font. Convert text to outlines in a vector
  editor before sending artwork to a printer that requires fixed typography.

`chain init` supplies `asset/app-icon.svg` and `asset/icons/` to new apps as
placeholder branding until the developer replaces it. The application version
places the symbol on a charcoal rounded square, with transparent outer padding.
The generated app's `AGENTS.md` explains how to configure its native runtime
to use these icons and regenerate them when replacing the artwork.

To regenerate this repository's native icons with the installed playground CLI:

```bash
apps/playground/node_modules/.bin/tauri icon asset/app-icon.svg --output asset/icons
```

The generator also produces mobile directories; only desktop files at the
top level of `asset/icons/` are copied into the CLI's current desktop scaffold.

The playground uses the same desktop files in `apps/playground/src-tauri/icons/`
and an SVG copy at `apps/playground/public/chain.svg` for its browser favicon.
Its UI header imports `asset/app-icon.svg` directly, so it shares the artwork
without embedding a second set of SVG paths. Superseded logo concepts and
unused template logos have been removed.
After regenerating, refresh these copies too. Existing initialized applications
keep their files; `chain init` never overwrites their branding.

## Files to check

- [`chain-mark.svg`](../../asset/chain-mark.svg) — primary two-color symbol.
- [`chain-mark-mono.svg`](../../asset/chain-mark-mono.svg) — single-color symbol.
- [`chain-sdk.svg`](../../asset/chain-sdk.svg) — horizontal SDK lockup.
- [`app-icon.svg`](../../asset/app-icon.svg) — editable desktop placeholder source.
- [`icons/`](../../asset/icons/) — generated desktop icon formats used by `chain init`.
