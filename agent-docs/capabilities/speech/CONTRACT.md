# Speech Capability — Contract

## What this is

On-device transcription of a recording the app already stored with
`desktop.files`, audio or video. Requested by mneme (request 19, Phase 17
"Speech-to-Text"; video files and Opus in request 39, Phase 42 "Import
Anything"). **Audio never leaves the machine** — there is no
server fallback, ever, including when the on-device path is unavailable.

## `desktop.speech.transcribe(reference, options?, onProgress?)`

```
transcribe(
  reference: string,                        // a desktop.files reference
  options?: { locale?: string },            // BCP-47; default = system locale
  onProgress?: (fraction: number) => void,  // 0–1, best effort
): Promise<{ text; segments: { startMs; endMs; text }[]; locale }>
```

- Input: an audio file, or a video file's first sound track — with every
  engine, at least:

  | Container | Sound |
  | --- | --- |
  | MP4, MOV, M4V (video), m4a | AAC |
  | WebM, Matroska (`.mkv`), audio or video | Opus, Vorbis |
  | Ogg (`.opus`, `.ogg`) | Opus, Vorbis |
  | mp3, wav, flac, caf, aiff | their usual codecs |

  `MediaRecorder`'s `audio/mp4` AAC (request 18) and its WebM/Opus both
  work. The OS engine also takes anything else the OS can decode. The
  picture is ignored; only the sound is read. A reference, not bytes: an
  hour of audio shouldn't cross IPC.
- Handles long recordings. An hour of speech takes about a minute on
  Apple Silicon, and timestamps are continuous from the start of the file.
- `text` — the whole transcript, punctuated, sentences joined by spaces.
- `segments` — phrase-sized pieces in order: a new one starts at a pause
  of 0.8 s or more, at a sentence end once the segment is 3 s long, and
  no segment runs past 15 s. `startMs`/`endMs` are offsets from the start
  of the file, video files included (a sound track that starts late keeps
  its offset).
- `locale` — the BCP-47 locale actually used (a requested `"en"` may come
  back as `"en-US"`).
- `onProgress` is called with growing fractions and a final `1`, over
  the length of the sound, for video files as for audio. Fractions are
  estimates; they may jump. A file that doesn't state its length
  (browser-recorded WebM often doesn't) is measured by how much of it
  has been read.
- Audio with no speech resolves `{ text: "", segments: [] }`.
- One transcription per app at a time.

The first use of a locale may download its on-device model (macOS 26),
which needs a network connection once; after that it's offline.

### `options.engine` — an installed model instead of the OS engine

```
engine: {
  modelId: string;                     // installed with desktop.models
  config: AsrModelConfig;              // family + relative file names
  vad: { modelId: string; model: string };  // Silero VAD, its own model
}
```

Runs sherpa-onnx (compiled into the app) over the file: decode its first
sound track (the containers and codecs in the table above), resample to
16 kHz, cut speech with the VAD, recognize each piece. Same `Transcript` shape;
`segments` are the speech pieces. Works on every OS the engine is built
for, including Windows. `cancel()` and the one-at-a-time rule apply
unchanged. `locale` picks the language for Whisper/SenseVoice (first
subtag); the returned `locale` is the one passed in, else the config's
`language`, else `"und"`. A missing model or file rejects `NOT_FOUND`; a
model whose files don't match its `type` rejects `NATIVE_FAILURE`.
Model families: `whisper`, `sense-voice`, `moonshine` (v1 or v2 layout),
`paraformer`, `transducer`, `nemo-ctc`.

## `desktop.speech.cancel()`

Stops the running transcription; its `transcribe` promise rejects
`CANCELLED`. Resolves immediately, and does nothing when none is running.

## `desktop.speech.locales()`

The BCP-47 locales `transcribe` accepts on this machine. On macOS 26
that's every locale with an on-device model, installed or downloadable
(30 on macOS 26.6). Before macOS 26, only locales whose model is already
installed.

## Errors (`ChainErrorCode`)

- `NOT_FOUND` — `reference` isn't a stored file, or the `engine`'s model
  or one of its files isn't installed, or **the file has no sound track**
  (a video without sound; the message says "the file has no sound track").
  This is how an app tells "nothing to transcribe" from an unreadable file,
  which is `NATIVE_FAILURE`.
- `INVALID_ARGUMENT` — an `engine` file name that isn't a plain relative path.
- `UNAVAILABLE` — another transcription is running.
- `UNSUPPORTED` — no on-device model exists for the locale (the message
  names it), or the platform has no built-in engine and no `engine` was
  given (Windows/Linux), or the call ran outside a Chain app.
- `PERMISSION_DENIED` — before macOS 26 only: the user refused speech
  recognition, or Siri & Dictation is off (the message says which setting).
- `CANCELLED` — after `cancel()`.
- `NATIVE_FAILURE` — anything else, such as a file that isn't audio or
  video, or a sound track in a codec neither the OS nor Chain decodes.

## Declaring speech recognition

macOS 26's SpeechAnalyzer needs no permission prompt. The fallback for
older macOS does, so apps that support it declare it next to
`microphone`:

```json
"chain": { "permissions": { "speechRecognition": "Mneme transcribes your recordings on this Mac." } }
```

That emits `NSSpeechRecognitionUsageDescription` (see
`agent-docs/framework/command/README.md`, "App permissions").

## Non-goals

- No live microphone transcription. That's a different, streaming API;
  add it when an app needs it.
- No speaker labels, word-level timings, alternatives or confidence.
- No cloud recognition, ever.
- No translation or summarizing — apps do that on the text.
- No extracting a video's sound to its own file, and no reading text
  shown in the picture (that's `desktop.vision` on a frame the app grabs).
