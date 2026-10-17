# Packages — macOS Research

- Plain Rust (`reqwest`, `sha2`, `zip`, `tar`) shared with models; no
  native API involved.
- Files are created with `create_new` and the default mode (0644 under
  the usual umask); a zip entry's stored Unix mode (e.g. 0755) is
  ignored. Verified with a 0755 entry in a unit test and with Tailwind's
  real VSIX (`-rw-r--r--`).
- Nothing Chain downloads gets `com.apple.quarantine` (that's set by
  apps that opt in via `LSFileQuarantineEnabled`, which Chain apps don't).
  It doesn't matter here: a `.jar` or `.js` is read by `java`/`node`,
  which are the user's own signed binaries, not launched itself.
  Not verified: whether an unsigned app bundle downloaded this way would
  run. Out of scope (nothing is executable).
- Verified end to end against Open VSX on 14 October 2026; see AGENTS.md.
