# Ports Capability — Contract

## What this is

One question: **could a server starting now bind this TCP port?** Only a
real bind answers it exactly, and a webview can't open sockets. Driven by
Lazify's request 05. Before running a dev script, Lazify steps up from
the tool's default port (5173, 3000, ...) to the first free one. After
stopping `dotnet run`, it waits for Kestrel's ports to free up (a probe
every 150 ms, up to 5 s) before relaunching.

## `isFree(port)`

```
isFree(port: number): Promise<boolean>
```

Binds a TCP listener to `port` on each of these addresses in turn, and
closes each one immediately:

| Address     | Catches a server on              |
| ----------- | -------------------------------- |
| `0.0.0.0`   | all IPv4 interfaces              |
| `127.0.0.1` | IPv4 loopback only               |
| `::`        | all IPv6 interfaces (dual-stack) |
| `::1`       | IPv6 loopback only (Vite on macOS) |

It resolves `true` only if every bind succeeds, and `false` as soon as
one is refused because the address is in use, **or because the OS won't
allow it** (e.g. a privileged port). The same probe a server would hit.

**Why four addresses, the decision Lazify left to Chain.** Servers set
`SO_REUSEADDR` (Node, Kestrel, and Rust's `std` all do). With it, macOS
only reports a conflict for a bind to the *exact* address a server holds
(verified, see `research/MACOS.md`). Electron's `listen(port, "0.0.0.0")`
probe therefore reads a server on `::1` *and* one on `127.0.0.1` as
free. "Free" here means free for a dev server on either family, which is
what Lazify asked for.

- An address the machine doesn't have (IPv6 disabled) is skipped, not
  counted as busy.
- Nothing is left listening, and no connection is made. Calling it in a
  loop is fine: 100 sequential probes took ~30 ms through the SDK.
- A port free now can be taken a moment later by another process. The
  answer is a snapshot, not a reservation.
- Two `isFree` calls on the same port at the same instant can see each
  other's probe and report it busy. Probe from one place at a time.
- A port held in `TIME_WAIT` after a server closed reads as **free**: a
  server using `SO_REUSEADDR` can bind it, so the answer matches what
  the next server will see.
- A server bound only to some other address (a LAN IP, say) isn't
  detected.

## Errors

- `INVALID_ARGUMENT` when `port` isn't an integer from 1 to 65535.
- `NATIVE_FAILURE` for any other failure to probe.
- `UNSUPPORTED` outside a Chain runtime.

## Non-goals

- **No listing ports, finding the process that holds one, or killing
  it.** Lazify runs `lsof`/`ps` through `process-runner` and parses the
  output itself.
- **No listening or reserving.** The probe socket never accepts
  connections and is closed before `isFree` resolves.
- **No "find me a free port" helper.** The app owns the stepping policy
  (start port, limit, order). It's one loop over `isFree`.
- **No UDP.**
