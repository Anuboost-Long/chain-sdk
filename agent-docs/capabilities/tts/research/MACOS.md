# Tts — macOS research

## Compiling one file (`compile`, audiobook mode)

- **Timings.** sherpa-onnx's `SherpaOnnxGeneratedAudio` is
  `{ samples, n, sample_rate }` only — no word or phoneme timestamps. So
  `compile` synthesizes each app-given segment separately and times it by
  its sample count. Word timings would need forced alignment (run an ASR
  model with token timestamps over the output), a separate, heavier job;
  not done until an app needs it.
- **Encoder.** AudioToolbox's `ExtAudioFile` (plain C, part of the OS, no
  binary size): `ExtAudioFileCreateWithURL(kAudioFileM4AType, AAC ASBD)`,
  client format float32 mono, `ExtAudioFileWrite` per segment,
  `ExtAudioFileDispose` finalizes. Bit rate is set via the file's
  `AudioConverter` (`kAudioConverterEncodeBitRate`, then a null
  `kExtAudioFileProperty_ConverterConfig` to apply) — best effort.
- **Priming.** AAC adds 2112 priming frames; the m4a records them
  (`afinfo`: "286973 valid frames + 2112 priming + 707 remainder") and
  WebKit trims them, so `audio.duration` equals the computed duration
  and segment timings line up. Verified 2026-09-29.
- **Size.** 64 kbps mono ≈ 0.5 MB/min vs 2.9 MB/min for 24 kHz WAV.
