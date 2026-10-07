# Documentation Website — Plan

A plan for a public-facing docs website explaining Chain's framework
pieces and capabilities to people who want to *use* the framework in
their own app — a different audience from `agent-docs/`, which is
written for an AI agent (or a maintainer) extending Chain itself. This
document is the plan. **Status:** built in `site/` with Next.js (the
stack the team chose over the options below); phases 1–3 and the Contributing page are done (no FAQ yet, no hosting yet) —
see `agent-docs/framework/site/README.md`.

## Why this needs to be a separate thing from `agent-docs/`

`agent-docs/<name>/README.md`/`CONTRACT.md`/`AGENTS.md` are already
excellent, accurate, and current — but they're written for the wrong
reader for this purpose. A `CONTRACT.md`'s Non-goals section, an
`AGENTS.md`'s "what's NOT done yet" checklist, and phrases like "see
rule 7" or "don't revisit this incrementally without re-reading X" are
exactly right for an agent picking up maintenance work, and exactly
wrong for a developer who just wants to know how to call
`desktop.files.write()` in their own Tauri app. The doc site is a
**translation layer for a different audience**, not a replacement.

This means the doc site should never become a second, independently
maintained source of truth — see "Keeping content in sync" below for
how content stays derived from the existing `agent-docs/`/`contract.ts`
sources rather than drifting from them.

## Audience and what they need

A developer evaluating or building with Chain, who:

- Wants to know what Chain *is* before anything else (one paragraph, not
  the full `MNEME_DESKTOP_FRAMEWORK.md` history).
- Wants a working app in under 5 minutes (`chain init` → `chain dev`).
- Wants to look up one capability's API while writing code (fast
  reference lookup, not a narrative read).
- Wants to know, honestly, what's actually production-ready vs.
  experimental vs. Windows-unverified — this audience needs the Draft/
  Experimental/Stable and per-platform status `component.json` already
  tracks, just presented plainly instead of buried in an `AGENTS.md`
  TODO checklist.
- Is *not* trying to add a new capability to Chain itself (that reader
  stays served by `docs/CAPABILITY_WORKFLOW.md` + `agent-docs/`) — this
  site gets a lighter "Contributing" pointer, not the full workflow
  inlined.

## Proposed site structure

1. **Home** — what Chain is (one paragraph), the layer diagram (reused
   from `docs/ARCHITECTURE.md`), and links into Getting Started.
2. **Getting Started** — install (`chain doctor`), `chain init
   <name>`, what gets scaffolded, `chain dev`, first capability call,
   `chain build`.
3. **CLI Reference** — one entry per `chain` subcommand (`init`, `dev`,
   `build`, `update`, `inspect`, `migration`, `database update`,
   `database list`, `doctor`, `--help`, `--version`), sourced from
   `agent-docs/framework/command/README.md` and `packages/cli/src/*.ts`'s
   actual `--help` text (so the docs can't silently drift from what the
   CLI really prints).
4. **Core Concepts** — capability model (what a "capability" is),
   `ChainError`/`ChainErrorCode` (the normalized error contract every
   capability shares), capability detection vs. platform detection,
   events over polling, the Draft/Experimental/Stable status meaning.
   One page, written once, linked from every capability page instead of
   re-explained on each one.
