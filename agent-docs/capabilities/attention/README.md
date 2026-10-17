# Attention Capability (`desktop.attention`)

## How it works

- **Focus** (`isFocused`, `onFocusChange`) and the **Dock bounce**
  (`requestAttention`) are Tauri's own window methods, exposed by
  `packages/cli/templates/attention.rs`.
- **Notifications** are `UNUserNotificationCenter`, in
  `crates/core/swift/ChainAttention.swift`, bridged by
  `crates/core/src/attention.rs`. `notify()` asks for permission on first
  use, then posts the notification with the app's id. A delegate
  installed at launch receives clicks. The template brings the window
  that showed the notification forward and emits
  `chain://attention-click` to its page.
- **Only in a built app.** The system API aborts a process that isn't
  an `.app` bundle, so under `chain dev` notifications report
  `unavailable` instead.

## How to use it

```ts
import { desktop } from "@chain/sdk";

desktop.attention.onNotificationClick((runId) => openRun(runId)); // the window is already in front

async function agentNeedsYou(run: Run) {
  if (await desktop.attention.isFocused()) return;
  const result = await desktop.attention.notify({
    id: run.id,
    title: `${run.agentName} needs you`,
    body: `${run.projectName} is waiting for a response.`
  });
  if (result.outcome !== "shown") showWhyNoNotification(result); // denied / unavailable / failed + message
  await desktop.attention.requestAttention(); // one Dock bounce
}

const permission = await desktop.attention.notificationPermission(); // granted | denied | notDetermined | unavailable
```

## Files to check

- `agent-docs/capabilities/attention/CONTRACT.md` — outcomes, the
  bundle-only rule, non-goals.
- `capabilities/attention/contract.ts` — the types.
- `crates/core/swift/ChainAttention.swift` — the bundle check,
  permission, posting, and the click delegate.
- `crates/core/src/attention.rs` — the C bridge and the unbundled test.
- `crates/core/build.rs` — compiles the Swift file with the others.
- `packages/cli/templates/attention.rs` (playground copy alongside) —
  commands, focus events, and bringing the window forward on a click.
- `packages/sdk/src/attention.ts` — the SDK wrapper.
- `agent-docs/capabilities/attention/research/MACOS.md` — the bundle
  crash, and the unresolved refusal of ad-hoc test bundles.
