//! Records the microphone, the computer's own output, or both mixed, into
//! an AAC `.m4a` as it goes — see agent-docs/capabilities/audio-recorder/.
//! Capture is native (swift/ChainRecorder.swift on macOS); mixing, level
//! metering and encoding (m4a.rs) happen here on a worker thread, so a
//! three-hour recording never sits in memory. One recording at a time.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::m4a::{M4aError, M4aWriter};
use crate::microphone_processor::MicrophoneProcessor;
use crate::recorder_tracks::RecorderTracks;
pub use crate::microphone_processor::Processing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Microphone,
    System,
    Both,
}

impl Source {
    fn microphone(self) -> bool {
        self != Source::System
    }

    fn system(self) -> bool {
        self != Source::Microphone
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub microphone: bool,
    pub system: bool,
    pub both: bool,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    pub microphone_choice: bool,
}

/// An input device, as macOS describes it (see contract.ts's `Microphone`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Microphone {
    pub id: String,
    pub name: String,
    /// "built-in", "bluetooth", "usb" or "other".
    pub transport: String,
    pub is_default: bool,
    pub sample_rate: f64,
}

/// Which microphone to record from: `id`, else the default input — or,
/// with `avoid_bluetooth`, another one when the default is Bluetooth.
#[derive(Debug, Clone, Default)]
pub struct MicrophoneChoice {
    pub id: Option<String>,
    pub avoid_bluetooth: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    pub microphone: Option<Microphone>,
    pub bluetooth_fallback: bool,
}

/// The recording moved to `microphone` because `previous` disappeared.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneChange {
    pub microphone: Microphone,
    pub previous: Microphone,
    pub bluetooth_fallback: bool,
}

#[derive(Debug)]
pub enum RecorderError {
    Unsupported(String),
    PermissionDenied(String),
    Unavailable(String),
    Failed(String),
}

impl From<M4aError> for RecorderError {
    fn from(e: M4aError) -> Self {
        match e {
            M4aError::Unsupported(m) => RecorderError::Unsupported(m),
            M4aError::Failed(m) => RecorderError::Failed(m),
        }
    }
}

pub struct Finished {
    /// The finished `.m4a`, still a temp file: the caller moves it into files.
    pub path: PathBuf,
    pub duration_ms: u64,
}

pub fn availability() -> Availability {
    let microphone = native::microphone_available();
    let system = native::system_available();
    let both = microphone && system;
    // Pure software on what the sources already capture: echo needs both,
    // the rest only the microphone.
    Availability {
        microphone,
        system,
        both,
        echo_cancellation: both,
        noise_suppression: microphone,
        auto_gain_control: microphone,
        microphone_choice: microphone,
    }
}

/// The connected inputs; empty where a microphone can't be recorded.
pub fn microphones() -> Vec<Microphone> {
    if !native::microphone_available() {
        return Vec::new();
    }
    native::microphones()
}

struct Chunk {
    microphone: Option<Vec<f32>>,
    system: Option<Vec<f32>>,
    sample_rate: f64,
}

/// What reaches the worker thread from the native side.
enum Event {
    Samples(Chunk),
    MicrophoneChange(MicrophoneChange),
}

/// What the native callback gets as its context: where to send samples,
/// and whether to drop them (paused).
struct Feed {
    sender: Sender<Event>,
    paused: Arc<AtomicBool>,
}

struct Recording {
    feed: *mut Feed,
    paused: Arc<AtomicBool>,
    worker: JoinHandle<Result<u64, RecorderError>>,
    path: PathBuf,
}

// SAFETY: `feed` is only dereferenced by the native callback until
// `native::stop` returns, then reclaimed once by `end`.
unsafe impl Send for Recording {}

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);

fn nothing_recording() -> RecorderError {
    RecorderError::Unavailable("nothing is recording".to_string())
}

