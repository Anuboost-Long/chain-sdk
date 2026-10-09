# KeepAwake — Windows Research

**Not implemented.** `start()` rejects `UNSUPPORTED` on Windows.

Use `PowerCreateRequest` (with the reason in a `REASON_CONTEXT`) plus
`PowerSetRequest(PowerRequestSystemRequired)` and, for `display`,
`PowerRequestDisplayRequired`, then `PowerClearRequest` and close the
handle. Requests die with the process. The older
`SetThreadExecutionState` has no reason string and is per-thread.

- [ ] `powercfg /requests` shows the reason while held, and not after
      `stop()` or killing the app.
