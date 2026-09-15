# {{name}} — Agent Memory

{{name}} consumes the Chain SDK (`@chain/sdk`) from the sibling `chain-sdk`
repo. It must never import Tauri, Rust, or any native/OS API directly —
only `@chain/sdk`. See that repo's `AGENTS.md` and `docs/ARCHITECTURE.md`
for the rules this app has to follow, and
`capabilities/<name>/CONTRACT.md` for what each capability actually does.

## Current state

This is the scaffold produced by `chain init`:

- Tauri + React + TypeScript (via `create-tauri-app`) — a real native
  runtime, not just a browser page. `npm run dev`/`npm run build` are the
  real thing (`tauri dev`/`tauri build`) — this was deliberately changed
  from `create-tauri-app`'s default, where those names only ran Vite.
  Use `npm run dev:web`/`npm run build:web` for frontend-only iteration.
  If you ever rename these scripts, update
  `src-tauri/tauri.conf.json`'s `beforeDevCommand`/`beforeBuildCommand`
  to match, or `npm run dev`/`npm run build` will recurse into
  themselves infinitely.
- Tailwind CSS v4, wired through `@tailwindcss/vite` in `vite.config.ts`.
  `src/App.css` defines the `chain-navy`/`chain-lime`/`chain-cream` theme
  tokens (matched to `asset/app-icon.svg`) via a Tailwind `@theme` block —
  use them as plain classes (`bg-chain-navy`, `text-chain-lime`, ...),
  don't reach for arbitrary-value brackets or a different palette without
  a reason. Add custom CSS there only when a utility class genuinely
  can't express it.
- Routing via `react-router-dom`'s data router — standard structure:
  - `src/router.tsx` — `createBrowserRouter` route tree. Add new
    top-level pages here as siblings; nest under a parent route only when
    pages genuinely share layout beyond `RootLayout`.
  - `src/layouts/RootLayout.tsx` — shared chrome (`NavBar` + `<Outlet/>`).
  - `src/components/` — pieces shared across routes (currently `NavBar`).
  - `src/pages/` — one component per route (`Home.tsx`, `About.tsx`).
  - `src/App.tsx` just renders `<RouterProvider router={router} />`;
    `src/main.tsx` is untouched from `create-tauri-app`'s default.
    This is standard in-window SPA routing, not Tauri's multi-window API —
    multiple native windows are a different pattern (separate OS-level
    windows, not in-page navigation) and would be a deliberate later choice
    if this app ever needs genuinely separate windows, not a default.
- `src/pages/Home.tsx` calls `desktop.platform.getInfo()` to prove the
  app can reach the Chain SDK end to end.
- Chain's placeholder branding: `asset/app-icon.svg` (used in the nav
  bar) and `asset/icons/` (the full desktop icon set), also copied into
  `src-tauri/icons/` where Tauri's bundler actually reads them from
  (`src-tauri/tauri.conf.json`'s `bundle.icon` already points there —
  no config change needed to use them).

## Staying in sync with chain-sdk

Run `chain update` from this app's root any time chain-sdk's templates
change. It does a real three-way merge (like git), not a reset — files
you haven't touched pick up the new template silently; files you've
edited merge cleanly if the changes don't overlap, or get real `<<<<<<< /
======= / >>>>>>>` conflict markers if they do (resolve those before
`npm install` or building). `.chain/baseline/` is the merge ancestor this
depends on — **commit it to git, don't delete or gitignore it**.

## Replacing the icon

1. Replace `asset/app-icon.svg` with your own square SVG.
2. Regenerate the desktop set: `npx tauri icon asset/app-icon.svg --output asset/icons`
   (run from this app's root, after installing the Tauri CLI).
3. Copy the regenerated files from `asset/icons/` into `src-tauri/icons/`
   — that's the copy Tauri's bundler actually uses.

## Next steps

- [ ] Replace `Home`/`About` with real screens — this scaffold's pages
      are placeholders, not a design to keep.
- [ ] Do not add capabilities here speculatively. A capability only gets
      built in `chain-sdk` when this app has a real requirement for it
      (see `chain-sdk/AGENTS.md` rule 7).
- [ ] If this app ever needs genuinely separate OS windows (not just
      in-page routes), that's a Tauri multi-window decision to make
      deliberately — don't reach for it by default.