/// Blocks while the OS asks the user for permission, then records into a
/// temp file until `stop` or `cancel`. `processing` acts on the microphone:
/// what doesn't apply to `source` is ignored (echo cancellation without
/// "both", anything for "system"). `on_level` gets 0–1 about ten times a
/// second, and `on_microphone_change` each switch to another microphone,
/// from the worker thread.
pub fn start(
    source: Source,
    microphone: MicrophoneChoice,
    processing: Processing,
    on_level: impl FnMut(f32) + Send + 'static,
    on_microphone_change: impl FnMut(MicrophoneChange) + Send + 'static,
) -> Result<Started, RecorderError> {
    let mut recording = RECORDING.lock().expect("recorder mutex poisoned");
    if recording.is_some() {
        return Err(RecorderError::Unavailable("a recording is already running".to_string()));
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!("chain-recording-{}-{nanos}.m4a", std::process::id()));
    let (sender, receiver) = mpsc::channel();
    let paused = Arc::new(AtomicBool::new(false));
    let feed = Box::into_raw(Box::new(Feed { sender, paused: paused.clone() }));
    let worker = {
        let path = path.clone();
        let processing = Processing {
            echo_cancellation: processing.echo_cancellation && source == Source::Both,
            noise_suppression: processing.noise_suppression && source.microphone(),
            auto_gain_control: processing.auto_gain_control && source.microphone(),
        };
        std::thread::spawn(move || encode(receiver, &path, processing, on_level, on_microphone_change))
    };
    let started = native::start(source, &microphone, feed);
    if started.is_err() {
        // SAFETY: start failed, so the native side holds no callback using it.
        drop(unsafe { Box::from_raw(feed) });
        let _ = worker.join();
        let _ = std::fs::remove_file(&path);
    } else {
        *recording = Some(Recording { feed, paused, worker, path });
    }
    started
}

pub fn pause() -> Result<(), RecorderError> {
    set_paused(true)
}

pub fn resume() -> Result<(), RecorderError> {
    set_paused(false)
}

fn set_paused(paused: bool) -> Result<(), RecorderError> {
    let recording = RECORDING.lock().expect("recorder mutex poisoned");
    let recording = recording.as_ref().ok_or_else(nothing_recording)?;
    recording.paused.store(paused, Ordering::SeqCst);
    Ok(())
}

pub fn stop() -> Result<Finished, RecorderError> {
    let recording = RECORDING.lock().expect("recorder mutex poisoned").take().ok_or_else(nothing_recording)?;
    let path = recording.path.clone();
    match end(recording) {
        Ok(duration_ms) => Ok(Finished { path, duration_ms }),
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            Err(e)
        }
    }
}

/// Stops and deletes the take; a no-op when idle.
pub fn cancel() {
    let Some(recording) = RECORDING.lock().expect("recorder mutex poisoned").take() else { return };
    let path = recording.path.clone();
    let _ = end(recording);
    let _ = std::fs::remove_file(path);
}

fn end(recording: Recording) -> Result<u64, RecorderError> {
    native::stop();
    // SAFETY: native::stop guarantees no callback runs any more; dropping
    // the sender lets the worker finish the file.
    drop(unsafe { Box::from_raw(recording.feed) });
    recording.worker.join().map_err(|_| RecorderError::Failed("the recording thread panicked".to_string()))?
}