5. **Capabilities** — one page per capability, generated from the same
   three sections every `agent-docs/capabilities/<name>/README.md`
   already has, translated for this audience:
   - Platform (`desktop.platform`)
   - Storage (`desktop.storage`)
   - Files (`desktop.files`)
   - Http (`desktop.http`)
   - AgentServer (`desktop.agentServer`)
   - ProcessRunner (`desktop.processRunner`)

   Each page: what it's for (one paragraph, no internal rationale), API
   reference (typed signatures, pulled from `contract.ts`), a runnable
   example (adapted from the README's "How to use it"), the error codes
   it can throw, and a platform-support line sourced directly from
   `component.json` (`macos`/`windows`/`linux` status) so that fact in
   particular can't drift from reality.
6. **Contributing** — one short page: "Chain grows through real capability
   requests, not speculative ones — see `docs/CAPABILITY_WORKFLOW.md` in
   the repo if you want to propose or build one," linking into the repo
   rather than duplicating that process here.
7. **FAQ / Troubleshooting** — seeded from real questions as they come
   up (empty at launch, not pre-filled with guesses).

## Keeping content in sync (the actual hard part)

The real risk with any docs site is that it's accurate on day one and
quietly wrong by month three, because updating it is a separate,
skippable step from updating the code. Three concrete decisions to
prevent that, in increasing order of effort — start with the first,
add the others if drift becomes a real problem in practice (rule 7:
don't build the heavier mitigation speculatively):

1. **Extend the existing README.md rule to this site.** `AGENTS.md`
   already states "every feature you add or materially change must get
   or update its `README.md` in the same change" — add one line to that
   same rule: a capability's doc-site page gets the same treatment,
   same change, same PR. Zero new tooling, just scope-extending a
   discipline that's already enforced by convention (and by the
   `.claude` hook the workflow doc mentions).
2. **Source the platform-support table directly from `component.json`**
   at build time (a small script reading `capabilities/*/component.json`
   into the site's data), rather than hand-typing "macOS: Experimental"
   prose that can silently go stale the next time a capability is
   verified further. This is the one fact most likely to drift
   invisibly (nobody remembers to update prose when they add a Windows
   verification), so it's worth automating first even under a
   "start minimal" plan.
3. **Pull type signatures from `contract.ts` directly** (a build step
   that extracts each capability's public interface into the API
   reference code block) instead of hand-copying them — prevents a
   renamed/changed method from silently going undocumented. Proposed as
   phase 2+, not launch-blocking: hand-copied signatures are fine to
   start with as long as (1) above catches renames in practice.

## Tech stack — decided: Next.js

The team chose Next.js (App Router, static export). The comparison
below is kept for the record.

### Options considered

This is a real decision worth naming explicitly rather than defaulting
into. Chain is a TypeScript/Rust monorepo already on npm workspaces, and
the content here is markdown-first, API-reference-heavy, with no need
for a blog, multi-version docs, or i18n at launch — that rules out
heavier options (Docusaurus) in favor of a lighter, markdown-native
static site generator. Two realistic candidates:

- **VitePress** — minimal config, fast, Vue-powered but content stays
  plain markdown; a natural fit if the team wants the smallest possible
  surface area and is comfortable with Vue's docs theme. Used by Vite,
  Vitest, and most of the Vite ecosystem's own docs.
- **Starlight** (Astro) — also markdown-first, slightly more built-in
  structure for large reference sections (auto-generated sidebars from
  file structure, built-in search via Pagefind), which fits this site's
  "one page per capability, all API-reference-shaped" content well.

**Leaning Starlight** for the built-in reference-doc affordances (auto
sidebar, search) that this specific content shape (six-plus near-
identical capability reference pages) benefits from more than a general
blog/guide site would — but this is a genuine, low-stakes-either-way
call the team should confirm before scaffolding, not something to lock
in from this plan alone.

## Where it lives in the repo

A new top-level `site/` directory, sibling to `docs/`, `packages/`,
`crates/` — **not** under `apps/`, since that directory is reserved for
real Chain (Tauri) apps per `AGENTS.md` (`apps/playground` is
explicitly "the framework's own proof app"); a public marketing/docs
website is a different kind of artifact entirely and shouldn't be
confused with one by living in the same place.

## Phased milestones

1. **Skeleton** — site scaffolded (`site/`), Home + Getting Started +
   Core Concepts + exactly one capability page (Platform — the
   simplest, already fully implemented and verified) as the template
   the rest follow.
2. **Full capability reference** — the remaining five capability pages
   (Storage, Files, Http, AgentServer, ProcessRunner) + CLI Reference.
3. **Sync tooling** — the `component.json`-sourced platform-support
   table (see "Keeping content in sync" #2).
4. **Contributing + FAQ**, then a deploy decision (see Open questions).

## Open questions this plan deliberately leaves open

- **Hosting**: GitHub Pages, Vercel, Cloudflare Pages, or something
  else — not decided, not needed until Phase 1 is ready to ship
  somewhere real.
- **Domain/branding**: whether this lives under a `chain-sdk` GitHub
  Pages URL or a real custom domain — a product decision outside this
  plan's scope.
- **Versioning**: Chain is pre-1.0 and every capability is still
  Draft/Experimental — this plan assumes a single "current" version of
  the docs, no version switcher, until the framework itself has a
  stable release to version against.
