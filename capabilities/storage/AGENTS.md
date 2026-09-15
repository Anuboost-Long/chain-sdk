# Storage Capability — Agent Memory

Scope: `desktop.storage.migrate/query/execute` — persistent SQLite-backed
local storage. Requested by mneme (see
`docs/chain-sdk-requests/01-local-storage.md` in the mneme repo) as the
first real gap beyond `platform`: mneme's Course/Module/Page/Attachment
schema needs somewhere to live, and mneme's own rule is to never touch a
native filesystem/OS API directly.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/storage.rs` / `packages/sdk/src/storage.ts`

## What's already decided

- One SQLite database per app, opened lazily on first
  `migrate`/`query`/`execute` call — no separate "open" step in the
  public API (see CONTRACT.md).
- `rusqlite` with the `bundled` SQLite feature — same Rust code on every
  platform, no per-OS branching (SQLite is a portable C library; what
  differs per OS is only where Tauri resolves the app-data directory to,
  which Tauri already handles).
- Rows come back as plain JSON objects (column name → value), not typed
  models — schema/query design is entirely the consuming app's job (see
  CONTRACT.md's Non-goals).
- Migrations are tracked in an internal `_chain_migrations` table this
  capability owns; re-running an already-applied migration is a no-op.

## Status

Implemented and verified for real on macOS:

- `crates/core/src/storage.rs` has a passing unit test
  (`migrate_query_execute_round_trip`) covering migrate → insert → query.
- Wired as real Tauri commands (`storage_migrate`/`storage_query`/
  `storage_execute`) in `packages/cli/templates/lib.rs` (propagates to
  every `chain init`/`chain update`'d app) and in `apps/playground`.
- Verified end to end in `mneme`'s actual running window: a temporary
  probe in `Home.tsx` ran `migrate` → `execute` (insert) → `query`
  through the real SDK → Tauri → Rust → SQLite path, confirmed via Rust
  stdout, including that rows **persisted across app restarts** (a real
  file-backed database, not an in-memory stand-in). The probe was
  reverted afterward — `Home.tsx`/`lib.rs` are back to the clean
  template.
- Propagated to `mneme` via `chain update` itself (not a fresh
  `chain init`) — real-world proof the update/merge feature works for a
  newly-added capability, not just a contrived test.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark the contract/component status
      stable until confirmed there (rule: no single-platform contracts).
      See `research/WINDOWS.md` for the specific risks to check (WAL over
      network drives, antivirus file locking) and the verification
      checklist.
- [ ] Add contract tests under `capabilities/storage/tests/` — currently
      the only test is `crates/core/src/storage.rs`'s Rust unit test,
      which doesn't exercise the Tauri command layer or the SDK's
      `isTauri()`/error-mapping behavior.
- [ ] Configure an explicit SQLite busy-timeout (`PRAGMA busy_timeout`)
      — not set yet; relevant to the Windows antivirus-locking risk in
      `research/WINDOWS.md` and generally good practice for concurrent
      access.
- [ ] No transaction API yet (CONTRACT.md non-goal) — add one only when
      mneme has a real multi-statement-atomicity need, not speculatively.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and
      `component.json`'s `platforms.windows` once Windows is verified.

## Rules specific to this capability

- Don't add app-level schema (Courses, Modules, ...) here — that's
  mneme's job, built on top of `migrate`/`query`/`execute`. This
  capability only knows how to run SQL, not what the SQL means.
- Don't add blob column support, multi-database support, or a
  transaction API speculatively — each is a deliberate non-goal until a
  real app hits the need (see CONTRACT.md).
