# Handling a Capability Request

What to do, step by step, when you're handed a path to a capability
request markdown file — e.g. "check `docs/chain-sdk-requests/01-local-storage.md`
inside the mneme project folder and start preparing the native module."
This doc exists so a fresh agent session with zero prior context can pick
this up correctly on its own. It generalizes the exact process used for
the `storage` capability (`capabilities/storage/` +
`agent-docs/capabilities/storage/`) — read that as a worked example
alongside this doc if anything here is ambiguous.

A request file like this normally lives in the *consuming app's* own
repo (e.g. `mneme/docs/chain-sdk-requests/`), not in `chain-sdk` — it's
that app's way of relaying "here's a real need, here's what I already
found researching it." Read it fully before doing anything else; it
usually already contains a native-module survey (what's shared across
platforms vs. platform-specific) that saves you re-deriving it.

## Step by step

1. **Read the request file completely.** Note: what capability is being
   asked for, why now (it must be a real requirement — rule 7, never
   speculative), and what research the requester already did. Don't
   start implementing yet.

2. **Research (rule 8 — findings become permanent knowledge, not left in
   conversation).** Write `agent-docs/capabilities/<name>/research/MACOS.md`
   and `research/WINDOWS.md`, expanding on whatever the request file
   provided. Figure out early: does this need real per-OS native code
   (Swift on macOS, C#/.NET on Windows — like Audio or Clipboard), or is
   the underlying library already portable (like SQLite for `storage`,
   where the same Rust code covers both platforms with no native
   adapter)? This determines how much of the rest of this list involves
   writing Swift/.NET vs. just Rust.

3. **Contract first (rule 1 — never write implementation code before
   this).** Draft:
   - `agent-docs/capabilities/<name>/CONTRACT.md` — semantic contract:
     what each method means, its error behavior, and its **explicit
     non-goals** (write down what this capability deliberately does
     *not* do, so nobody accidentally scope-creeps it later).
   - `capabilities/<name>/contract.ts` — structural contract: the exact
     TypeScript types. Name things for what they mean to an app
     developer, never for what the native API happens to call them
     (rule 2 — no porting Swift/WinRT method names into the public SDK).
   - `capabilities/<name>/component.json` — status `"draft"`,
     `platforms` per OS (`"not-started"`/`"experimental"`/etc.), and a
     `contract.semantic` field pointing at the `CONTRACT.md` above
     (relative path, e.g. `../../agent-docs/capabilities/<name>/CONTRACT.md`).

4. **Write `agent-docs/capabilities/<name>/AGENTS.md`** — the
   capability's own memory: what's already decided, a concrete "what's
   NOT done yet" checklist, and any capability-specific rules (e.g.
   "don't let this grow into a grab-bag" — see
   `agent-docs/capabilities/platform/AGENTS.md` for the pattern). You'll
   keep updating this file's Status section as you go.

5. **Implement.**
   - Rust: `crates/core/src/<name>.rs`, registered in
     `crates/core/src/lib.rs` (`pub mod <name>;`), dependencies added to
     `crates/core/Cargo.toml`. Write a real unit test
     (`cargo test -p chain-core`) — don't rely on the Tauri layer to be
     your only test coverage.
   - If real native code is needed (Swift/.NET), that's a separate,
     larger piece of work per platform — research the native APIs first
     (section 9 of `docs/MNEME_DESKTOP_FRAMEWORK.md`), then bridge
     through Rust with plain data only, never native objects (section
     29 of that doc).

6. **Wire the Tauri command layer in two places:**
   - `packages/cli/templates/lib.rs` — this is what every `chain init`
     and every existing app's `chain update` picks up. This is how the
     capability reaches the requesting app without a manual patch.
   - `apps/playground/src-tauri/src/lib.rs` — kept in sync by hand
     (playground isn't `chain init`-managed); the framework's own proof
     app should demonstrate every capability, not just the first one.

7. **Write the SDK wrapper**: `packages/sdk/src/<name>.ts`, implementing
   the contract — check `isTauri()` and reject with `ChainError`
   `UNSUPPORTED` outside a real Tauri runtime, wrap native failures as
   `NATIVE_FAILURE` (rule 5 — normalized errors, see
   `packages/sdk/src/platform.ts` or `storage.ts` for the pattern). Add
   it to `desktop` and export its types from `packages/sdk/src/index.ts`.

8. **Typecheck and build everything before touching the requesting app:**
   `tsc --noEmit` in `packages/sdk`, `cargo check`/`cargo test -p
   chain-core`, `cargo check` in `apps/playground/src-tauri`.

9. **Propagate to the requesting app with `chain update`** (from inside
   that app's directory) — not a fresh `chain init`, which would erase
   whatever the app already has. This is also a real-world exercise of
   the merge machinery itself, not just the new capability.

10. **Verify end to end for real, not just "it compiles."** Actually run
    the requesting app (`npm run dev`) and exercise the new capability.
    If there's no existing UI hook to trigger it, add a temporary probe
    (e.g. a `useEffect` call plus a Rust-side `eprintln!` so you can see
    the result in the process log without needing a devtools console),
    confirm real behavior, then **revert the probe** back to the clean
    template/app state and re-run `chain update` to confirm "already up
    to date."

11. **Documentation sweep — do this in the same pass, not as an
    afterthought (there's a `.claude` hook that will remind you if you
    forget):**
    - `agent-docs/capabilities/<name>/README.md` — the standard three
      sections (How it works / How to use it / Files to check). Add it
      to the index in `agent-docs/README.md`.
    - `docs/CAPABILITY_MATRIX.md` — add/update the capability's row.
    - `docs/FRAMEWORK_CANDIDATES.md` — add a section (used by,
      generalizable, contract status, per-platform status, possible
      package).
    - `agent-docs/capabilities/<name>/AGENTS.md`'s Status section — what's
      actually verified (and how), and what's still TODO.

12. **Be honest about what's not verified (rule 3 — no single-platform
    contracts).** If you only had access to one OS while implementing,
    say so explicitly in the research doc and in `component.json` —
    leave a concrete verification checklist for whoever has the other
    platform, rather than marking anything stable/Tested prematurely.

## What this looks like when it's genuinely done

Everything in step 11 exists, the capability's `component.json` and
`docs/CAPABILITY_MATRIX.md` accurately reflect what's verified vs. not,
`agent-docs/capabilities/<name>/AGENTS.md`'s TODO list is up to date, and
the requesting app has actually run the capability successfully in its
own window — not just "the code looks right."
