//! Speech capability — see /agent-docs/capabilities/speech/CONTRACT.md.
//! On-device only — audio never leaves the machine. macOS 26+ uses
//! SpeechAnalyzer through the Swift bridge in swift/ChainSpeech.swift
//! (built by build.rs); older macOS falls back to `SFSpeechRecognizer`
//! with `requiresOnDeviceRecognition`, through objc2. Files AVFoundation
//! can't open (WebM, Opus) reach either one as samples decoded by
//! crate::sound. Every other platform is `Unsupported` for now.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use crate::sound::Sound;

#[derive(Debug)]
pub enum SpeechError {
    /// No on-device model for the locale, or no recognizer on this platform.
    Unsupported(String),
    /// Another transcription is already running.
    Unavailable(String),
    /// The file has no sound track.
    NotFound(String),
    PermissionDenied(String),
    Cancelled,
    Other(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
    pub locale: String,
}

/// One recognized word (or punctuation-carrying token) with its timing, in seconds.
#[derive(Debug, Clone, Deserialize)]
struct Word {
    text: String,
    start: f64,
    end: f64,
}

/// A pause at least this long starts a new segment.
const SEGMENT_PAUSE_SECONDS: f64 = 0.8;
/// A sentence end starts a new segment once the current one is this long.
const SEGMENT_MIN_SENTENCE_SECONDS: f64 = 3.0;
/// No segment runs longer than this, sentence end or not.
const SEGMENT_MAX_SECONDS: f64 = 15.0;

/// Groups words into phrase-sized segments: split at a pause, at a
/// sentence end once the segment has some length, or at a length cap.
fn group_words(words: &[Word]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current: Vec<&Word> = Vec::new();
    for word in words {
        if let (Some(first), Some(last)) = (current.first(), current.last()) {
            let length = last.end - first.start;
            let ends_sentence = last.text.ends_with(['.', '?', '!']);
            if word.start - last.end >= SEGMENT_PAUSE_SECONDS
                || (ends_sentence && length >= SEGMENT_MIN_SENTENCE_SECONDS)
                || word.end - first.start > SEGMENT_MAX_SECONDS
            {
                segments.push(segment_of(&current));
                current.clear();
            }
        }
        current.push(word);
    }
    if !current.is_empty() {
        segments.push(segment_of(&current));
    }
    segments
}

fn segment_of(words: &[&Word]) -> Segment {
    Segment {
        start_ms: (words[0].start * 1000.0).round() as u64,
        end_ms: (words[words.len() - 1].end * 1000.0).round() as u64,
        text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
    }
}

/// `en_US` (NSLocale) → `en-US` (BCP-47), and back.
fn to_bcp47(identifier: &str) -> String {
    identifier.replace('_', "-")
}

fn to_locale_identifier(tag: &str) -> String {
    tag.replace('-', "_")
}

/// One transcription at a time per app (see CONTRACT.md).
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Set by `cancel()`; the running transcription checks it when its task ends.
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

struct RunningGuard;

impl RunningGuard {
    fn acquire() -> Result<Self, SpeechError> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err(SpeechError::Unavailable("a transcription is already running".to_string()));
        }
        CANCEL_REQUESTED.store(false, Ordering::SeqCst);
        Ok(Self)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// SpeechAnalyzer (macOS 26+) through swift/ChainSpeech.swift's C functions.
#[cfg(target_os = "macos")]
mod analyzer {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::sync::mpsc;

    use super::*;

    // Finish status codes — keep in sync with ChainSpeech.swift.
    const STATUS_OK: i32 = 0;
    const STATUS_UNSUPPORTED: i32 = 1;
    const STATUS_CANCELLED: i32 = 2;

    type Progress = extern "C" fn(*mut c_void, f64);
    type Finish = extern "C" fn(*mut c_void, i32, *const c_char);
    type Read = extern "C" fn(*mut c_void, *mut f32, isize) -> isize;

