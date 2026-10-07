# Embeddings — Windows research

**Not built or run on Windows yet.** Everything below is what should
hold, from the code and the archives; the checklist at the end is what
someone with a Windows machine has to confirm before `component.json`
says anything but `unverified`.

## Same engine, same runtime

- sherpa-onnx's `win-x64-static-MD-Release-no-tts-lib` archive (build.rs)
  ships the same ONNX Runtime 1.28.2 as a static `onnxruntime.lib`, so
  `src/embeddings/ort.rs` links against it exactly as on macOS — no DLL,
  no DirectML, CPU only.
- `ORT_API_CALL` is `__stdcall`, which on x64 is the one Windows calling
  convention; ort.rs declares every pointer `extern "system"`, which is
  right on both.
- `ORTCHAR_T` is `wchar_t` on Windows, so `CreateSession` takes a UTF-16
  path: ort.rs encodes it with `encode_wide` under `cfg(windows)`. Model
  folders under `%APPDATA%` with non-ASCII user names need this.
- `tokenizers` with `fancy-regex` is pure Rust; its `esaxx-rs`
  dependency comes in without the `cpp` feature, so nothing is compiled
  for MSVC.
- Vectors should match macOS to float rounding: same runtime version, but
  MLAS picks different kernels per CPU (AVX2/AVX-512 vs NEON), so not
  bit-identical across machines. An index built on one machine and
  queried on another still works (cosine differences ~1e-6).

## Checklist

- [ ] `cargo test -p chain-core --lib embeddings` with
      `CHAIN_TEST_EMBEDDING_MODELS` (reference vectors from
      `research/reference.py`).
- [ ] A model installed under a path with non-ASCII characters loads.
- [ ] Cancel mid-batch rejects `CANCELLED` promptly.
- [ ] Load time and memory for multilingual-e5-small.
