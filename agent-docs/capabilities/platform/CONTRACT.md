# Platform Capability — Contract

## `desktop.platform.getInfo()`

Returns basic information about the operating system and runtime the
application is currently running under.

```
desktop.platform.getInfo(): Promise<PlatformInfo>
```

### Behavior

- Always resolves. There is no failure mode for this call on a supported
  runtime — it does not touch permissions, hardware, or user data.
- If called on a runtime Chain does not support, it rejects with
  `ChainError { code: "UNSUPPORTED" }`.
- The application must never receive a Tauri-specific, Rust-specific, or
  platform-native object. Only the plain `PlatformInfo` shape below.

### `PlatformInfo`

```
os:             "macos" | "windows"
arch:           "arm64" | "x64"
runtimeVersion: string   // version of the underlying Chain Runtime (e.g. Tauri version)
```

`os` and `arch` are closed unions. Adding a new platform (e.g. `"linux"`)
is a contract change and must be reflected here and in `contract.ts`
together, not silently widened by one native implementation.

### Non-goals

This capability intentionally does NOT cover: hostname, locale, user
identity, hardware details beyond arch, or environment variables. Those
are separate future decisions, not implicit extensions of `getInfo()`.