    extern "C" {
        fn chain_speech_analyzer_available() -> bool;
        fn chain_speech_analyzer_cancel();
        fn chain_speech_analyzer_locales(context: *mut c_void, on_finish: Finish);
        fn chain_speech_analyzer_transcribe(
            path: *const c_char,
            locale: *const c_char,
            context: *mut c_void,
            on_progress: Progress,
            on_finish: Finish,
        );
        fn chain_speech_analyzer_transcribe_samples(
            sample_rate: f64,
            locale: *const c_char,
            reader: *mut c_void,
            read: Read,
            context: *mut c_void,
            on_progress: Progress,
            on_finish: Finish,
        );
    }

    enum Event {
        Progress(f64),
        Finished(i32, String),
    }

    /// `context` is a leaked `Sender<Event>`; Swift never calls back after `on_finish`.
    extern "C" fn forward_progress(context: *mut c_void, fraction: f64) {
        let sender = unsafe { &*(context as *const mpsc::Sender<Event>) };
        let _ = sender.send(Event::Progress(fraction));
    }

    extern "C" fn forward_finish(context: *mut c_void, status: i32, payload: *const c_char) {
        let sender = unsafe { &*(context as *const mpsc::Sender<Event>) };
        let payload = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
        let _ = sender.send(Event::Finished(status, payload));
    }

    /// Runs one bridge call and waits for its finish callback, forwarding progress.
    fn call(start: impl FnOnce(*mut c_void), mut progress: impl FnMut(f64)) -> (i32, String) {
        let (tx, rx) = mpsc::channel();
        let context = Box::into_raw(Box::new(tx));
        start(context.cast());
        let finished = loop {
            match rx.recv() {
                Ok(Event::Progress(fraction)) => progress(fraction),
                Ok(Event::Finished(status, payload)) => break (status, payload),
                Err(_) => unreachable!("the context keeps a sender alive"),
            }
        };
        drop(unsafe { Box::from_raw(context) });
        finished
    }

    fn to_error(status: i32, message: String) -> SpeechError {
        match status {
            STATUS_UNSUPPORTED => SpeechError::Unsupported(message),
            STATUS_CANCELLED => SpeechError::Cancelled,
            _ => SpeechError::Other(message),
        }
    }

    pub fn available() -> bool {
        unsafe { chain_speech_analyzer_available() }
    }

    pub fn cancel() {
        unsafe { chain_speech_analyzer_cancel() }
    }

    /// A cancel() that landed before Swift registered its task.
    fn cancel_if_requested() {
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            cancel();
        }
    }

    pub fn locales() -> Result<Vec<String>, SpeechError> {
        let (status, payload) = call(|context| unsafe { chain_speech_analyzer_locales(context, forward_finish) }, |_| {});
        if status != STATUS_OK {
            return Err(to_error(status, payload));
        }
        serde_json::from_str(&payload).map_err(|e| SpeechError::Other(e.to_string()))
    }

    #[derive(Deserialize)]
    struct Recognized {
        words: Vec<Word>,
        text: String,
        locale: String,
    }

    /// What Swift pulls samples from: the decoded sound, plus the call's
    /// event channel (`call`'s context) for reporting progress as it reads.
    struct Reader {
        sound: Sound,
        pending: Vec<f32>,
        events: *mut c_void,
        reported: f64,
        error: Option<SpeechError>,
    }

    extern "C" fn read_samples(reader: *mut c_void, out: *mut f32, capacity: isize) -> isize {
        let reader = unsafe { &mut *(reader as *mut Reader) };
        let out = unsafe { std::slice::from_raw_parts_mut(out, capacity.max(0) as usize) };
        while reader.pending.len() < out.len() && !CANCEL_REQUESTED.load(Ordering::SeqCst) {
            match reader.sound.next() {
                Ok(Some(chunk)) => reader.pending.extend(chunk),
                Ok(None) => break,
                Err(e) => {
                    reader.error = Some(e);
                    return -1;
                }
            }
        }
        let count = reader.pending.len().min(out.len());
        out[..count].copy_from_slice(&reader.pending[..count]);
        reader.pending.drain(..count);
        // Whole percents only, like the engine path.
        let fraction = (reader.sound.progress() * 100.0).floor() / 100.0;
        if fraction > reader.reported {
            reader.reported = fraction;
            forward_progress(reader.events, fraction.min(0.99));
        }
        count as isize
    }

