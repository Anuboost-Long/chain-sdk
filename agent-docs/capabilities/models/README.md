# Models capability

## How it works

`desktop.models.install(manifest)` invokes `models_install`
(`templates/lib.rs`), which lazily opens `chain_core::models::Models` on
`<app data>/models/`, takes a per-id ticket from `Installs` (one install
per id; `cancel` flips its flag), then:

1. `Models::download` streams the URL with reqwest into
   `models/.partial/`, hashing as it writes and emitting
   `chain://models-progress` events. A SHA-256 mismatch deletes the file.
2. `Models::install` (blocking thread) unpacks into a staging folder,
   allowing only regular files and folders with safe relative paths,
   lifts a single top folder, swaps the result into `models/<id>/`
   (the old copy goes through `.trash/`), and writes
   `models/.meta/<id>.json`.

Engines resolve files with `Models::resolve(id, relative)`; paths never
reach JS. The ASR engine lives in `crates/core/src/sherpa/` — see
`agent-docs/capabilities/speech/README.md`.

## How to use it

```ts
const vad = { id: "silero-vad", url: ".../silero_vad.onnx", sha256: "9e24…1fd6" };
await desktop.models.install(vad);
await desktop.models.install(moonshine, (received, total) => setProgress(received / (total ?? 1)));

await desktop.speech.transcribe(recording, {
  engine: {
    modelId: "moonshine-tiny-en",
    config: {
      type: "moonshine",
      encoder: "encoder_model.ort",
      mergedDecoder: "decoder_model_merged.ort",
      tokens: "tokens.txt"
    },
    vad: { modelId: "silero-vad", model: "silero_vad.onnx" }
  }
});

await desktop.models.list();
await desktop.models.remove("moonshine-tiny-en");
```

## Files to check

- `capabilities/models/contract.ts` — TS types.
- `crates/core/src/models.rs` — download, hashing, safe extraction, list/remove/resolve (+ tests incl. hostile archives).
- `crates/core/build.rs` — fetches and links the no-TTS sherpa-onnx libs (pinned SHA-256).
- `crates/core/src/sherpa/` — the ASR engine (`ffi.rs` bindings, `mod.rs` pipeline).
- `packages/cli/templates/lib.rs` — `models_*` commands and `speech_transcribe`'s `engine`.
- `packages/sdk/src/models.ts` — SDK wrapper.
- `research/LICENSING.md` — why no-TTS, what's linked, TTS options.
