# Embeddings — macOS research

No native API involved: the engine is portable Rust plus the ONNX Runtime
chain-core already links, so this file is about that runtime and the
models, not about macOS frameworks.

## Why not Apple's NLEmbedding / NLContextualEmbedding

`NaturalLanguage` has sentence embeddings (`NLEmbedding.sentenceEmbedding`,
macOS 11+) and `NLContextualEmbedding` (macOS 14+). Rejected: the vectors
differ per OS and OS version (an index built on one machine can't be
compared with a query on another, and an OS update silently changes them),
there's no Windows equivalent (rule 3), and the app can't pick the model
mneme's catalog names. The ONNX path gives the same vectors everywhere.

## Which ONNX Runtime

sherpa-onnx's static archives (build.rs, v1.13.8) contain
`libonnxruntime.a`, **ONNX Runtime 1.28.2**, API versions 1–28 (checked
by calling `OrtGetApiBase()->GetVersionString()` and probing `GetApi(n)`
against the archive). It exports the C API (`OrtGetApiBase`), so
`src/embeddings/ort.rs` calls it directly:

- No `ort` crate: it would link or download a second ONNX Runtime
  (duplicate symbols against the static one, or a second ~20 MB copy),
  and its versions pin a specific runtime release.
- The C API is a struct of function pointers (`struct OrtApi` in
  `onnxruntime_c_api.h`). Only entries from API version 1 are used, at
  fixed indices (computed with `offsetof` against the v1.28.2 header);
  the struct is append-only, so a sherpa-onnx bump can't move them. If a
  future ORT ever breaks that promise, `matches_reference_vectors_from_real_models`
  fails immediately.
- One `OrtEnv` per process; ONNX Runtime returns the same instance to
  sherpa-onnx's sessions anyway.
- `RunOptionsSetTerminate` is the cancel path: it's documented as
  callable from another thread and makes the `Run` in progress return an
  error ("Exiting due to terminate flag being set to true").
- Session options: intra-op threads = min(cores, 4) (the same as TTS, so a
  background index doesn't take every core from the UI), graph
  optimization `ORT_ENABLE_ALL`.

## Tokenizer

The `tokenizers` crate (Hugging Face's own, Apache-2.0) 0.23 with
`default-features = false, features = ["fancy-regex"]`: pure Rust (the
default `onig` builds the Oniguruma C library), no progress bars, no hub
downloads. It reads any `tokenizer.json`: WordPiece (bge, MiniLM) and
SentencePiece Unigram (e5/XLM-R) were both checked.

Chain turns off the tokenizer's own truncation and padding and does both
itself: each batch is padded to its longest text with the tokenizer's
pad id (`[PAD]` = 0 for BERT, `<pad>` = 1 for XLM-R — XLM-R derives
position ids from it, so the real pad id matters), and truncation keeps
the trailing special tokens, as Hugging Face does for a single sequence.
Verified equal to Python `tokenizers`' `enable_truncation(512)` for a
600+-token text in both models (`research/reference.py`).

## Models checked (MIT)

| Model | ONNX | Inputs | Output | Pooling | Prefixes |
|---|---|---|---|---|---|
| `BAAI/bge-small-en-v1.5` `onnx/model.onnx` | 133 MB | input_ids, attention_mask, token_type_ids (int64) | last_hidden_state [b, s, 384] | cls | none (an optional query instruction exists; not needed for short queries) |
| `intfloat/multilingual-e5-small` `onnx/model.onnx` | 470 MB | input_ids, attention_mask, token_type_ids (int64) | last_hidden_state [b, s, 384] | mean | `"query: "`, `"passage: "` |

Both have a `tokenizer_config.json` with `model_max_length: 512` and no
truncation in `tokenizer.json`, so without the config file next to the
tokenizer Chain falls back to 512 — the same number.

## Verification on macOS arm64 (M3 Pro, 2026-10-05)

`CHAIN_TEST_EMBEDDING_MODELS=<dir> cargo test -p chain-core --lib embeddings`
against vectors from `research/reference.py` (Python onnxruntime 1.30 +
tokenizers, each text embedded alone with no padding):

- Every vector's cosine with the reference > 0.9999 for both models,
  batched 3 at a time with padding, including Chinese, French, an empty
  string and a truncated 600+-token text.
- Token counts equal Python's exactly.
- The same call twice gives bit-identical vectors. The same text alone vs
  padded in a batch: bge was bit-identical; e5 differed in the 7th
  decimal (cosine > 0.99999) — float rounding from the longer padded
  attention, documented in CONTRACT.md.