    pub fn transcribe(input: Input, locale: Option<&str>, on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
        let locale = locale.map(CString::new).transpose().map_err(|e| SpeechError::Other(e.to_string()))?;
        let locale = locale.as_ref().map_or(std::ptr::null(), |l| l.as_ptr());
        let mut reader = None;
        let (status, payload) = match input {
            Input::File(audio) => {
                let path = CString::new(audio.to_string_lossy().as_bytes()).map_err(|e| SpeechError::Other(e.to_string()))?;
                call(
                    |context| unsafe {
                        chain_speech_analyzer_transcribe(path.as_ptr(), locale, context, forward_progress, forward_finish);
                        cancel_if_requested();
                    },
                    on_progress,
                )
            }
            Input::Decoded(sound) => {
                let rate = f64::from(sound.sample_rate);
                let reader = reader.insert(Box::new(Reader {
                    sound,
                    pending: Vec::new(),
                    events: std::ptr::null_mut(),
                    reported: 0.0,
                    error: None,
                }));
                call(
                    |context| unsafe {
                        reader.events = context;
                        let reader: *mut Reader = &mut **reader;
                        chain_speech_analyzer_transcribe_samples(
                            rate,
                            locale,
                            reader.cast(),
                            read_samples,
                            context,
                            forward_progress,
                            forward_finish,
                        );
                        cancel_if_requested();
                    },
                    on_progress,
                )
            }
        };
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            return Err(SpeechError::Cancelled);
        }
        if let Some(error) = reader.and_then(|r| r.error) {
            return Err(error);
        }
        if status != STATUS_OK {
            return Err(to_error(status, payload));
        }
        let recognized: Recognized = serde_json::from_str(&payload).map_err(|e| SpeechError::Other(e.to_string()))?;
        Ok(Transcript { text: recognized.text, segments: group_words(&recognized.words), locale: recognized.locale })
    }

}

/// `SFSpeechRecognizer` for macOS before 26. Needs Siri & Dictation on
/// and the app's speech-recognition authorization, unlike SpeechAnalyzer.
#[cfg(target_os = "macos")]
mod legacy {
    use std::sync::mpsc;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::{AllocAnyThread, Message};
    use objc2_avf_audio::{AVAudioFile, AVAudioFormat, AVAudioPCMBuffer};
    use objc2_foundation::{NSError, NSLocale, NSOperationQueue, NSString, NSURL};
    use objc2_speech::{
        SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionRequest, SFSpeechRecognitionResult,
        SFSpeechRecognitionTask, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
        SFSpeechURLRecognitionRequest,
    };

    use super::*;

    /// The running task, so `cancel()` can reach it from another thread.
    struct RunningTask(Retained<SFSpeechRecognitionTask>);
    // SFSpeechRecognitionTask is documented thread-safe for cancel().
    unsafe impl Send for RunningTask {}
    static TASK: Mutex<Option<RunningTask>> = Mutex::new(None);

    enum Event {
        Progress(Vec<Word>),
        Finished(Vec<Word>, String),
        Failed(String),
    }

