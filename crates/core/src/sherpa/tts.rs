//! Text-to-speech through sherpa-onnx — see
//! agent-docs/capabilities/tts/CONTRACT.md. Only real when chain-core is
//! built with the `tts` feature (an app opts in with package.json
//! `"chain": { "gpl": true }`), because the TTS-enabled sherpa-onnx links
//! espeak-ng (GPL-3.0). Without it, every call is `Unsupported`.

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum TtsError {
    InvalidArgument(String),
    Unsupported(String),
    /// `compile` while another compile is running.
    Unavailable(String),
    Cancelled,
    Other(String),
}

impl From<crate::m4a::M4aError> for TtsError {
    fn from(e: crate::m4a::M4aError) -> Self {
        match e {
            crate::m4a::M4aError::Unsupported(m) => TtsError::Unsupported(m),
            crate::m4a::M4aError::Failed(m) => TtsError::Other(m),
        }
    }
}

/// Which files and folders inside an installed voice model are which, by
/// relative name, and the model's family. The models capability resolves
/// them before they get here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TtsModelConfig {
    Kokoro {
        model: String,
        voices: String,
        tokens: String,
        /// espeak-ng-data folder.
        data_dir: Option<String>,
        /// Folder of jieba dictionaries (Chinese).
        dict_dir: Option<String>,
        #[serde(default)]
        lexicon: Vec<String>,
        lang: Option<String>,
        /// Speaker names in id order; the catalog knows them, the model files don't.
        speakers: Option<Vec<String>>,
    },
    /// Piper and other VITS voices.
    Vits {
        model: String,
        tokens: String,
        data_dir: Option<String>,
        dict_dir: Option<String>,
        #[serde(default)]
        lexicon: Vec<String>,
        speakers: Option<Vec<String>>,
    },
    Kitten {
        model: String,
        voices: String,
        tokens: String,
        data_dir: Option<String>,
        speakers: Option<Vec<String>>,
    },
}

impl TtsModelConfig {
    /// File names in the config, for resolving and validating.
    pub fn files_mut(&mut self) -> Vec<&mut String> {
        use TtsModelConfig::*;
        match self {
            Kokoro { model, voices, tokens, lexicon, .. } => {
                let mut files = vec![model, voices, tokens];
                files.extend(lexicon.iter_mut());
                files
            }
            Vits { model, tokens, lexicon, .. } => {
                let mut files = vec![model, tokens];
                files.extend(lexicon.iter_mut());
                files
            }
            Kitten { model, voices, tokens, .. } => vec![model, voices, tokens],
        }
    }

    /// Folder names in the config.
    pub fn dirs_mut(&mut self) -> Vec<&mut String> {
        use TtsModelConfig::*;
        match self {
            Kokoro { data_dir, dict_dir, .. } | Vits { data_dir, dict_dir, .. } => {
                [data_dir, dict_dir].into_iter().filter_map(Option::as_mut).collect()
            }
            Kitten { data_dir, .. } => data_dir.iter_mut().collect(),
        }
    }

    #[cfg_attr(not(all(feature = "tts", not(chain_no_sherpa))), allow(dead_code))]
    fn speakers(&self) -> Option<&[String]> {
        use TtsModelConfig::*;
        match self {
            Kokoro { speakers, .. } | Vits { speakers, .. } | Kitten { speakers, .. } => speakers.as_deref(),
        }
    }
}

/// Where one `compile` segment sits in the audio, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SegmentTiming {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Serialize)]
pub struct Compiled {
    pub duration: f64,
    pub segments: Vec<SegmentTiming>,
}

/// Back-to-back timings from each segment's length in samples. Summing
/// frames (not seconds) keeps the last `end` exactly `duration`.
#[cfg_attr(not(all(feature = "tts", not(chain_no_sherpa))), allow(dead_code))]
fn timings(frame_counts: &[u64], sample_rate: u32) -> Compiled {
    let seconds = |frames: u64| frames as f64 / sample_rate as f64;
    let mut position = 0;
    let segments = frame_counts
        .iter()
        .map(|&frames| {
            let start = position;
            position += frames;
            SegmentTiming { start: seconds(start), end: seconds(position) }
        })
        .collect();
    Compiled { duration: seconds(position), segments }
}

