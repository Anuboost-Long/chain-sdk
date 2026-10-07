# Documentation site (`site/`)

## How it works

A Next.js 16 App Router site for developers *using* Chain, built as a
static export (`output: "export"` in `site/next.config.ts`): every page
is prebuilt HTML, and Next's client router handles navigation after the
first load, so moving between pages never reloads. It's a translation of
`agent-docs/` for a different reader, not a second source of truth — see
`docs/DOC_SITE_PLAN.md` for why.

- **Pages are MDX** under `site/app/(docs)/<route>/page.mdx`, rendered
  inside `(docs)/layout.tsx` (sidebar, content, on-page contents).
  The home page (`app/page.tsx`) is the only TSX page and has its own
  layout.
- **`site/lib/nav.ts` is the single ordered page list.** The sidebar,
  previous/next links, and search all read it, so a new page only needs
  a `page.mdx` plus one entry there.
- **Capability facts come from the repo at build time**
  (`site/lib/capabilities.ts`, server-only): `<PlatformSupport name>`
  and `<CapabilityTable>` read `capabilities/<name>/component.json`;
  `<ContractSource name>` prints `capabilities/<name>/contract.ts`
  verbatim. Changing either file changes the site on the next build,
  with no copy to forget. That's also why `component.json` must stay
  accurate: the site shows whatever it says.
- **Everything else on a capability page is hand-written** (example,
  per-method explanation, Behavior, Errors, Not included), adapted from
  that capability's `CONTRACT.md` and `README.md`.
- **Theme:** tokens in `site/app/globals.css` use the brand palette
  (see `framework/brand/`). Dark is default; light follows
  `prefers-color-scheme` and swaps lime text for olive `#3F5C00`. Code
  is highlighted by shiki with two brand themes (`site/lib/code-themes.ts`)
  emitting `--shiki-dark`/`--shiki-light` per token; one CSS rule picks
  the right one for both MDX code blocks (rehype-pretty-code) and
  `<ContractSource>`.
- **Plugins are named as strings** in `next.config.ts` because Turbopack
  can only pass serializable MDX plugin options.

## How to use it

From the repo root (each installs `site/`'s dependencies first if needed):

```bash
npm run docs         # dev server at http://localhost:3000
npm run docs:build   # static site in site/out/
```

Or inside `site/`: `npm install`, then `npm run dev` / `npm run build`.

**When a capability's behavior changes, update its page in the same
change** — its example, Behavior, and Errors sections. The API block and
platform-support line update themselves. A new capability needs a page
under `app/(docs)/capabilities/<name>/`, an entry in `lib/nav.ts`, and
one in `capabilityPages` in `components/capability.tsx`. A new CLI
command gets a page under `app/(docs)/cli/<command>/`.

Examples of CLI output on the site are copied from the real
`console.log` strings in `packages/cli/src/` — check them there when a
command's output changes.

## Files to check

- `site/next.config.ts` — static export, MDX, and code-highlighting setup.
- `site/lib/nav.ts` — page order and titles for sidebar, search, prev/next.
- `site/lib/capabilities.ts` — reads `component.json` and `contract.ts`.
- `site/components/capability.tsx` — `PlatformSupport`, `ContractSource`,
  `CapabilityTable`, and the shared `highlight()`.
- `site/components/layer-trace.tsx` — the home page's call-through-the-layers
  animation (plays once; static under reduced motion).
- `site/app/globals.css` — tokens, prose styles, code-block theming.
- `site/mdx-components.tsx` — routes internal MDX links through `next/link`.
