# Capability Matrix

Status of every Chain capability, per platform. Update this whenever a
capability's status changes — this is the fastest way for a human or agent
to see what actually exists vs. what's just planned.

| Capability      | macOS | Windows | Linux | Contract |
| --------------- | ----- | ------- | ----- | -------- |
| Platform/System | 🧪    | ⏳      | ⏳    | Draft    |
| Storage         | 🧪    | ⏳      | ⏳    | Draft    |

Legend:

```
✅ Tested
🧪 Experimental
⏳ Not implemented
⚠ Partial
❌ Unsupported
```

`platform` has a native implementation wired end to end on macOS,
verified running in both `apps/playground` and `mneme`'s own window —
still marked Experimental, not Tested, until contract tests exist and
it's verified on Windows too (`capabilities/platform/AGENTS.md`).

`storage` (SQLite-backed `migrate`/`query`/`execute`) is implemented and
verified end to end on macOS — real inserts/queries through the full
React → SDK → Tauri → Rust → SQLite path in `mneme`'s running window,
including persistence across app restarts — but not yet verified on
Windows (`capabilities/storage/research/WINDOWS.md` has specific risks
to check first: WAL over network drives, antivirus file locking).

See `docs/FRAMEWORK_CANDIDATES.md` for what's next.