/// Runs on the worker thread: processes the microphone, mixes, meters and
/// encodes every chunk until the feed is dropped. Returns the recorded
/// duration in milliseconds.
fn encode(
    receiver: Receiver<Event>,
    path: &Path,
    processing: Processing,
    mut on_level: impl FnMut(f32),
    mut on_microphone_change: impl FnMut(MicrophoneChange),
) -> Result<u64, RecorderError> {
    let mut writer: Option<(M4aWriter, f64)> = None;
    let mut converter: Option<RateConverter> = None;
    let mut processor: Option<MicrophoneProcessor> = None;
    let mut tracks: Option<RecorderTracks> = None;
    let (mut cleaned, mut system) = (Vec::new(), Vec::new());
    let mut mixer = Mixer { level_microphone_alone: processing.auto_gain_control, ..Mixer::default() };
    let mut meter = LevelMeter::default();
    let mut mixed = Vec::new();
    let mut frames = 0u64;
    for event in receiver {
        let mut chunk = match event {
            Event::Samples(chunk) => chunk,
            Event::MicrophoneChange(change) => {
                on_microphone_change(change);
                continue;
            }
        };
        if writer.is_none() {
            writer = Some((M4aWriter::create(path, chunk.sample_rate.round() as u32)?, chunk.sample_rate));
            processor = processing.any().then(|| MicrophoneProcessor::new(chunk.sample_rate, processing));
            tracks = RecorderTracks::from_env(chunk.sample_rate);
        }
        if let Some(&(_, take_rate)) = writer.as_ref().filter(|(_, rate)| *rate != chunk.sample_rate) {
            if converter.as_ref().is_none_or(|c| c.from != chunk.sample_rate) {
                converter = Some(RateConverter::new(chunk.sample_rate, take_rate));
            }
            converter.as_mut().expect("just set").convert(&mut chunk);
        }
        let (microphone, played, speaking) = match (processor.as_mut(), chunk.microphone.as_deref()) {
            (Some(processor), Some(microphone)) => {
                let speaking = processor.process(microphone, chunk.system.as_deref(), &mut cleaned, &mut system);
                (Some(cleaned.as_slice()), chunk.system.is_some().then_some(system.as_slice()), speaking)
            }
            _ => (chunk.microphone.as_deref(), chunk.system.as_deref(), true),
        };
        mixer.mix(microphone, played, chunk.sample_rate, speaking, &mut mixed);
        if let Some(tracks) = tracks.as_mut() {
            tracks.write(chunk.microphone.as_deref(), microphone, mixer.microphone_applied, played);
        }
        if let Some((writer, _)) = writer.as_mut() {
            writer.write(&mixed)?;
        }
        frames += mixed.len() as u64;
        if let Some(level) = meter.feed(&mixed, chunk.sample_rate) {
            on_level(level);
        }
    }
    if let Some(tracks) = tracks {
        tracks.finish();
    }
    let (writer, sample_rate) = writer.ok_or_else(|| RecorderError::Failed("nothing was recorded".to_string()))?;
    writer.finish()?;
    Ok((frames as f64 * 1000.0 / sample_rate).round() as u64)
}

/// Each source's loudness is steered toward this RMS (−20 dBFS) when mixing both.
const TARGET_RMS: f32 = 0.1;
/// Below −50 dBFS a source is treated as silent and keeps its gain, so
/// room hiss between sentences isn't turned up.
const SILENCE_RMS: f32 = 0.003;
const MIN_GAIN: f32 = 0.25;
const MAX_GAIN: f32 = 8.0;
/// Seconds for a gain to move most of the way to where it's heading.
const GAIN_TIME_CONSTANT: f64 = 1.5;
/// How fast the microphone's gain applies once someone starts speaking,
/// and eases back to at most 1× after they stop. Speex's speech detection
/// already bridges the gaps between words, so the release can be short.
const SPEECH_ATTACK: f64 = 0.02;
const SPEECH_RELEASE: f64 = 0.15;
/// The soft limiter starts bending the mix above this.
const LIMIT_KNEE: f32 = 0.8;

#[derive(Debug)]
struct Mixer {
    /// Where the microphone's level is heading, learned while someone speaks.
    microphone_gain: f32,
    system_gain: f32,
    /// 0–1: how much of `microphone_gain` applies — 1 while someone
    /// speaks, easing to 0 (at most 1×) in pauses.
    speech: f32,
    /// The microphone's gain at the end of the last mix.
    microphone_applied: f32,
    /// Gain control for "microphone" alone; "both" always levels.
    level_microphone_alone: bool,
}

impl Default for Mixer {
    fn default() -> Self {
        Self { microphone_gain: 1.0, system_gain: 1.0, speech: 0.0, microphone_applied: 1.0, level_microphone_alone: false }
    }
}

