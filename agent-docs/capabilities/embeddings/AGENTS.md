# Embeddings Capability — Agent Memory

Scope: `desktop.embeddings.embed / countTokens / availability / unload`
over a BERT-style sentence-embedding ONNX model and its Hugging Face
`tokenizer.json`, both installed through `desktop.models`. Requested by
mneme (request 37) for search by meaning.

## What's already decided (2026-10-05)

- **No second ONNX Runtime.** `embeddings/ort.rs` calls the C API of the
  runtime inside sherpa-onnx's static archive by function-table index
  (research/MACOS.md has why not the `ort` crate). Only API v1 entries;
  keep it that way, and rerun the real-model test after any sherpa-onnx
  bump.
- **The catalog knows the model.** Pooling and prefixes come from the
  app's config, never guessed from the files.
- **Chain pads and truncates, not the tokenizer.** The tokenizer's own
  truncation/padding settings are turned off on load.
- **Per-call cancel through `AbortSignal`**, not a global `cancel()`
  like tts: a ⌘P query must not be cancelled by stopping a background
  index.
- **Vectors cross as raw bytes** (`Embedded::to_bytes`), never JSON.
- **Model locked per batch**, so concurrent calls interleave.
- No idle-unload timer (unlike tts): mneme didn't ask for one; `unload()`
  is enough to free it after indexing. Add `setIdleUnload` only on a real
  request, reusing tts's pattern.
- Out of scope, on purpose: vector storage/search, downloads outside
  `desktop.models`, splitting text, rerankers, GPU.

## Status

- [x] `cargo test -p chain-core --lib embeddings`: pooling, normalizing,
      byte layout, early cancel, non-ONNX file → invalid model.
- [x] Real models, macOS arm64 (M3 Pro), with
      `CHAIN_TEST_EMBEDDING_MODELS` and research/reference.py:
      bge-small-en-v1.5 (cls) and multilingual-e5-small (mean, prefixes)
      match Python onnxruntime to cosine > 0.9999, token counts exact
      (incl. a truncated 600+-token text, Chinese, French, empty string),
      repeat calls bit-identical.
- [x] End to end in `apps/playground` (dev build, inspector bridge), both
      models installed from Hugging Face as model + tokenizer installs
      (`tokenizerModelId`): 384-dim normalized `Float32Array`s each with
      its own buffer; NOT_FOUND for a missing model and tokenizer;
      INVALID_ARGUMENT for a speech model (`input_values`), a non-ONNX
      file, a non-JSON tokenizer, `batchSize` 0, `maxTokens` 0;
      CANCELLED pre-aborted and mid-run (512 long passages stopped ~260 ms
      after abort); a query during that run answered in 112 ms; unload
      then embed reloads. Footprint: e5 237 MB → 1.2 GB loaded → 108 MB
      after unload.
- [x] mneme's own window (2026-10-05, reported by the mneme session,
      debug `chain dev` build): bge-small-en-v1.5 installed as model +
      tokenizer, 37 real pages indexed as passages (batchSize 8, ~2.5 min
      in debug), then `unload()`; ⌘P "why do people hack" ranked the
      cyber threats chapter first. e5 not tried there.
- [ ] Windows: never built — research/WINDOWS.md checklist.
- [ ] Linux: the archive exists and the code is the same; not a target.
- [ ] Out-of-memory → `TOO_LARGE`: mapped from ONNX Runtime's message,
      never actually triggered.
