# Packages — Windows Research

Not built or run on Windows. The code is plain Rust shared with models,
with nothing per-OS. To check on real hardware:

- [ ] Install Tailwind CSS IntelliSense and SonarLint from Open VSX;
      `resolve` returns a usable path and `node`/`java` start from it.
- [ ] Long paths: a VSIX may hold deep `node_modules/…` paths under
      `%APPDATA%\<identifier>\packages\<id>\`. Past 260 characters,
      `std::fs` needs the `\\?\` form or long-path support enabled.
- [ ] Atomic replace while a server still runs from the old copy:
      Windows refuses to rename a folder with open files, so reinstalling
      (or removing) a package in use likely fails `NATIVE_FAILURE`.
      Callers should stop the server first, as the contract says.
- [ ] Antivirus scanning the freshly unpacked files can make the rename
      fail transiently.
