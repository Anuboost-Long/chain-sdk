# Terminal — Windows Research

**Not compiled or run on Windows.** `portable-pty` uses ConPTY
(`CreatePseudoConsole`, Windows 10 1809+) on Windows, the same as
node-pty, so `crates/core/src/terminal.rs` has no Windows-specific code
of its own.

Known differences to check:

- **ConPTY re-renders output.** It emits its own escape sequences and may
  repaint, so the bytes differ from macOS for the same program. Aim for
  visual parity, not byte parity.
- **npm `.cmd` shims.** `CreateProcess` doesn't apply `PATHEXT` to a bare
  name, and a `.cmd` needs `cmd.exe /c`. `portable-pty`'s Windows
  `search_path` does try `PATHEXT` extensions. Whether that launches
  `npm.cmd` correctly inside ConPTY is unverified, and it's the same open
  question as process-runner's `research/WINDOWS.md`. Lazify's Electron
  app wraps these as `cmd.exe /c <command>`.
- **Tree kill** is `taskkill /T /F` (`process_tree.rs`), with no grace
  period. A Job Object would be more robust; see process-runner's
  `research/WINDOWS.md`.
- **No `SIGWINCH`.** `ResizePseudoConsole` tells the console host
  directly; `MasterPty::resize` does that.
- **The exit drain:** ConPTY keeps the output pipe open until the pseudo
  console is closed, so the 1 s drain after exit may always be used
  there. Check that exits still arrive promptly.

Checklist:

- [ ] `start({ command: "npm", args: ["run", "dev"], cwd })` shows coloured
      output; `kill()` frees the port.
- [ ] `claude` (a `.cmd` shim) runs its TUI; resize redraws it.
- [ ] Reload the page mid-run; `attach()` replays without gaps.
- [ ] Bracketed paste reaches the program intact.
