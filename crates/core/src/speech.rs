//! Speech capability — see /agent-docs/capabilities/speech/CONTRACT.md.
//! On-device only — audio never leaves the machine. macOS 26+ uses
//! SpeechAnalyzer through the Swift bridge in swift/ChainSpeech.swift
//! (built by build.rs); older macOS falls back to `SFSpeechRecognizer`
//! with `requiresOnDeviceRecognition`, through objc2. Every other
//! platform is `Unsupported` for now.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum SpeechError {
    /// No on-device model for the locale, or no recognizer on this platform.
    Unsupported(String),
    /// Another transcription is already running.
    Unavailable(String),
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

    pub fn transcribe(audio: &Path, locale: Option<&str>, on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
        let path = CString::new(audio.to_string_lossy().as_bytes()).map_err(|e| SpeechError::Other(e.to_string()))?;
        let locale = locale.map(CString::new).transpose().map_err(|e| SpeechError::Other(e.to_string()))?;
        let (status, payload) = call(
            |context| unsafe {
                chain_speech_analyzer_transcribe(
                    path.as_ptr(),
                    locale.as_ref().map_or(std::ptr::null(), |l| l.as_ptr()),
                    context,
                    forward_progress,
                    forward_finish,
                );
                // A cancel() that landed before Swift registered its task.
                if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                    chain_speech_analyzer_cancel();
                }
            },
            on_progress,
        );
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            return Err(SpeechError::Cancelled);
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
    use objc2_avf_audio::AVAudioFile;
    use objc2_foundation::{NSError, NSLocale, NSOperationQueue, NSString, NSURL};
    use objc2_speech::{
        SFSpeechRecognitionResult, SFSpeechRecognitionTask, SFSpeechRecognizer,
        SFSpeechRecognizerAuthorizationStatus, SFSpeechURLRecognitionRequest,
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

    pub fn transcribe(
        audio: &Path,
        locale: Option<&str>,
        mut on_progress: impl FnMut(f64),
    ) -> Result<Transcript, SpeechError> {
        authorize()?;
        autoreleasepool(|_| {
            let recognizer = recognizer_for(locale)?;
            let tag = to_bcp47(&unsafe { recognizer.locale() }.localeIdentifier().to_string());
            // Results arrive on this queue, never the main thread (which the
            // app's event loop owns).
            unsafe { recognizer.setQueue(&NSOperationQueue::new()) };

            let url = NSURL::fileURLWithPath(&NSString::from_str(&audio.to_string_lossy()));
            let file = unsafe { AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url) }
                .map_err(|e| SpeechError::Other(format!("couldn't read the audio: {}", e.localizedDescription())))?;
            let duration = unsafe { file.length() as f64 / file.processingFormat().sampleRate() };

            let request = unsafe { SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url) };
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

#[cfg(target_os = "macos")]
pub fn transcribe(audio: &Path, locale: Option<&str>, on_progress: impl FnMut(f64)) -> Result<Transcript, SpeechError> {
    let _running = RunningGuard::acquire()?;
    if analyzer::available() {
        analyzer::transcribe(audio, locale, on_progress)
    } else {
        legacy::transcribe(audio, locale, on_progress)
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

    #[test]
    fn only_one_transcription_runs_at_a_time() {
        let first = RunningGuard::acquire().unwrap();
        assert!(matches!(RunningGuard::acquire(), Err(SpeechError::Unavailable(_))));
        drop(first);
        drop(RunningGuard::acquire().unwrap());
    }
}
