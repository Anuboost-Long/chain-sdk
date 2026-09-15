# Chain Core (`crates/core`)

## How it works

`chain-core` is the Rust crate meant to coordinate every capability,
normalize errors, and route to the right native adapter (see
`docs/ARCHITECTURE.md`). It's registered in the root `Cargo.toml`
workspace (`members = ["crates/core"]`), so `cargo build`/`cargo test`
from the repo root operate on it.

Each capability gets its own module here (e.g. `src/platform.rs`),
declared in `src/lib.rs`. Right now the crate only contains the
`platform` module, and it's an `unimplemented!()` stub — it compiles
(verified with `cargo build`), but calling `get_platform_info()` panics
until that's implemented for real.

Rust is a build-time-only dependency: it's needed on a developer's
machine to build a Chain app, never on an end user's machine (Tauri
compiles it into the shipped native binary). Run `chain doctor` to
check for it, and optionally install it via rustup.

## How to use it

Not consumable yet — nothing bridges it to the SDK or to a runtime. Once
`apps/playground` is scaffolded (see that app's `AGENTS.md`), this crate
will be built as a native dependency of the Tauri app there.

```bash
cargo build   # from repo root, once Rust is installed
```

## Files to check

- `Cargo.toml` (repo root) — workspace member list; add new crates here
  if the framework ever splits into more than one.
- `crates/core/Cargo.toml` — crate metadata (library name `chain_core`).
- `crates/core/src/lib.rs` — module registration; add `pub mod
<capability>;` here for each new capability's Rust module.
- `crates/core/src/platform.rs` — the platform capability's Rust-side
  stub (see `platform-capability` doc for what's left to implement).