#[derive(Debug, Serialize)]
pub struct Voice {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Kokoro's voice names start with a language letter (`af_heart`,
/// `zf_xiaobei`); map it to BCP-47.
#[cfg_attr(not(all(feature = "tts", not(chain_no_sherpa))), allow(dead_code))]
fn kokoro_language(name: &str) -> Option<String> {
    let tag = match name.chars().next()? {
        'a' => "en-US",
        'b' => "en-GB",
        'e' => "es",
        'f' => "fr",
        'h' => "hi",
        'i' => "it",
        'j' => "ja",
        'p' => "pt-BR",
        'z' => "zh",
        _ => return None,
    };
    name.get(1..2).filter(|second| *second == "f" || *second == "m").map(|_| tag.to_string())
}

#[cfg_attr(not(all(feature = "tts", not(chain_no_sherpa))), allow(dead_code))]
fn voices_from(config: &TtsModelConfig, count: i32) -> Vec<Voice> {
    (0..count)
        .map(|id| {
            let name = config
                .speakers()
                .and_then(|names| names.get(id as usize).cloned())
                .unwrap_or_else(|| format!("Voice {}", id + 1));
            let language = match config {
                TtsModelConfig::Kokoro { .. } => kokoro_language(&name),
                _ => None,
            };
            Voice { id, name, language }
        })
        .collect()
}

/// 16-bit PCM mono WAV — what every webview's `<audio>` plays.
pub fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[cfg(all(feature = "tts", not(chain_no_sherpa)))]
mod engine {
    use std::ffi::CString;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use super::super::tts_ffi::*;
    use super::*;

    const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.25..=4.0;

    fn check_speed(speed: f32) -> Result<(), TtsError> {
        if SPEED_RANGE.contains(&speed) {
            Ok(())
        } else {
            Err(TtsError::InvalidArgument(format!("speed must be between 0.25 and 4, got {speed}")))
        }
    }

    struct Tts(*const SherpaOnnxOfflineTts);
    // sherpa-onnx's OfflineTts is safe to move between threads; calls are
    // serialized by the mutex below.
    unsafe impl Send for Tts {}
    impl Drop for Tts {
        fn drop(&mut self) {
            unsafe { SherpaOnnxDestroyOfflineTts(self.0) }
        }
    }

    /// The last loaded voice model, keyed by its resolved config. Loading
    /// Kokoro takes about a second; reusing it keeps paragraph-by-paragraph
    /// read-aloud fast.
    static LOADED: Mutex<Option<(String, Tts)>> = Mutex::new(None);

