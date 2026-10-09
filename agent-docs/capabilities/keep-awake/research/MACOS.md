# KeepAwake — macOS Research

Verified on macOS 27, 9 October 2026.

`IOPMAssertionCreateWithName(type, kIOPMAssertionLevelOn, name, &id)` and
`IOPMAssertionRelease(id)` from IOKit, called directly from Rust
(`crates/core/src/keep_awake.rs`, linking the IOKit and CoreFoundation
frameworks; no Swift). The type strings are
`"PreventUserIdleDisplaySleep"` and `"PreventUserIdleSystemSleep"`. The
display assertion implies the system one.

Checked with `pmset -g assertions`:

- While held, it lists
  `pid <app>(lazyprobe): … PreventUserIdleDisplaySleep named: "<reason>"`.
- `start()` again with a new reason: the new name is listed and the old
  one is gone. It's created before the old one is released, so there's no
  gap.
- After `stop()`: not listed.
- After `kill -9` of the app while held: not listed within 1.5 s. The
  kernel drops a dead process's assertions.

The unit test runs the same `pmset` checks.