impl Mixer {
    /// "system", and "microphone" without gain control, are copied as is;
    /// otherwise the microphone is levelled while `speaking` (and the two
    /// sources toward each other), summed and soft-limited. Without speech
    /// detection, pass `speaking: true` — it levels on loudness alone.
    fn mix(
        &mut self,
        microphone: Option<&[f32]>,
        system: Option<&[f32]>,
        sample_rate: f64,
        speaking: bool,
        out: &mut Vec<f32>,
    ) {
        out.clear();
        match (microphone, system) {
            (Some(microphone), None) if self.level_microphone_alone => {
                let gains = self.microphone_gains(microphone, sample_rate, speaking);
                out.extend(microphone.iter().zip(gains).map(|(m, g)| soft_limit(m * g)));
            }
            (Some(only), None) | (None, Some(only)) => out.extend_from_slice(only),
            (Some(microphone), Some(system)) => {
                let gains = self.microphone_gains(microphone, sample_rate, speaking);
                level_gain(&mut self.system_gain, system, smoothing(system.len(), sample_rate, GAIN_TIME_CONSTANT));
                out.extend(
                    microphone
                        .iter()
                        .zip(gains)
                        .zip(system)
                        .map(|((m, g), s)| soft_limit(m * g + s * self.system_gain)),
                );
            }
            (None, None) => {}
        }
    }

    /// The microphone's gain across this block, ramped from the last one.
    /// It's steered toward the target only while someone speaks, and eases
    /// down to at most 1× in pauses, so echo the canceller left behind and
    /// room noise between sentences are never turned up.
    fn microphone_gains(&mut self, samples: &[f32], sample_rate: f64, speaking: bool) -> impl Iterator<Item = f32> {
        if speaking {
            level_gain(&mut self.microphone_gain, samples, smoothing(samples.len(), sample_rate, GAIN_TIME_CONSTANT));
        }
        let (target, time) = if speaking { (1.0, SPEECH_ATTACK) } else { (0.0, SPEECH_RELEASE) };
        self.speech += (target - self.speech) * smoothing(samples.len(), sample_rate, time);
        let floor = self.microphone_gain.min(1.0);
        let from = self.microphone_applied;
        self.microphone_applied = floor + (self.microphone_gain - floor) * self.speech;
        let step = (self.microphone_applied - from) / samples.len().max(1) as f32;
        (1..=samples.len()).map(move |i| from + step * i as f32)
    }
}

fn smoothing(frames: usize, sample_rate: f64, seconds: f64) -> f32 {
    (1.0 - (-(frames as f64) / (sample_rate * seconds)).exp()) as f32
}

fn level_gain(gain: &mut f32, samples: &[f32], smoothing: f32) {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    if rms < SILENCE_RMS {
        return;
    }
    let wanted = (TARGET_RMS / rms).clamp(MIN_GAIN, MAX_GAIN);
    *gain += (wanted - *gain) * smoothing;
}

fn soft_limit(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= LIMIT_KNEE {
        return sample;
    }
    let headroom = 1.0 - LIMIT_KNEE;
    sample.signum() * (LIMIT_KNEE + headroom * ((magnitude - LIMIT_KNEE) / headroom).tanh())
}

/// After a switch to a microphone running at another rate, brings both
/// sources back to the take's rate (the file has one). Linear
/// interpolation: plenty for a voice, not for music.
struct RateConverter {
    from: f64,
    microphone: Resampler,
    system: Resampler,
}

impl RateConverter {
    fn new(from: f64, to: f64) -> Self {
        Self { from, microphone: Resampler::new(from, to), system: Resampler::new(from, to) }
    }

    fn convert(&mut self, chunk: &mut Chunk) {
        if let Some(microphone) = chunk.microphone.as_mut() {
            *microphone = self.microphone.process(microphone);
        }
        if let Some(system) = chunk.system.as_mut() {
            *system = self.system.process(system);
        }
        chunk.sample_rate = self.microphone.to;
    }
}

struct Resampler {
    to: f64,
    /// Input samples per output sample.
    step: f64,
    /// Where the next output falls, counting the previous block's last
    /// sample as 0 and this block's samples from 1.
    position: f64,
    last: f32,
}

impl Resampler {
    fn new(from: f64, to: f64) -> Self {
        Self { to, step: from / to, position: 0.0, last: 0.0 }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 1);
        while self.position < input.len() as f64 {
            let i = self.position as usize;
            let a = if i == 0 { self.last } else { input[i - 1] };
            out.push(a + (input[i] - a) * (self.position - i as f64) as f32);
            self.position += self.step;
        }
        self.position -= input.len() as f64;
        self.last = input.last().copied().unwrap_or(self.last);
        out
    }
}

