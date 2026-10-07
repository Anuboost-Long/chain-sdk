# Embeddings capability

## How it works

`desktop.embeddings.embed` → `embeddings_embed` (`templates/lib.rs`)
resolves the config's ONNX file and tokenizer inside their installed
models (`Models::resolve`; the tokenizer from `tokenizerModelId` when
it's a separate download), then on a blocking thread runs
`chain_core::embeddings::embed`:

1. The tokenizer (`tokenizers` crate, cached in `TOKENIZER` by path, its
   own truncation and padding off) encodes a batch of prefixed texts.
   Chain truncates to `maxTokens` itself, keeping the trailing special
   tokens.
2. The batch is padded to its longest text and fed to the model through
   `embeddings/ort.rs` — a thin binding to the ONNX Runtime C API that
   sherpa-onnx's static archive already contains (1.28.2), looked up by
   index in the `OrtApi` function table. The session (cached in `MODEL`)
   is held only for one batch, so concurrent calls interleave per batch.
3. `last_hidden_state` is mean- or CLS-pooled over the real tokens (a
   model that outputs pooled vectors is used as is), then L2-normalized.
4. `Embedded::to_bytes` packs everything into one little-endian buffer
   sent as a raw `tauri::ipc::Response`; `packages/sdk/src/embeddings.ts`
   unpacks it into a `Float32Array` per text.

Cancelling: the SDK gives each call a random id; aborting its
`AbortSignal` sends `embeddings_cancel(id)`, which sets that call's flag
and `RunOptionsSetTerminate` on its run options, so the batch in progress
stops. A cancel that arrives before its embed has registered leaves a
marker the embed finds.

`countTokens` loads only the tokenizer. `unload` drops `MODEL` (after the
batch holding it) and `TOKENIZER`. On a target without sherpa-onnx's
archive (`chain_no_sherpa`) `embed` rejects `UNSUPPORTED`.

## How to use it

```ts
// The app's catalog: two installs per model, as Hugging Face hosts them.
const e5 = {
  modelId: "multilingual-e5-small",
  config: {
    model: "model.onnx",
    tokenizer: "tokenizer.json",
    tokenizerModelId: "multilingual-e5-small-tokenizer",
    pooling: "mean",
    queryPrefix: "query: ",
    passagePrefix: "passage: "
  }
} as const;

// Fit passages to the model before embedding them.
const { tokens, maxTokens } = await desktop.embeddings.countTokens(paragraphs, { ...e5, as: "passage" });

const indexing = new AbortController();
const { vectors } = await desktop.embeddings.embed(passages, {
  ...e5,
  as: "passage",
  batchSize: 32,
  signal: indexing.signal // indexing.abort() when the user switches model
});
// Store new Uint8Array(vectors[i].buffer) with the passage.

const [query] = (await desktop.embeddings.embed([typed], { ...e5, as: "query" })).vectors;
// Normalized, so cosine similarity is a dot product.

await desktop.embeddings.unload(); // when indexing is done
```

## Files to check

- `capabilities/embeddings/contract.ts` — the TypeScript contract.
- `agent-docs/capabilities/embeddings/CONTRACT.md` — semantics, errors, non-goals.
- `crates/core/src/embeddings/mod.rs` — tokenizing, batching, pooling,
  cancel registry, the byte layout, and the tests (the real-model one
  runs when `CHAIN_TEST_EMBEDDING_MODELS` is set).
- `crates/core/src/embeddings/ort.rs` — the ONNX Runtime C API binding;
  the function-table indices are the thing to recheck if ORT is ever
  replaced by something other than sherpa-onnx's archive.
- `packages/cli/templates/lib.rs` (and `apps/playground/src-tauri/src/lib.rs`)
  — `embeddings_*` commands and their error prefixes.
- `packages/sdk/src/embeddings.ts` — the SDK wrapper and byte parser.
- `research/reference.py` — regenerates the reference vectors the Rust
  test compares against.
