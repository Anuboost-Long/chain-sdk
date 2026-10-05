# Embeddings Capability — Contract

## What this is

On-device sentence embeddings: text in, one vector per text out, from a
BERT-style ONNX model and its Hugging Face tokenizer that the user
downloaded through `desktop.models`. Requested by mneme (request 37) for
search by meaning: notes and course material are embedded on the device
and never sent anywhere.

Runs on the ONNX Runtime chain-core already links (through sherpa-onnx),
so there's nothing extra to ship and no license beyond what's there
(ONNX Runtime MIT, `tokenizers` Apache-2.0). The models mneme lists,
`intfloat/multilingual-e5-small` and `BAAI/bge-small-en-v1.5`, are MIT.

## Installing a model

Through `desktop.models.install`, from the app's own catalog. A model is
an ONNX file plus a `tokenizer.json`. Hugging Face hosts them as separate
files, so a catalog can install them as two models (`archive: "none"`
stores each under its URL's file name) and point at the tokenizer with
`tokenizerModelId`:

```ts
await desktop.models.install({ id: "bge-small-en", url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx", sha256: "828e14…" });
await desktop.models.install({ id: "bge-small-en-tokenizer", url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json", sha256: "d241a6…" });

const model = {
  modelId: "bge-small-en",
  config: { model: "model.onnx", tokenizer: "tokenizer.json", tokenizerModelId: "bge-small-en-tokenizer", pooling: "cls" }
};
```

Or one archive holding both, with no `tokenizerModelId`.

## `EmbeddingModelConfig`

From the app's catalog, per model:

| Field | Default | Meaning |
|---|---|---|
| `model` | required | The ONNX file's name inside the installed model. |
| `tokenizer` | required | The Hugging Face `tokenizer.json`'s name. |
| `tokenizerModelId` | the model's own id | The installed model holding `tokenizer`. |
| `pooling` | `"mean"` | `"mean"` averages the token vectors (e5, MiniLM), `"cls"` takes the first token's (bge). A model whose output is already one vector per text is used as is, and `pooling` doesn't apply. |
| `normalize` | `true` | Scale each vector to length 1, so cosine similarity is a dot product. |
| `maxTokens` | the model's own limit | Texts longer than this are cut. The model's limit is the tokenizer's own truncation length, else `model_max_length` from a `tokenizer_config.json` next to it, else 512. A value above what the model was trained on fails in the model. |
| `queryPrefix` / `passagePrefix` | none | Put in front of every query / passage. e5 models need `"query: "` and `"passage: "`. |

What's a model's right pooling and prefixes is the catalog's knowledge
(the model card says); Chain doesn't guess them from the files.

## `desktop.embeddings.embed(texts, options)`

```
embed(texts: string[], { modelId, config, as: "query" | "passage", batchSize?, signal? })
  → { dimension, maxTokens, vectors: Float32Array[], texts: { tokens, truncated }[] }
```

- `as` picks the prefix: `queryPrefix` for `"query"`, `passagePrefix`
  for `"passage"`.
- One vector per text, in order, each `dimension` long (384 for both of
  mneme's models) and its own `Float32Array` with its own buffer, so
  `new Uint8Array(vector.buffer)` is exactly that vector's bytes for
  storing. They cross from native as raw bytes, not JSON.
- `texts[i].tokens` — tokens in text `i` with its prefix and the model's
  special tokens (`[CLS]`, `[SEP]`, `<s>`, `</s>`): what counts against
  `maxTokens`. `truncated` — it was more than `maxTokens`, so only the
  first `maxTokens` were embedded. Truncation is Hugging Face's: the
  content is cut and the closing special token kept.
- `maxTokens` — the limit texts were cut to.
- An empty string embeds (as just the special tokens); an empty `texts`
  resolves with no vectors and `dimension` 0.
- `batchSize` — texts run through the model this many at a time, 1–256
  (default 16); outside that range rejects `INVALID_ARGUMENT`. Each batch
  is padded to its longest text, so similar lengths batch best. A big
  call isn't one huge run: memory is per batch.
- Runs off the UI thread, on up to 4 cores. The first call loads the
  model: under a second for either of mneme's models in a release build
  on an M3 Pro (a debug `chain dev` build took 6 s for
  multilingual-e5-small). After that a short query takes about 40 ms. It
  stays loaded until `unload()` or another model is used; one model is
  loaded at a time.
- Several calls may run at once (a query typed while a library is being
  indexed); they share the model a batch at a time, so a query waits for
  at most the batch in progress.

**Cancelling.** Pass an `AbortSignal`; aborting rejects the call
`CANCELLED`, stopping the batch being run (ONNX Runtime's terminate
flag), so it doesn't wait for it to finish. Only that call stops. An
already-aborted signal rejects before anything runs.

**Deterministic.** The same texts, model, config and `batchSize` give
bit-identical vectors on the same machine. A text batched with longer
texts is padded, which changes float rounding in the last bits (cosine
similarity to itself above 0.99999), never the meaning. Apps deciding
what to re-embed should compare the text, not the vector.

## `desktop.embeddings.countTokens(texts, options)`

```
countTokens(texts: string[], { modelId, config, as? }) → { tokens: number[], maxTokens }
```

The model's own tokenizer, counted exactly as `embed`'s `texts[i].tokens`
(with the prefix `as` picks; omitted, no prefix). For splitting a page
into passages that fit `maxTokens` before embedding. Loads only the
tokenizer, not the model, and runs off the UI thread.

## `desktop.embeddings.availability()`

`{ available, pooling }` — whether `embed` and `countTokens` work here,
and which pooling strategies are supported (`["mean", "cls"]`). Not
available outside a Chain app, or on a target chain-core links no ONNX
Runtime for (today: anything but macOS arm64/x64, Windows x64 and Linux
x64).

## `desktop.embeddings.unload()`

Frees the loaded model and tokenizer, after the batch in progress, and
resolves once they're freed; a no-op when nothing is loaded. The next
call loads it again.

Loaded models are big: measured as the process's physical footprint on
macOS, multilingual-e5-small adds about 1 GB (its 250,000-token
vocabulary; Python's onnxruntime shows the same, with every session
option tried) and bge-small-en-v1.5 about 200 MB. After `unload()` the
footprint went back below where it started. An app that's done indexing
should let the model go.

## Errors (`ChainErrorCode`)

| Code | When |
|---|---|
| `NOT_FOUND` | The model isn't installed (`model … isn't installed`), or its ONNX file or the tokenizer isn't in it (`model … has no file "tokenizer.json"`). |
| `INVALID_ARGUMENT` | The files aren't a sentence-embedding model (not ONNX, an input other than `input_ids`/`attention_mask`/`token_type_ids`/`position_ids`, no `input_ids`, a non-float32 output) or the tokenizer isn't a Hugging Face `tokenizer.json`; `batchSize` out of range; `maxTokens` 0. |
| `TOO_LARGE` | Out of memory running a batch; the message suggests a smaller `batchSize`. |
| `CANCELLED` | The call's `signal` was aborted. |
| `UNSUPPORTED` | Outside a Chain app, or no ONNX Runtime on this target. |
| `NATIVE_FAILURE` | Anything else from ONNX Runtime or the tokenizer, with its message. |

## Non-goals

- **Storing or searching vectors.** The app keeps them (mneme: its own
  SQLite table) and ranks them itself (cosine in JS); a student's
  library is thousands of passages, not millions.
- **Downloading models.** Only through `desktop.models`; catalogs stay
  in the app.
- **Splitting text into passages.** The app decides what a passage is;
  `countTokens` tells it what fits.
- **Generation, reranking, cross-encoders, image or audio embeddings.**
  Sentence embeddings only.
- **GPU execution providers.** CPU only, the same everywhere.
- **Choosing pooling or prefixes from the model files.** The catalog says.