    fn authorize() -> Result<(), SpeechError> {
        let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
        let status = if status == SFSpeechRecognizerAuthorizationStatus::NotDetermined {
            let (tx, rx) = mpsc::channel();
            let handler = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
                let _ = tx.send(status);
            });
            unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
            rx.recv().map_err(|_| SpeechError::Other("speech authorization never answered".to_string()))?
        } else {
            status
        };
        match status {
            SFSpeechRecognizerAuthorizationStatus::Authorized => Ok(()),
            SFSpeechRecognizerAuthorizationStatus::Restricted => Err(SpeechError::PermissionDenied(
                "speech recognition is restricted on this Mac".to_string(),
            )),
            _ => Err(SpeechError::PermissionDenied(
                "speech recognition is turned off for this app in System Settings → Privacy & Security → Speech Recognition"
                    .to_string(),
            )),
        }
    }

    fn recognizer_for(tag: Option<&str>) -> Result<Retained<SFSpeechRecognizer>, SpeechError> {
        let recognizer = match tag {
            Some(tag) => {
                let locale = NSLocale::initWithLocaleIdentifier(
                    NSLocale::alloc(),
                    &NSString::from_str(&to_locale_identifier(tag)),
                );
                unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &locale) }
            }
            None => unsafe { SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) },
        };
        let name = tag.map(str::to_string).unwrap_or_else(|| to_bcp47(&NSLocale::currentLocale().localeIdentifier().to_string()));
        let recognizer =
            recognizer.ok_or_else(|| SpeechError::Unsupported(format!("speech recognition doesn't support {name}")))?;
        if !unsafe { recognizer.supportsOnDeviceRecognition() } {
            return Err(SpeechError::Unsupported(format!(
                "no on-device speech model for {name} — download it by adding the language in System Settings → Keyboard → Dictation"
            )));
        }
        Ok(recognizer)
    }

    fn words_of(result: &SFSpeechRecognitionResult) -> (Vec<Word>, String) {
        let transcription = unsafe { result.bestTranscription() };
        let words = unsafe { transcription.segments() }
            .iter()
            .map(|segment| unsafe {
                let start = segment.timestamp();
                Word { text: segment.substring().to_string(), start, end: start + segment.duration() }
            })
            .collect();
        (words, unsafe { transcription.formattedString() }.to_string())
    }

    pub fn locales() -> Result<Vec<String>, SpeechError> {
        autoreleasepool(|_| {
            let mut tags: Vec<String> = unsafe { SFSpeechRecognizer::supportedLocales() }
                .iter()
                .filter(|locale| {
                    unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), locale) }
                        .is_some_and(|r| unsafe { r.supportsOnDeviceRecognition() })
                })
                .map(|locale| to_bcp47(&locale.localeIdentifier().to_string()))
                .collect();
            tags.sort();
            Ok(tags)
        })
    }

    pub fn cancel() {
        if let Some(task) = TASK.lock().expect("speech task mutex poisoned").as_ref() {
            unsafe { task.0.cancel() };
        }
    }

    /// The request and the audio's length in seconds.
    fn file_request(audio: &Path) -> Result<(Retained<SFSpeechRecognitionRequest>, f64), SpeechError> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&audio.to_string_lossy()));
        let file = unsafe { AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url) }
            .map_err(|e| SpeechError::Other(format!("couldn't read the audio: {}", e.localizedDescription())))?;
        let duration = unsafe { file.length() as f64 / file.processingFormat().sampleRate() };
        let request = unsafe { SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url) };
        Ok((Retained::into_super(request), duration))
    }

    /// Appends the whole decoded sound up front, in one-second buffers; the
    /// recognizer queues what it hasn't read yet.
    fn decoded_request(mut sound: Sound) -> Result<(Retained<SFSpeechRecognitionRequest>, f64), SpeechError> {
        let rate = sound.sample_rate;
        let format = unsafe { AVAudioFormat::initStandardFormatWithSampleRate_channels(AVAudioFormat::alloc(), f64::from(rate), 1) }
            .ok_or_else(|| SpeechError::Other(format!("can't recognize audio at {rate} Hz")))?;
        let request = unsafe { SFSpeechAudioBufferRecognitionRequest::init(SFSpeechAudioBufferRecognitionRequest::alloc()) };
        let append = |samples: &[f32]| -> Result<(), SpeechError> {
            let buffer = unsafe { AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &format, samples.len() as u32) }
                .ok_or_else(|| SpeechError::Other("couldn't allocate an audio buffer".to_string()))?;
            unsafe {
                std::ptr::copy_nonoverlapping(samples.as_ptr(), (*buffer.floatChannelData()).as_ptr(), samples.len());
                buffer.setFrameLength(samples.len() as u32);
                request.appendAudioPCMBuffer(&buffer);
            }
            Ok(())
        };
        let mut pending = Vec::new();
        let mut frames = 0;
        while let Some(chunk) = sound.next()? {
            frames += chunk.len();
            pending.extend(chunk);
            if pending.len() >= rate as usize {
                append(&pending)?;
                pending.clear();
            }
        }
        if !pending.is_empty() {
            append(&pending)?;
        }
        unsafe { request.endAudio() };
        Ok((Retained::into_super(request), frames as f64 / f64::from(rate)))
    }

    pub fn transcribe(input: Input, locale: Option<&str>, mut on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
        authorize()?;
        autoreleasepool(|_| {
            let recognizer = recognizer_for(locale)?;
            let tag = to_bcp47(&unsafe { recognizer.locale() }.localeIdentifier().to_string());
            // Results arrive on this queue, never the main thread (which the
            // app's event loop owns).
            unsafe { recognizer.setQueue(&NSOperationQueue::new()) };

            let (request, duration) = match input {
                Input::File(audio) => file_request(audio)?,
                Input::Decoded(sound) => decoded_request(sound)?,
            };
            unsafe {
                request.setRequiresOnDeviceRecognition(true);
                request.setAddsPunctuation(true);
                request.setShouldReportPartialResults(true);
            }

            let (tx, rx) = mpsc::channel();
            let handler = RcBlock::new(move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
                if let Some(result) = unsafe { result.as_ref() } {
                    let (words, text) = words_of(result);
                    let event = if unsafe { result.isFinal() } { Event::Finished(words, text) } else { Event::Progress(words) };
                    let _ = tx.send(event);
                } else if let Some(error) = unsafe { error.as_ref() } {
                    let _ = tx.send(Event::Failed(error.localizedDescription().to_string()));
                }
            });
            let task = unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler) };
            *TASK.lock().expect("speech task mutex poisoned") = Some(RunningTask(task.retain()));
            // A cancel() that landed before the task existed.
            if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                unsafe { task.cancel() };
            }

            let outcome = loop {
                match rx.recv() {
                    Ok(Event::Progress(words)) => {
                        if let (Some(last), true) = (words.last(), duration > 0.0) {
                            on_progress((last.end / duration).min(0.99));
                        }
                    }
                    Ok(Event::Finished(words, text)) => break Ok((words, text)),
                    Ok(Event::Failed(message)) => break Err(message),
                    Err(_) => break Err("speech recognition ended without a result".to_string()),
                }
            };
            *TASK.lock().expect("speech task mutex poisoned") = None;

            if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                return Err(SpeechError::Cancelled);
            }
            match outcome {
                Ok((words, text)) => {
                    on_progress(1.0);
                    Ok(Transcript { text, segments: group_words(&words), locale: tag })
                }
                // Audio with no speech ends in an error, not an empty result.
                Err(message) if message.contains("No speech detected") => {
                    on_progress(1.0);
                    Ok(Transcript { text: String::new(), segments: Vec::new(), locale: tag })
                }
                Err(message) if message.contains("Dictation are disabled") => Err(SpeechError::PermissionDenied(
                    "on-device speech recognition needs Dictation — turn it on in System Settings → Keyboard → Dictation"
                        .to_string(),
                )),
                Err(message) => Err(SpeechError::Other(message)),
            }
        })
    }
}