/// Turns samples into a 0–1 level per tenth of a second: the window's
/// peak on a dB scale from −60 dBFS (0) to full scale (1).
#[derive(Debug, Default)]
struct LevelMeter {
    peak: f32,
    frames: usize,
}

const LEVEL_FLOOR_DB: f32 = -60.0;

impl LevelMeter {
    fn feed(&mut self, samples: &[f32], sample_rate: f64) -> Option<f32> {
        self.peak = samples.iter().fold(self.peak, |peak, s| peak.max(s.abs()));
        self.frames += samples.len();
        if (self.frames as f64) < sample_rate / 10.0 {
            return None;
        }
        let db = 20.0 * self.peak.max(f32::MIN_POSITIVE).log10();
        *self = Self::default();
        Some(((db - LEVEL_FLOOR_DB) / -LEVEL_FLOOR_DB).clamp(0.0, 1.0))
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::sync::atomic::Ordering;

    use super::{Chunk, Event, Feed, Microphone, MicrophoneChoice, RecorderError, Source, Started};

    // Start status codes — keep in sync with ChainRecorder.swift.
    const STATUS_OK: i32 = 0;
    const STATUS_UNSUPPORTED: i32 = 1;
    const STATUS_DENIED: i32 = 2;
    const STATUS_UNAVAILABLE: i32 = 4;

    type Samples = extern "C" fn(*mut c_void, *const f32, *const f32, u32, f64);
    type MicrophoneChanged = extern "C" fn(*mut c_void, *const c_char);

    extern "C" {
        fn chain_recorder_microphone_available() -> bool;
        fn chain_recorder_system_available() -> bool;
        fn chain_recorder_microphones() -> *mut c_char;
        fn chain_recorder_start(
            microphone: bool,
            system: bool,
            microphone_id: *const c_char,
            avoid_bluetooth: bool,
            context: *mut c_void,
            on_samples: Samples,
            on_microphone_change: MicrophoneChanged,
            started: *mut *mut c_char,
            message: *mut *mut c_char,
        ) -> i32;
        fn chain_recorder_stop();
    }

    /// Takes a strdup'd string from Swift; None when it's null.
    unsafe fn take_string(text: *mut c_char) -> Option<String> {
        if text.is_null() {
            return None;
        }
        let owned = CStr::from_ptr(text).to_string_lossy().into_owned();
        libc::free(text.cast());
        Some(owned)
    }

    pub fn microphones() -> Vec<Microphone> {
        // SAFETY: returns a strdup'd JSON string, freed by take_string.
        let json = unsafe { take_string(chain_recorder_microphones()) };
        json.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
    }

    pub fn microphone_available() -> bool {
        // SAFETY: no arguments; reads OS state only.
        unsafe { chain_recorder_microphone_available() }
    }

    pub fn system_available() -> bool {
        // SAFETY: as above.
        unsafe { chain_recorder_system_available() }
    }

    unsafe fn copy(samples: *const f32, frames: u32) -> Option<Vec<f32>> {
        (!samples.is_null()).then(|| std::slice::from_raw_parts(samples, frames as usize).to_vec())
    }

    extern "C" fn forward(context: *mut c_void, microphone: *const f32, system: *const f32, frames: u32, sample_rate: f64) {
        // SAFETY: `context` is the Feed `start` passed, alive until after
        // chain_recorder_stop; the sample pointers hold `frames` floats for
        // the duration of this call.
        let feed = unsafe { &*(context as *const Feed) };
        if feed.paused.load(Ordering::SeqCst) {
            return;
        }
        let chunk = unsafe { Chunk { microphone: copy(microphone, frames), system: copy(system, frames), sample_rate } };
        let _ = feed.sender.send(Event::Samples(chunk));
    }

    extern "C" fn microphone_changed(context: *mut c_void, json: *const c_char) {
        // SAFETY: as in `forward`; `json` is valid for the call.
        let feed = unsafe { &*(context as *const Feed) };
        let json = unsafe { CStr::from_ptr(json) }.to_string_lossy();
        if let Ok(change) = serde_json::from_str(&json) {
            let _ = feed.sender.send(Event::MicrophoneChange(change));
        }
    }

    pub fn start(source: Source, choice: &MicrophoneChoice, feed: *mut Feed) -> Result<Started, RecorderError> {
        let id = choice.id.as_deref().map(|id| CString::new(id).unwrap_or_default());
        let (mut started, mut message): (*mut c_char, *mut c_char) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: `feed` outlives the recording (see Recording); `id` lives
        // through the call; `started`/`message` are left null or set to
        // strdup'd strings, freed by take_string.
        let (status, started, message) = unsafe {
            let status = chain_recorder_start(
                source.microphone(),
                source.system(),
                id.as_ref().map_or(std::ptr::null(), |id| id.as_ptr()),
                choice.avoid_bluetooth,
                feed.cast(),
                forward,
                microphone_changed,
                &mut started,
                &mut message,
            );
            (status, take_string(started), take_string(message))
        };
        if status == STATUS_OK {
            return started
                .and_then(|json| serde_json::from_str(&json).ok())
                .ok_or_else(|| RecorderError::Failed("couldn't read which microphone is recording".to_string()));
        }
        let text = message.unwrap_or_else(|| "couldn't start recording".to_string());
        Err(match status {
            STATUS_UNSUPPORTED => RecorderError::Unsupported(text),
            STATUS_DENIED => RecorderError::PermissionDenied(text),
            STATUS_UNAVAILABLE => RecorderError::Unavailable(text),
            _ => RecorderError::Failed(text),
        })
    }

    pub fn stop() {
        // SAFETY: no arguments; stops whatever is running.
        unsafe { chain_recorder_stop() }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::{Feed, Microphone, MicrophoneChoice, RecorderError, Source, Started};

    pub fn microphone_available() -> bool {
        false
    }

    pub fn microphones() -> Vec<Microphone> {
        Vec::new()
    }

    pub fn system_available() -> bool {
        false
    }

    pub fn start(_source: Source, _choice: &MicrophoneChoice, _feed: *mut Feed) -> Result<Started, RecorderError> {
        Err(RecorderError::Unsupported("audio recording isn't implemented on this platform yet".to_string()))
    }

    pub fn stop() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| amplitude * (i as f32 * 0.05).sin()).collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn one_source_is_recorded_as_is() {
        let mut mixer = Mixer::default();
        let mut out = Vec::new();
        let input = tone(0.02, 480);
        mixer.mix(Some(&input), None, 48_000.0, true, &mut out);
        assert_eq!(out, input);
        mixer.mix(None, Some(&input), 48_000.0, true, &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn both_brings_a_quiet_voice_up_to_a_loud_lecture() {
        let mut mixer = Mixer::default();
        let mut out = Vec::new();
        let voice = tone(0.02, 480);
        let lecture = tone(0.5, 480);
        // 5 s of 10 ms chunks: well past the gain's time constant.
        for _ in 0..500 {
            mixer.mix(Some(&voice), Some(&lecture), 48_000.0, true, &mut out);
        }
        let voice_level = rms(&voice) * mixer.microphone_gain;
        let lecture_level = rms(&lecture) * mixer.system_gain;
        assert!(voice_level / lecture_level > 0.8, "voice {voice_level} vs lecture {lecture_level}");
        assert!(out.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn the_microphone_is_never_raised_while_nobody_speaks() {
        let mut mixer = Mixer::default();
        let mut out = Vec::new();
        let voice = tone(0.01, 480);
        let lecture = tone(0.3, 480);
        // A quiet voice teaches the mixer a high gain…
        for _ in 0..500 {
            mixer.mix(Some(&voice), Some(&lecture), 48_000.0, true, &mut out);
        }
        assert!(mixer.microphone_applied > 4.0, "{}", mixer.microphone_applied);
        // …which isn't applied to what the microphone hears in a pause.
        let echo_left = tone(0.01, 480);
        for _ in 0..100 {
            mixer.mix(Some(&echo_left), Some(&lecture), 48_000.0, false, &mut out);
        }
        assert!(mixer.microphone_applied <= 1.01, "{}", mixer.microphone_applied);
        assert!(mixer.microphone_gain > 4.0, "the learned level is kept for the next sentence");
    }

    #[test]
    fn gain_control_levels_the_microphone_alone_only_when_asked() {
        let quiet = tone(0.01, 480);
        let mut out = Vec::new();
        let mut plain = Mixer::default();
        let mut levelled = Mixer { level_microphone_alone: true, ..Mixer::default() };
        for _ in 0..500 {
            plain.mix(Some(&quiet), None, 48_000.0, true, &mut out);
            levelled.mix(Some(&quiet), None, 48_000.0, true, &mut out);
        }
        assert_eq!(plain.microphone_applied, 1.0);
        assert!(rms(&out) > rms(&quiet) * 4.0);
    }

    #[test]
    fn resampling_keeps_the_duration_and_the_tone() {
        let mut down = Resampler::new(48_000.0, 16_000.0);
        let mut up = Resampler::new(16_000.0, 48_000.0);
        let input = tone(0.5, 48_000);
        let (mut low, mut back) = (Vec::new(), Vec::new());
        for block in input.chunks(441) {
            low.extend(down.process(block));
        }
        for block in low.chunks(160) {
            back.extend(up.process(block));
        }
        assert!((low.len() as i64 - 16_000).abs() <= 1, "{}", low.len());
        assert!((back.len() as i64 - 48_000).abs() <= 3, "{}", back.len());
        assert!((rms(&back[1000..47_000]) / rms(&input[1000..47_000]) - 1.0).abs() < 0.05);
    }

    #[test]
    fn silence_keeps_its_gain() {
        let mut mixer = Mixer::default();
        let mut out = Vec::new();
        let hiss = tone(0.001, 480);
        for _ in 0..500 {
            mixer.mix(Some(&hiss), Some(&hiss), 48_000.0, true, &mut out);
        }
        assert_eq!(mixer.microphone_gain, 1.0);
    }

    #[test]
    fn soft_limit_bends_only_above_the_knee() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.8), -0.8);
        assert!(soft_limit(3.0) <= 1.0 && soft_limit(3.0) > 0.95);
        assert!(soft_limit(-3.0) >= -1.0);
    }

    #[test]
    fn meter_reports_a_tenth_of_a_second_on_a_db_scale() {
        let mut meter = LevelMeter::default();
        assert_eq!(meter.feed(&[0.5; 4_000], 48_000.0), None);
        let level = meter.feed(&[0.0; 800], 48_000.0).unwrap();
        // 0.5 is −6 dBFS → 54/60.
        assert!((level - 0.9).abs() < 0.01, "{level}");
        assert_eq!(meter.feed(&[0.0; 4_800], 48_000.0), Some(0.0));
        assert_eq!(meter.feed(&[1.0; 4_800], 48_000.0), Some(1.0));
    }

    #[test]
    fn idle_controls_report_nothing_recording() {
        assert!(matches!(pause(), Err(RecorderError::Unavailable(_))));
        assert!(matches!(stop(), Err(RecorderError::Unavailable(_))));
        cancel();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn encodes_chunks_into_a_playable_file_with_their_duration() {
        let path = std::env::temp_dir().join(format!("chain-recorder-test-{}.m4a", std::process::id()));
        let (sender, receiver) = mpsc::channel();
        let mut levels = Vec::new();
        for _ in 0..100 {
            let chunk = Chunk { microphone: Some(tone(0.3, 480)), system: Some(tone(0.3, 480)), sample_rate: 48_000.0 };
            sender.send(Event::Samples(chunk)).unwrap();
        }
        drop(sender);
        let duration = encode(receiver, &path, Processing::default(), |level| levels.push(level), |_| {}).unwrap();
        assert_eq!(duration, 1000);
        assert_eq!(levels.len(), 10);
        let header = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(&header[4..8], b"ftyp");
    }

    #[test]
    fn nothing_recorded_is_an_error() {
        let (sender, receiver) = mpsc::channel::<Event>();
        drop(sender);
        let path = std::env::temp_dir().join("chain-recorder-never-written.m4a");
        assert!(matches!(encode(receiver, &path, Processing::default(), |_| {}, |_| {}), Err(RecorderError::Failed(_))));
    }
}
