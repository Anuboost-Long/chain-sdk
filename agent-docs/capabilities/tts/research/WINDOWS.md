# Tts — Windows research

## `compile`'s AAC encoder (not implemented)

`crates/core/src/m4a.rs`'s `M4aWriter::create` returns `Unsupported` on
Windows, so `compile` rejects `UNSUPPORTED` there. The plan: Media
Foundation's sink writer (`MFCreateSinkWriterFromURL` to a `.m4a`,
output `MFAudioFormat_AAC` 1 ch at the voice's rate, input
`MFAudioFormat_Float`, `WriteSample` per segment, `Finalize`). The AAC
encoder MFT only takes 44.1/48 kHz input, so Kokoro's 24 kHz and Piper's
16/22.05 kHz would need resampling first (the Resampler DSP MFT, or a
Rust resampler). Check that WebView2's `<audio>` trims the priming
frames the way WebKit does before trusting the timings. Unverified.
