# Speech capability

## How it works

`desktop.speech.transcribe(reference, options, onProgress)` generates a
call id, listens for `chain://speech-progress` events carrying it, then
invokes `speech_transcribe` (`templates/lib.rs`). That resolves the
reference to a path through `files.process_path` (`NOT_FOUND` if absent)
and runs `chain_core::speech::transcribe` on a blocking thread.

`crates/core/src/speech.rs` allows one run at a time, then:

- **macOS 26+**: calls the Swift bridge `crates/core/swift/ChainSpeech.swift`,
  compiled into a static library by `crates/core/build.rs`. It runs
  SpeechAnalyzer + SpeechTranscriber over the file (downloading the
  locale's model through `AssetInventory` if needed) and returns
  per-token timings as JSON through C callbacks.
- **Older macOS**: `SFSpeechRecognizer` with `requiresOnDeviceRecognition`
  through objc2 (needs authorization and Siri & Dictation on).

Both backends produce tokens with times, which `group_words` turns into
phrase segments.

The Swift bridge links Swift Concurrency, which is why apps get a
`build.rs` adding an `/usr/lib/swift` rpath and a macOS 12 minimum (both
from `chain update`).

## How to use it

```ts
const transcript = await desktop.speech.transcribe(recordingReference, { locale: "en-US" }, (f) =>
  setProgress(f)
);
// Cancel button:
await desktop.speech.cancel(); // the transcribe() above rejects CANCELLED
```

## Files to check

- `capabilities/speech/contract.ts` — TS types.
- `crates/core/src/speech.rs` — dispatch, one-at-a-time guard, cancel, legacy backend, segment grouping (+ tests).
- `crates/core/swift/ChainSpeech.swift` — the SpeechAnalyzer bridge (C ABI, JSON payloads).
- `crates/core/build.rs` — compiles the Swift file; link search order matters (see research/MACOS.md).
- `packages/cli/templates/lib.rs` — `speech_transcribe`/`speech_cancel`/`speech_locales`.
- `packages/cli/templates/build.rs` — the app's `/usr/lib/swift` rpath.
- `packages/sdk/src/speech.ts` — SDK wrapper, progress listener, error mapping.