/// What the OS engine reads: a file AVFoundation opens itself, or sound
/// decoded by crate::sound when it can't (WebM, Opus) — or when the file
/// has no sound track, which `Sound::open` rejects as `NotFound`.
#[cfg(target_os = "macos")]
enum Input<'a> {
    File(&'a Path),
    Decoded(Sound),
}

#[cfg(target_os = "macos")]
impl<'a> Input<'a> {
    fn of(audio: &'a Path) -> Result<Self, SpeechError> {
        use objc2::AllocAnyThread;
        use objc2_avf_audio::AVAudioFile;
        use objc2_foundation::{NSString, NSURL};

        let readable = objc2::rc::autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&audio.to_string_lossy()));
            unsafe { AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url) }.is_ok_and(|file| unsafe { file.length() } > 0)
        });
        Ok(if readable { Input::File(audio) } else { Input::Decoded(Sound::open(audio)?) })
    }
}

#[cfg(target_os = "macos")]
pub fn transcribe(audio: &Path, locale: Option<&str>, on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
    let _running = RunningGuard::acquire()?;
    let input = Input::of(audio)?;
    if analyzer::available() {
        analyzer::transcribe(input, locale, on_progress)
    } else {
        legacy::transcribe(input, locale, on_progress)
    }
}

#[cfg(target_os = "macos")]
pub fn cancel() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    if analyzer::available() {
        analyzer::cancel();
    } else {
        legacy::cancel();
    }
}

