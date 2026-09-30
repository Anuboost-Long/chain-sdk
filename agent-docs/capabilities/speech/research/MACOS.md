# Speech — macOS research

## Two engines

|                        | SpeechAnalyzer + SpeechTranscriber (macOS 26)            | SFSpeechRecognizer (10.15+)                        |
| ---------------------- | -------------------------------------------------------- | -------------------------------------------------- |
| Reachable from         | Swift only (async/await API)                             | ObjC → objc2                                       |
| Permission             | none observed (authorizationStatus stayed notDetermined) | `NSSpeechRecognitionUsageDescription` + TCC prompt |
| Siri & Dictation off   | works                                                    | fails: "Siri and Dictation are disabled"           |
| Models                 | `AssetInventory.assetInstallationRequest` downloads      | only what Dictation installed                      |
| On-device locales here | 30                                                       | 4 (en-ID, en-PH, en-SA, en-US)                     |
| Long form              | designed for it                                          | URL requests historically capped; not tested       |

Chosen: SpeechAnalyzer when `SpeechTranscriber.isAvailable` (macOS 26+),
else SFSpeechRecognizer.

## SpeechAnalyzer details

- `SpeechTranscriber(locale:, transcriptionOptions: [], reportingOptions: [],
attributeOptions: [.audioTimeRange])` — finalized results only, each an
  `AttributedString` whose runs carry `audioTimeRange` per token.
- `SpeechTranscriber.supportedLocale(equivalentTo:)` maps `en` → `en-US`
  and returns nil for unknown locales (→ `UNSUPPORTED`).
- `analyzer.analyzeSequence(from: AVAudioFile)` then
  `finalizeAndFinish(through:)`; results stream through
  `transcriber.results`. Cancel: cancel the Swift `Task`, then
  `cancelAndFinishNow()`.
- Results are sentence-sized, already punctuated and capitalized.

## Building Swift into chain-core

`build.rs` runs `swiftc -emit-library -static -parse-as-library
-swift-version 5 -target <arch>-apple-macos12.0`, then links the
archive, the Swift runtime search paths from `swiftc -print-target-info`,
and the Foundation/AVFoundation/Speech frameworks. Code using macOS 26
APIs sits behind `#if compiler(>=6.2)` and `#available(macOS 26, *)`, so
an older Xcode still builds and uses the fallback.

**Swift Concurrency and `@rpath`:** the SDK's `libswift_Concurrency.tbd`
carries `$ld$previous$@rpath/...$$1$10.9$12.0$$`, so any final binary
whose deployment target is below 12.0 gets `@rpath/libswift_Concurrency.dylib`
— and Rust's aarch64 default is 11.0, and `tauri dev` sets no target. The
binary then fails at launch with `dyld: Library not loaded:
@rpath/libswift_Concurrency.dylib`. The fix is in the app: the template
`build.rs` adds `-rpath /usr/lib/swift`, and `tauri.conf.json` sets
`minimumSystemVersion: 12.0` (the dylib only exists in the OS from 12).
(`cargo test` hides this: Cargo adds link search paths to
`DYLD_FALLBACK_LIBRARY_PATH`.)

## Verification run (2026-09-28, macOS 26.6.2, Apple Silicon)

Audio made with `say` (AAC .m4a): an 11.6 s three-sentence lecture and a
59.9 min lecture (83 repeated sections).

- Short: exact text including "Rubisco", segments 0–4680, 4740–7800,
  7860–11610 ms; ~0.8 s.
- Long: 51 s, 746 segments, last ends at 3591540 ms, 60010 characters,
  progress evenly paced 0.1 every ~5 s.
- Second call during a run: `Unavailable`. Cancel after 1.5 s / 5 s:
  `Cancelled`. `tlh-XX`: `Unsupported`.
- The real `MediaRecorder` recording from the microphone verification
  (request 18) transcribed.
- Legacy path (before switching): TCC prompt for speech recognition, then
  "Siri and Dictation are disabled" on this Mac.