    fn create(config: &TtsModelConfig) -> Result<Tts, TtsError> {
        let mut strings: Vec<CString> = Vec::new();
        let mut add = |value: &str| {
            let owned = CString::new(value).unwrap_or_default();
            let pointer = owned.as_ptr();
            strings.push(owned);
            pointer
        };
        let mut c: OfflineTtsConfig = unsafe { std::mem::zeroed() };
        c.model.num_threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4) as i32);
        c.model.provider = add("cpu");
        c.max_num_sentences = 1;
        c.silence_scale = 0.2;
        match config {
            TtsModelConfig::Kokoro { model, voices, tokens, data_dir, dict_dir, lexicon, lang, .. } => {
                c.model.kokoro.model = add(model);
                c.model.kokoro.voices = add(voices);
                c.model.kokoro.tokens = add(tokens);
                c.model.kokoro.length_scale = 1.0;
                if let Some(dir) = data_dir {
                    c.model.kokoro.data_dir = add(dir);
                }
                if let Some(dir) = dict_dir {
                    c.model.kokoro.dict_dir = add(dir);
                }
                if !lexicon.is_empty() {
                    c.model.kokoro.lexicon = add(&lexicon.join(","));
                }
                if let Some(lang) = lang {
                    c.model.kokoro.lang = add(lang);
                }
            }
            TtsModelConfig::Vits { model, tokens, data_dir, dict_dir, lexicon, .. } => {
                c.model.vits.model = add(model);
                c.model.vits.tokens = add(tokens);
                c.model.vits.noise_scale = 0.667;
                c.model.vits.noise_scale_w = 0.8;
                c.model.vits.length_scale = 1.0;
                if let Some(dir) = data_dir {
                    c.model.vits.data_dir = add(dir);
                }
                if let Some(dir) = dict_dir {
                    c.model.vits.dict_dir = add(dir);
                }
                if !lexicon.is_empty() {
                    c.model.vits.lexicon = add(&lexicon.join(","));
                }
            }
            TtsModelConfig::Kitten { model, voices, tokens, data_dir, .. } => {
                c.model.kitten.model = add(model);
                c.model.kitten.voices = add(voices);
                c.model.kitten.tokens = add(tokens);
                c.model.kitten.length_scale = 1.0;
                if let Some(dir) = data_dir {
                    c.model.kitten.data_dir = add(dir);
                }
            }
        }
        let tts = unsafe { SherpaOnnxCreateOfflineTts(&c) };
        drop(strings);
        if tts.is_null() {
            return Err(TtsError::Other("couldn't load the voice model — its files may not match its type".to_string()));
        }
        // ONNX Runtime's first inference is several times slower than the
        // rest (12 s vs ~2.5 s for Kokoro on an M3 Pro); pay it here, at
        // load, instead of on the user's first sentence.
        let warm_up = CString::new("Hello.").expect("no NUL");
        unsafe { SherpaOnnxDestroyOfflineTtsGeneratedAudio(SherpaOnnxOfflineTtsGenerate(tts, warm_up.as_ptr(), 0, 1.0)) };
        Ok(Tts(tts))
    }

    /// Runs `f` with the loaded engine for `config`, loading it first if needed.
    fn with_engine<T>(config: &TtsModelConfig, f: impl FnOnce(&Tts) -> Result<T, TtsError>) -> Result<T, TtsError> {
        let key = serde_json::to_string(config).map_err(|e| TtsError::Other(e.to_string()))?;
        let mut loaded = LOADED.lock().expect("tts mutex poisoned");
        if loaded.as_ref().is_none_or(|(k, _)| *k != key) {
            *loaded = None; // free the old model before loading the next
            *loaded = Some((key, create(config)?));
        }
        f(&loaded.as_ref().expect("just loaded").1)
    }

    pub fn voices(config: &TtsModelConfig) -> Result<Vec<Voice>, TtsError> {
        with_engine(config, |tts| Ok(voices_from(config, unsafe { SherpaOnnxOfflineTtsNumSpeakers(tts.0) }.max(1))))
    }

    /// Returns WAV bytes.
    pub fn synthesize(text: &str, config: &TtsModelConfig, voice: i32, speed: f32) -> Result<Vec<u8>, TtsError> {
        if text.trim().is_empty() {
            return Err(TtsError::InvalidArgument("there's no text to speak".to_string()));
        }
        check_speed(speed)?;
        let text = CString::new(text).map_err(|_| TtsError::InvalidArgument("text can't contain NUL".to_string()))?;
        with_engine(config, |tts| generate(tts, &text, voice, speed, wav))
    }

    /// Speaks `text` and hands the samples and sample rate to `use_audio`
    /// while sherpa-onnx still owns them.
    fn generate<T>(
        tts: &Tts,
        text: &CString,
        voice: i32,
        speed: f32,
        use_audio: impl FnOnce(&[f32], u32) -> T,
    ) -> Result<T, TtsError> {
        unsafe {
            let count = SherpaOnnxOfflineTtsNumSpeakers(tts.0).max(1);
            if voice < 0 || voice >= count {
                return Err(TtsError::InvalidArgument(format!("voice {voice} doesn't exist; this model has {count}")));
            }
            let audio = SherpaOnnxOfflineTtsGenerate(tts.0, text.as_ptr(), voice, speed);
            if audio.is_null() {
                return Err(TtsError::Other("speech synthesis failed".to_string()));
            }
            let samples = std::slice::from_raw_parts((*audio).samples, (*audio).n.max(0) as usize);
            let result = use_audio(samples, (*audio).sample_rate as u32);
            SherpaOnnxDestroyOfflineTtsGeneratedAudio(audio);
            Ok(result)
        }
    }

    /// One compile per app (see CONTRACT.md); `cancel_compile` sets the
    /// flag, checked before each segment.
    static COMPILING: AtomicBool = AtomicBool::new(false);
    static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

    struct CompilingGuard;

    impl CompilingGuard {
        fn acquire() -> Result<Self, TtsError> {
            if COMPILING.swap(true, Ordering::SeqCst) {
                return Err(TtsError::Unavailable("audio is already being compiled".to_string()));
            }
            CANCEL_REQUESTED.store(false, Ordering::SeqCst);
            Ok(Self)
        }
    }

    impl Drop for CompilingGuard {
        fn drop(&mut self) {
            COMPILING.store(false, Ordering::SeqCst);
        }
    }

    pub fn cancel_compile() {
        if COMPILING.load(Ordering::SeqCst) {
            CANCEL_REQUESTED.store(true, Ordering::SeqCst);
        }
    }

    /// Speaks `segments` in order into one AAC file at `out`. The engine
    /// lock is taken per segment, so `synthesize` calls can run between
    /// them. On error `out` may hold a partial file; the caller removes it.
    pub fn compile(
        segments: &[String],
        config: &TtsModelConfig,
        voice: i32,
        speed: f32,
        out: &std::path::Path,
        mut progress: impl FnMut(f64),
    ) -> Result<Compiled, TtsError> {
        let _guard = CompilingGuard::acquire()?;
        if segments.is_empty() {
            return Err(TtsError::InvalidArgument("there are no segments to speak".to_string()));
        }
        check_speed(speed)?;
        let texts = segments
            .iter()
            .enumerate()
            .map(|(i, text)| {
                if text.trim().is_empty() {
                    return Err(TtsError::InvalidArgument(format!("segment {i} has no text to speak")));
                }
                CString::new(text.as_str())
                    .map_err(|_| TtsError::InvalidArgument(format!("segment {i} contains NUL")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let total_chars: usize = segments.iter().map(|s| s.chars().count()).sum();
        let mut spoken_chars = 0;
        let mut writer: Option<(crate::m4a::M4aWriter, u32)> = None;
        let mut frame_counts = Vec::with_capacity(segments.len());
        for (text, segment) in texts.iter().zip(segments) {
            if CANCEL_REQUESTED.load(Ordering::SeqCst) {
                return Err(TtsError::Cancelled);
            }
            let frames = with_engine(config, |tts| {
                generate(tts, text, voice, speed, |samples, rate| -> Result<u64, TtsError> {
                    let (file, file_rate) = match &mut writer {
                        Some(open) => open,
                        None => writer.insert((crate::m4a::M4aWriter::create(out, rate)?, rate)),
                    };
                    // Another synthesize() may have swapped models in between.
                    if *file_rate != rate {
                        return Err(TtsError::Other(format!("the voice changed sample rate mid-compile ({file_rate} → {rate} Hz)")));
                    }
                    file.write(samples)?;
                    Ok(samples.len() as u64)
                })
            })??;
            frame_counts.push(frames);
            spoken_chars += segment.chars().count();
            progress(spoken_chars as f64 / total_chars as f64);
        }
        let (file, rate) = writer.expect("segments is non-empty");
        file.finish()?;
        Ok(timings(&frame_counts, rate))
    }
}

#[cfg(all(feature = "tts", not(chain_no_sherpa)))]
pub use engine::{cancel_compile, compile, synthesize, voices};

#[cfg(not(all(feature = "tts", not(chain_no_sherpa))))]
fn not_built() -> TtsError {
    TtsError::Unsupported(
        "text-to-speech isn't in this build — it links espeak-ng (GPL-3.0), so the app must opt in with package.json \"chain\": { \"gpl\": true }"
            .to_string(),
    )
}

#[cfg(not(all(feature = "tts", not(chain_no_sherpa))))]
pub fn voices(_config: &TtsModelConfig) -> Result<Vec<Voice>, TtsError> {
    Err(not_built())
}

#[cfg(not(all(feature = "tts", not(chain_no_sherpa))))]
pub fn synthesize(_text: &str, _config: &TtsModelConfig, _voice: i32, _speed: f32) -> Result<Vec<u8>, TtsError> {
    Err(not_built())
}

#[cfg(not(all(feature = "tts", not(chain_no_sherpa))))]
pub fn compile(
    _segments: &[String],
    _config: &TtsModelConfig,
    _voice: i32,
    _speed: f32,
    _out: &std::path::Path,
    _progress: impl FnMut(f64),
) -> Result<Compiled, TtsError> {
    Err(not_built())
}

#[cfg(not(all(feature = "tts", not(chain_no_sherpa))))]
pub fn cancel_compile() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kokoro_and_piper_configs() {
        let mut kokoro: TtsModelConfig = serde_json::from_str(
            r#"{"type":"kokoro","model":"model.onnx","voices":"voices.bin","tokens":"tokens.txt","dataDir":"espeak-ng-data","dictDir":"dict","lexicon":["lexicon-us-en.txt","lexicon-zh.txt"]}"#,
        )
        .unwrap();
        assert_eq!(kokoro.files_mut().len(), 5);
        assert_eq!(kokoro.dirs_mut().len(), 2);
        let mut piper: TtsModelConfig = serde_json::from_str(
            r#"{"type":"vits","model":"en_US-amy-low.onnx","tokens":"tokens.txt","dataDir":"espeak-ng-data"}"#,
        )
        .unwrap();
        assert_eq!(piper.files_mut().len(), 2);
        assert_eq!(piper.dirs_mut().len(), 1);
    }

    #[test]
    fn names_voices_and_derives_kokoro_languages() {
        let config: TtsModelConfig = serde_json::from_str(
            r#"{"type":"kokoro","model":"m","voices":"v","tokens":"t","speakers":["af_heart","bm_george","zf_xiaobei"]}"#,
        )
        .unwrap();
        let voices = voices_from(&config, 4);
        assert_eq!((voices[0].name.as_str(), voices[0].language.as_deref()), ("af_heart", Some("en-US")));
        assert_eq!(voices[1].language.as_deref(), Some("en-GB"));
        assert_eq!(voices[2].language.as_deref(), Some("zh"));
        assert_eq!((voices[3].name.as_str(), voices[3].language.as_deref()), ("Voice 4", None));
    }

    #[test]
    fn times_segments_back_to_back_from_frame_counts() {
        let compiled = timings(&[24_000, 12_000, 36_000], 24_000);
        assert_eq!(compiled.segments[0], SegmentTiming { start: 0.0, end: 1.0 });
        assert_eq!(compiled.segments[1], SegmentTiming { start: 1.0, end: 1.5 });
        assert_eq!(compiled.segments[2], SegmentTiming { start: 1.5, end: 3.0 });
        assert_eq!(compiled.duration, 3.0);
    }

    #[test]
    fn writes_a_valid_wav_header() {
        let bytes = wav(&[0.0, 1.0, -1.0], 24_000);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(i16::from_le_bytes(bytes[46..48].try_into().unwrap()), i16::MAX);
        assert_eq!(bytes.len(), 44 + 6);
    }

    #[cfg(all(feature = "tts", not(chain_no_sherpa)))]
    #[test]
    fn tts_layout_matches_c_header() {
        // Printed from sherpa-onnx 1.13.8's c-api.h on a 64-bit target.
        use super::super::tts_ffi::*;
        assert_eq!(std::mem::size_of::<OfflineTtsConfig>(), 448);
        assert_eq!(std::mem::size_of::<OfflineTtsModelConfig>(), 416);
        assert_eq!(std::mem::offset_of!(OfflineTtsModelConfig, kokoro), 128);
        assert_eq!(std::mem::offset_of!(OfflineTtsConfig, silence_scale), 440);
        assert_eq!(std::mem::size_of::<SherpaOnnxGeneratedAudio>(), 16);
    }
}