#[cfg(target_os = "macos")]
pub fn locales() -> Result<Vec<String>, SpeechError> {
    if analyzer::available() {
        analyzer::locales()
    } else {
        legacy::locales()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn transcribe(_audio: &Path, _locale: Option<&str>, _on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
    let _running = RunningGuard::acquire()?;
    Err(SpeechError::Unsupported(
        "this platform has no built-in speech engine — install a speech model and pass it as `engine`".to_string(),
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn cancel() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

/// Transcribes with an installed sherpa-onnx model instead of the OS
/// engine — every platform. Shares the one-at-a-time rule and `cancel()`.
pub fn transcribe_with_engine(
    audio: &Path,
    engine: &crate::sherpa::Engine,
    locale: Option<&str>,
    on_progress: impl FnMut(f64),
) -> Result<Transcript, SpeechError> {
    let _running = RunningGuard::acquire()?;
    crate::sherpa::transcribe(audio, engine, locale, &|| CANCEL_REQUESTED.load(Ordering::SeqCst), on_progress)
}

#[cfg(not(target_os = "macos"))]
pub fn locales() -> Result<Vec<String>, SpeechError> {
    Err(SpeechError::Unsupported("speech recognition isn't available on this platform yet".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: f64, end: f64) -> Word {
        Word { text: text.to_string(), start, end }
    }

    #[test]
    fn groups_words_at_pauses_and_sentence_ends() {
        let words = vec![
            word("Photosynthesis", 0.0, 0.8),
            word("makes", 0.9, 1.2),
            word("sugar.", 1.3, 3.2),
            word("Next", 3.3, 3.6),
            word("topic", 3.7, 4.0),
            word("after", 5.5, 5.8),
            word("pause", 5.9, 6.2),
        ];
        let segments = group_words(&words);
        let texts: Vec<_> = segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["Photosynthesis makes sugar.", "Next topic", "after pause"]);
        assert_eq!((segments[0].start_ms, segments[0].end_ms), (0, 3200));
        assert_eq!((segments[2].start_ms, segments[2].end_ms), (5500, 6200));
    }

    #[test]
    fn short_sentences_stay_together_and_long_runs_are_capped() {
        let short = vec![word("Yes.", 0.0, 0.4), word("Right.", 0.5, 0.9)];
        assert_eq!(group_words(&short).len(), 1);

        let long: Vec<Word> = (0..40).map(|i| word("word", i as f64 * 0.5, i as f64 * 0.5 + 0.4)).collect();
        let segments = group_words(&long);
        assert!(segments.len() >= 2);
        assert!(segments.iter().all(|s| s.end_ms - s.start_ms <= 15_000));
        assert!(group_words(&[]).is_empty());
    }

    #[test]
    fn converts_locale_identifiers() {
        assert_eq!(to_bcp47("en_US"), "en-US");
        assert_eq!(to_locale_identifier("zh-Hans-CN"), "zh_Hans_CN");
    }

    /// Every speaking fixture says this one sentence.
    const SENTENCE: &str = "photosynthesis turns light into sugar inside the leaf";
    const SPOKEN: [&str; 8] =
        ["lecture.mp4", "lecture.mov", "lecture.m4v", "lecture.webm", "lecture.mkv", "recorded.webm", "note.opus", "note.ogg"];

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech").join(name)
    }

    /// Transcribes every fixture, checking the sentence, timestamps and progress.
    fn transcribes_every_container(engine: &str, transcribe: impl Fn(&Path, &mut dyn FnMut(f64)) -> Result<Transcript, SpeechError>) {
        for name in SPOKEN {
            let mut fractions = Vec::new();
            let transcript = transcribe(&fixture(name), &mut |f| fractions.push(f)).unwrap_or_else(|e| panic!("{engine} {name}: {e:?}"));
            let heard: String = transcript.text.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
            for word in SENTENCE.split(' ') {
                assert!(heard.contains(word), "{engine} {name}: \"{}\" lacks \"{word}\"", transcript.text);
            }
            let (first, last) = (transcript.segments.first().unwrap(), transcript.segments.last().unwrap());
            assert!(first.start_ms < 1000 && (2500..3700).contains(&last.end_ms), "{engine} {name}: {:?}", transcript.segments);
            assert_eq!(fractions.last(), Some(&1.0), "{engine} {name}: progress {fractions:?}");
            assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{engine} {name}: progress {fractions:?}");
        }
    }

    /// The OS engine, only when CHAIN_TEST_SYSTEM_SPEECH is set (it may
    /// download the en-US model; macOS only).
    #[cfg(target_os = "macos")]
    #[test]
    fn system_engine_transcribes_video_and_opus() {
        if std::env::var_os("CHAIN_TEST_SYSTEM_SPEECH").is_none() {
            eprintln!("skipped: set CHAIN_TEST_SYSTEM_SPEECH to run against the OS engine");
            return;
        }
        transcribes_every_container("system", |path, progress| transcribe(path, Some("en-US"), progress));
        assert!(matches!(transcribe(&fixture("silent.mp4"), Some("en-US"), |_| {}), Err(SpeechError::NotFound(_))));
    }

    /// Whisper base through sherpa-onnx, only when CHAIN_TEST_SPEECH_MODELS
    /// names a models folder holding whisper-base/ and silero-vad/ (as an
    /// app's desktop.models folder does).
    #[cfg(not(chain_no_sherpa))]
    #[test]
    fn model_engine_transcribes_video_and_opus() {
        let Some(dir) = std::env::var_os("CHAIN_TEST_SPEECH_MODELS").map(std::path::PathBuf::from) else {
            eprintln!("skipped: set CHAIN_TEST_SPEECH_MODELS to run against real models");
            return;
        };
        let whisper = |file: &str| dir.join("whisper-base").join(file).to_string_lossy().into_owned();
        let engine = crate::sherpa::Engine {
            asr: crate::sherpa::AsrModelConfig::Whisper {
                encoder: whisper("base-encoder.int8.onnx"),
                decoder: whisper("base-decoder.int8.onnx"),
                tokens: whisper("base-tokens.txt"),
                language: None,
            },
            vad_model: dir.join("silero-vad/silero_vad.onnx"),
        };
        transcribes_every_container("whisper", |path, progress| transcribe_with_engine(path, &engine, Some("en-US"), progress));
        assert!(matches!(
            transcribe_with_engine(&fixture("silent.mp4"), &engine, None, |_| {}),
            Err(SpeechError::NotFound(_))
        ));
    }

    #[test]
    fn only_one_transcription_runs_at_a_time() {
        let first = RunningGuard::acquire().unwrap();
        assert!(matches!(RunningGuard::acquire(), Err(SpeechError::Unavailable(_))));
        drop(first);
        drop(RunningGuard::acquire().unwrap());
    }
}
