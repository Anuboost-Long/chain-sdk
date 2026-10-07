# Speech — Windows research (not implemented, by decision)

`transcribe` rejects `UNSUPPORTED` on Windows. Decided 2026-09-28 to wait
until mneme ships on Windows and someone can test there, rather than write
a large COM integration nobody can run.

## Why it isn't a small job

WinRT's `Windows.Media.SpeechRecognition.SpeechRecognizer` only listens to
a live microphone — there's no file or stream input — so the obvious OS API
can't transcribe a stored recording.

## Options, when this is picked up

1. **SAPI 5 desktop recognizer** (Win32 COM: `ISpRecognizer` in-proc +
   `ISpStream` over a PCM WAV, dictation grammar). Built in and on-device,
   reachable from the `windows` crate. The m4a/AAC recording has to be
   decoded first with Media Foundation (`IMFSourceReader` → PCM). Downsides:
   an old engine with clearly worse accuracy, no punctuation, and only the
   languages with an installed desktop recognizer.
2. **Bundled Whisper (whisper.cpp / `whisper-rs`)**: one engine on every OS,
   good accuracy and punctuation, fully local. Downsides: a 150–500 MB model
   to ship or download, heavy CPU use, and it isn't the OS engine — a product
   decision, not just a platform adapter.
3. **Windows AI APIs** (newer on-device speech on Copilot+ PCs): hardware-
   limited; check whether a file-capable API has shipped when this starts.

Whatever is chosen must keep the contract's rule: on-device only, never a
cloud service.
