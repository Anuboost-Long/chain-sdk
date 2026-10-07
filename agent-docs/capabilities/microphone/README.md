# Microphone capability

## How it works

There's no JS API. An app records with the webview's own
`getUserMedia` + `MediaRecorder`; Chain's job is the OS declaration a web
page can't make. The app lists `microphone` under `chain.permissions` in
its `package.json`, and `chain dev`/`chain build`
(`packages/cli/src/permissions.ts`) turn that into
`.chain/native/Info.plist` (`NSMicrophoneUsageDescription`) and
`.chain/native/Entitlements.plist` (`com.apple.security.device.audio-input`,
passed to `tauri build` as a `--config` override). Both files carry a
"GENERATED" marker and are removed when the declaration goes away; a
hand-written `Info.plist` without the marker is never touched.

On macOS, wry's `WKUIDelegate` answers WebKit's media-capture request with
Grant, so the OS (TCC) prompt is the only one the user sees.

## How to use it

```json
// package.json
"chain": { "permissions": { "microphone": "Records lectures you choose to capture." } }
```

```ts
const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
const recorder = new MediaRecorder(stream);
const chunks: Blob[] = [];
recorder.ondataavailable = (e) => chunks.push(e.data);
recorder.onstop = async () => {
  const blob = new Blob(chunks, { type: recorder.mimeType });
  const reference = await desktop.files.write(new Uint8Array(await blob.arrayBuffer()), {
    extension: recorder.mimeType.startsWith("audio/mp4") ? "m4a" : "webm"
  });
};
recorder.start();
```

A refusal rejects `getUserMedia` with `NotAllowedError`. `chain dev`
builds prompt as the app itself too, with their own Privacy row — see
CONTRACT.md's "Development caveat" for how that row differs from the
built app's.

## Files to check

- `packages/cli/src/permissions.ts` — reads `chain.permissions`, writes/removes the two plists.
- `packages/cli/src/dev.ts`, `build.ts` — call it before spawning Tauri; build passes the entitlements.
- `apps/playground/src-tauri/Info.plist`, `Entitlements.plist` — the playground's hand-written equivalent.
- `research/MACOS.md` — wry's grant, TCC attribution, the verification run.
