//! Model-based transcription through sherpa-onnx — the `engine` option of
//! the speech capability (see agent-docs/capabilities/speech/CONTRACT.md).
//! Plain Rust over sherpa-onnx's C API, so it's the same on every OS.
//!
//! Pipeline, streamed so an hour of audio never sits in memory at once:
//! symphonia decodes the file → downmix to mono → sherpa's resampler to
//! 16 kHz → Silero VAD cuts speech segments → each segment is decoded by
//! the offline recognizer and becomes one transcript segment.

#[cfg(not(chain_no_sherpa))]
mod ffi;
#[cfg(all(feature = "tts", not(chain_no_sherpa)))]
mod tts_ffi;
pub mod tts;

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::speech::{Segment, SpeechError, Transcript};

/// Which sherpa-onnx model family a pack holds, and which of its files are
/// which — relative names inside the model's directory, from the app's
/// catalog. The models capability resolves them to paths before they get here.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum AsrModelConfig {
    Whisper { encoder: String, decoder: String, tokens: String, language: Option<String> },
    SenseVoice { model: String, tokens: String, language: Option<String>, use_itn: Option<bool> },
    /// v1 packs name a preprocessor plus uncached/cached decoders; v2
    /// packs have one merged decoder and no preprocessor.
    Moonshine {
        preprocessor: Option<String>,
        encoder: String,
        uncached_decoder: Option<String>,
        cached_decoder: Option<String>,
        merged_decoder: Option<String>,
        tokens: String,
    },
    Paraformer { model: String, tokens: String },
    Transducer { encoder: String, decoder: String, joiner: String, tokens: String },
    NemoCtc { model: String, tokens: String },
}

impl AsrModelConfig {
    /// Every file name in the config, for resolving and validating.
    pub fn files_mut(&mut self) -> Vec<&mut String> {
        use AsrModelConfig::*;
        match self {
            Whisper { encoder, decoder, tokens, .. } => vec![encoder, decoder, tokens],
            SenseVoice { model, tokens, .. } => vec![model, tokens],
            Moonshine { preprocessor, encoder, uncached_decoder, cached_decoder, merged_decoder, tokens } => {
                let optional = [preprocessor, uncached_decoder, cached_decoder, merged_decoder];
                let mut files: Vec<&mut String> = optional.into_iter().filter_map(Option::as_mut).collect();
                files.extend([encoder, tokens]);
                files
            }
            Paraformer { model, tokens } | NemoCtc { model, tokens } => vec![model, tokens],
            Transducer { encoder, decoder, joiner, tokens } => vec![encoder, decoder, joiner, tokens],
        }
    }

    fn language(&self) -> Option<&str> {
        match self {
            AsrModelConfig::Whisper { language, .. } | AsrModelConfig::SenseVoice { language, .. } => {
                language.as_deref()
            }
            _ => None,
        }
    }
}

/// A resolved engine request: the recognizer's files and the VAD model, as paths.
pub struct Engine {
    pub asr: AsrModelConfig,
    pub vad_model: PathBuf,
}

/// Whisper and SenseVoice take a bare language code; a BCP-47 locale's first subtag is it.
fn language_code(locale: &str) -> String {
    locale.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase()
}

/// 20 ms at 16 kHz — the frame the splitter measures loudness over.
const SPLIT_FRAME: usize = 320;

/// Ends of the pieces a speech segment is cut into so none exceeds `max`
/// samples: each cut lands on the quietest 20 ms frame in the back half of
/// the allowed window — usually a gap between words, where a hard cut at
/// the limit would split one.
fn split_points(samples: &[f32], max: usize) -> Vec<usize> {
    let mut points = Vec::new();
    let mut from = 0;
    while samples.len() - from > max {
        let (lo, hi) = (from + max / 2, from + max - SPLIT_FRAME);
        let energy = |at: usize| samples[at..at + SPLIT_FRAME].iter().map(|x| x * x).sum::<f32>();
        let cut = (lo..=hi)
            .step_by(SPLIT_FRAME / 2)
            .min_by(|a, b| energy(*a).total_cmp(&energy(*b)))
            .map_or(from + max, |at| at + SPLIT_FRAME / 2);
        points.push(cut);
        from = cut;
    }
    points.push(samples.len());
    points
}

/// Joined segment texts; empty segments dropped.
fn transcript_of(segments: Vec<Segment>, locale: String) -> Transcript {
    let segments: Vec<Segment> = segments.into_iter().filter(|s| !s.text.is_empty()).collect();
    let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    Transcript { text, segments, locale }
}

#[cfg(not(chain_no_sherpa))]
mod engine {
    use std::ffi::{CStr, CString};
    use std::fs::File;

    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as DecodeError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    use super::ffi::*;
    use super::*;

    const SAMPLE_RATE: i32 = 16_000;
    /// Silero VAD's window at 16 kHz.
    const VAD_WINDOW: usize = 512;

    /// Owns the C strings a config points into, so they outlive the create call.
    struct Strings(Vec<CString>);

    impl Strings {
        fn add(&mut self, value: &str) -> *const std::os::raw::c_char {
            let owned = CString::new(value).unwrap_or_default();
            let pointer = owned.as_ptr();
            self.0.push(owned);
            pointer
        }
    }

    struct Recognizer(*const OfflineRecognizer);
    impl Drop for Recognizer {
        fn drop(&mut self) {
            unsafe { SherpaOnnxDestroyOfflineRecognizer(self.0) }
        }
    }

    struct Vad(*const VoiceActivityDetector);
    impl Drop for Vad {
        fn drop(&mut self) {
            unsafe { SherpaOnnxDestroyVoiceActivityDetector(self.0) }
        }
    }

    struct Resampler(*const SherpaOnnxLinearResampler);
    impl Drop for Resampler {
        fn drop(&mut self) {
            unsafe { SherpaOnnxDestroyLinearResampler(self.0) }
        }
    }

    fn threads() -> i32 {
        std::thread::available_parallelism().map_or(2, |n| n.get().min(4) as i32)
    }

    fn create_recognizer(asr: &AsrModelConfig, language: &str) -> Result<Recognizer, SpeechError> {
        let mut s = Strings(Vec::new());
        let mut config: OfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config = FeatureConfig { sample_rate: SAMPLE_RATE, feature_dim: 80 };
        config.decoding_method = s.add("greedy_search");
        let model = &mut config.model_config;
        model.num_threads = threads();
        model.provider = s.add("cpu");
        match asr {
            AsrModelConfig::Whisper { encoder, decoder, tokens, .. } => {
                model.whisper.encoder = s.add(encoder);
                model.whisper.decoder = s.add(decoder);
                model.whisper.language = s.add(language);
                model.whisper.task = s.add("transcribe");
                model.whisper.tail_paddings = -1;
                model.tokens = s.add(tokens);
            }
            AsrModelConfig::SenseVoice { model: file, tokens, use_itn, .. } => {
                model.sense_voice.model = s.add(file);
                model.sense_voice.language = s.add(if language.is_empty() { "auto" } else { language });
                model.sense_voice.use_itn = i32::from(use_itn.unwrap_or(true));
                model.tokens = s.add(tokens);
            }
            AsrModelConfig::Moonshine { preprocessor, encoder, uncached_decoder, cached_decoder, merged_decoder, tokens } => {
                let mut optional = |file: &Option<String>| file.as_deref().map_or(std::ptr::null(), |f| s.add(f));
                model.moonshine.preprocessor = optional(preprocessor);
                model.moonshine.uncached_decoder = optional(uncached_decoder);
                model.moonshine.cached_decoder = optional(cached_decoder);
                model.moonshine.merged_decoder = optional(merged_decoder);
                model.moonshine.encoder = s.add(encoder);
                model.tokens = s.add(tokens);
            }
            AsrModelConfig::Paraformer { model: file, tokens } => {
                model.paraformer.model = s.add(file);
                model.tokens = s.add(tokens);
            }
            AsrModelConfig::Transducer { encoder, decoder, joiner, tokens } => {
                model.transducer.encoder = s.add(encoder);
                model.transducer.decoder = s.add(decoder);
                model.transducer.joiner = s.add(joiner);
                model.tokens = s.add(tokens);
            }
            AsrModelConfig::NemoCtc { model: file, tokens } => {
                model.nemo_ctc.model = s.add(file);
                model.tokens = s.add(tokens);
            }
        }
        let recognizer = unsafe { SherpaOnnxCreateOfflineRecognizer(&config) };
        if recognizer.is_null() {
            return Err(SpeechError::Other(
                "couldn't load the speech model — its files may not match its type".to_string(),
            ));
        }
        Ok(Recognizer(recognizer))
    }

    /// Longest speech segment each family decodes reliably. Whisper reads
    /// 30 s windows; Moonshine v2 fails past ~9 s (an ONNX broadcast error
    /// in its decoder, seen with the 2026-02-27 tiny-en pack).
    fn max_segment_seconds(asr: &AsrModelConfig) -> f32 {
        match asr {
            AsrModelConfig::Moonshine { .. } => 8.0,
            _ => 20.0,
        }
    }

    fn create_vad(model: &Path, max_segment_seconds: f32) -> Result<Vad, SpeechError> {
        let mut s = Strings(Vec::new());
        let mut config: VadModelConfig = unsafe { std::mem::zeroed() };
        config.silero_vad = SileroVadModelConfig {
            model: s.add(&model.to_string_lossy()),
            threshold: 0.5,
            min_silence_duration: 0.5,
            min_speech_duration: 0.25,
            window_size: VAD_WINDOW as i32,
            max_speech_duration: max_segment_seconds,
        };
        config.sample_rate = SAMPLE_RATE;
        config.num_threads = 1;
        config.provider = s.add("cpu");
        let vad = unsafe { SherpaOnnxCreateVoiceActivityDetector(&config, 60.0) };
        if vad.is_null() {
            return Err(SpeechError::Other("couldn't load the voice-activity model".to_string()));
        }
        Ok(Vad(vad))
    }

    fn recognize(recognizer: &Recognizer, samples: &[f32]) -> String {
        unsafe {
            let stream = SherpaOnnxCreateOfflineStream(recognizer.0);
            SherpaOnnxAcceptWaveformOffline(stream, SAMPLE_RATE, samples.as_ptr(), samples.len() as i32);
            SherpaOnnxDecodeOfflineStream(recognizer.0, stream);
            let json = SherpaOnnxGetOfflineStreamResultAsJson(stream);
            let text = if json.is_null() {
                String::new()
            } else {
                let value: serde_json::Value =
                    serde_json::from_str(&CStr::from_ptr(json).to_string_lossy()).unwrap_or_default();
                SherpaOnnxDestroyOfflineStreamResultJson(json);
                value["text"].as_str().unwrap_or_default().trim().to_string()
            };
            SherpaOnnxDestroyOfflineStream(stream);
            text
        }
    }

    /// Takes every finished speech segment off the VAD and recognizes it.
    /// The VAD's own maximum is soft (segments overshoot it by seconds), so
    /// anything longer than `max_samples` is split here (see `split_points`).
    fn drain(vad: &Vad, recognizer: &Recognizer, max_samples: usize, segments: &mut Vec<Segment>) {
        unsafe {
            while SherpaOnnxVoiceActivityDetectorEmpty(vad.0) == 0 {
                let segment = SherpaOnnxVoiceActivityDetectorFront(vad.0);
                let samples = std::slice::from_raw_parts((*segment).samples, (*segment).n as usize);
                let start = (*segment).start as usize;
                let mut from = 0;
                for to in split_points(samples, max_samples) {
                    let offset = (start + from) as u64;
                    segments.push(Segment {
                        start_ms: offset * 1000 / SAMPLE_RATE as u64,
                        end_ms: (start + to) as u64 * 1000 / SAMPLE_RATE as u64,
                        text: recognize(recognizer, &samples[from..to]),
                    });
                    from = to;
                }
                SherpaOnnxDestroySpeechSegment(segment);
                SherpaOnnxVoiceActivityDetectorPop(vad.0);
            }
        }
    }

    /// Container reader, decoder, sample rate, and frame count when known.
    type OpenedAudio =
        (Box<dyn symphonia::core::formats::FormatReader>, Box<dyn symphonia::core::codecs::Decoder>, u32, Option<u64>);

    fn open_audio(audio: &Path) -> Result<OpenedAudio, SpeechError> {
        let unreadable = |e: DecodeError| SpeechError::Other(format!("couldn't read the audio: {e}"));
        let file = File::open(audio).map_err(|e| SpeechError::Other(format!("couldn't open the audio: {e}")))?;
        let mut hint = Hint::new();
        if let Some(extension) = audio.extension().and_then(|e| e.to_str()) {
            hint.with_extension(extension);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, MediaSourceStream::new(Box::new(file), Default::default()), &FormatOptions::default(), &MetadataOptions::default())
            .map_err(unreadable)?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| SpeechError::Other("the file has no audio track".to_string()))?;
        let rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| SpeechError::Other("the audio doesn't say its sample rate".to_string()))?;
        let frames = track.codec_params.n_frames;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(unreadable)?;
        Ok((format, decoder, rate, frames))
    }

    pub fn transcribe(
        audio: &Path,
        engine: &Engine,
        locale: Option<&str>,
        cancelled: &dyn Fn() -> bool,
        mut on_progress: impl FnMut(f64),
    ) -> Result<Transcript, SpeechError> {
        let language = engine.asr.language().map(str::to_string).or_else(|| locale.map(language_code)).unwrap_or_default();
        let (mut format, mut decoder, rate, total_frames) = open_audio(audio)?;
        let recognizer = create_recognizer(&engine.asr, &language)?;
        let max_seconds = max_segment_seconds(&engine.asr);
        let max_samples = (max_seconds * SAMPLE_RATE as f32) as usize;
        let vad = create_vad(&engine.vad_model, max_seconds)?;
        let resampler = (rate as i32 != SAMPLE_RATE).then(|| {
            let cutoff = 0.99 * 0.5 * SAMPLE_RATE as f32;
            Resampler(unsafe { SherpaOnnxCreateLinearResampler(rate as i32, SAMPLE_RATE, cutoff, 6) })
        });

        let mut segments = Vec::new();
        let mut pending: Vec<f32> = Vec::new();
        let mut decoded_frames = 0u64;
        let mut reported = 0.0;
        let feed = |mono: &[f32], flush: bool, pending: &mut Vec<f32>, segments: &mut Vec<Segment>| {
            match &resampler {
                Some(resampler) => unsafe {
                    let out = SherpaOnnxLinearResamplerResample(resampler.0, mono.as_ptr(), mono.len() as i32, i32::from(flush));
                    pending.extend_from_slice(std::slice::from_raw_parts((*out).samples, (*out).n as usize));
                    SherpaOnnxLinearResamplerResampleFree(out);
                },
                None => pending.extend_from_slice(mono),
            }
            let whole = pending.len() / VAD_WINDOW * VAD_WINDOW;
            unsafe { SherpaOnnxVoiceActivityDetectorAcceptWaveform(vad.0, pending.as_ptr(), whole as i32) };
            pending.drain(..whole);
            drain(&vad, &recognizer, max_samples, segments);
        };

        loop {
            if cancelled() {
                return Err(SpeechError::Cancelled);
            }
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(SpeechError::Other(format!("couldn't read the audio: {e}"))),
            };
            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // A damaged packet is skipped, like players do.
                Err(DecodeError::DecodeError(_)) => continue,
                Err(e) => return Err(SpeechError::Other(format!("couldn't decode the audio: {e}"))),
            };
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            buffer.copy_interleaved_ref(decoded);
            let mono: Vec<f32> =
                buffer.samples().chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect();
            decoded_frames += mono.len() as u64;
            feed(&mono, false, &mut pending, &mut segments);
            if let Some(total) = total_frames.filter(|t| *t > 0) {
                // Whole percents only: one call per decoded packet would flood IPC.
                let fraction = ((decoded_frames as f64 / total as f64) * 100.0).floor() / 100.0;
                if fraction > reported {
                    reported = fraction;
                    on_progress(fraction.min(0.99));
                }
            }
        }
        feed(&[], true, &mut pending, &mut segments);
        unsafe {
            // Pad the last partial window so the VAD sees every sample, then close it.
            if !pending.is_empty() {
                pending.resize(VAD_WINDOW, 0.0);
                SherpaOnnxVoiceActivityDetectorAcceptWaveform(vad.0, pending.as_ptr(), VAD_WINDOW as i32);
            }
            SherpaOnnxVoiceActivityDetectorFlush(vad.0);
        }
        drain(&vad, &recognizer, max_samples, &mut segments);
        if cancelled() {
            return Err(SpeechError::Cancelled);
        }
        on_progress(1.0);
        let locale = locale.map(str::to_string).unwrap_or(if language.is_empty() { "und".to_string() } else { language });
        Ok(transcript_of(segments, locale))
    }
}

#[cfg(not(chain_no_sherpa))]
pub use engine::transcribe;

#[cfg(chain_no_sherpa)]
pub fn transcribe(
    _audio: &Path,
    _engine: &Engine,
    _locale: Option<&str>,
    _cancelled: &dyn Fn() -> bool,
    _on_progress: impl FnMut(f64),
) -> Result<Transcript, SpeechError> {
    Err(SpeechError::Unsupported("model-based transcription isn't available on this platform".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_configs() {
        let whisper: AsrModelConfig = serde_json::from_str(
            r#"{"type":"whisper","encoder":"tiny.en-encoder.onnx","decoder":"tiny.en-decoder.onnx","tokens":"tiny.en-tokens.txt"}"#,
        )
        .unwrap();
        assert!(matches!(whisper, AsrModelConfig::Whisper { language: None, .. }));
        let moonshine: AsrModelConfig = serde_json::from_str(
            r#"{"type":"moonshine","preprocessor":"p.onnx","encoder":"e.onnx","uncachedDecoder":"u.onnx","cachedDecoder":"c.onnx","tokens":"t.txt"}"#,
        )
        .unwrap();
        let mut moonshine = moonshine;
        assert_eq!(moonshine.files_mut().len(), 5);
        let mut merged: AsrModelConfig = serde_json::from_str(
            r#"{"type":"moonshine","encoder":"encoder_model.ort","mergedDecoder":"decoder_model_merged.ort","tokens":"tokens.txt"}"#,
        )
        .unwrap();
        assert_eq!(merged.files_mut().len(), 3);
        assert!(serde_json::from_str::<AsrModelConfig>(r#"{"type":"kokoro"}"#).is_err());
    }

    #[test]
    fn splits_long_segments_at_the_quietest_point() {
        // 3 s of "speech" with a silent gap at 2.0–2.1 s; max 2.5 s.
        let mut samples = vec![0.5f32; 48_000];
        samples[32_000..33_600].fill(0.0);
        let points = split_points(&samples, 40_000);
        assert_eq!(points.len(), 2);
        assert!((32_000..33_600).contains(&points[0]), "cut at {}", points[0]);
        assert_eq!(points[1], 48_000);
        // Short segments aren't split; long ones never exceed the max.
        assert_eq!(split_points(&samples[..1000], 40_000), vec![1000]);
        let long = vec![0.5f32; 200_000];
        let mut from = 0;
        for to in split_points(&long, 40_000) {
            assert!(to - from <= 40_000);
            from = to;
        }
    }

    #[test]
    fn language_code_is_the_first_subtag() {
        assert_eq!(language_code("en-US"), "en");
        assert_eq!(language_code("zh_Hans_CN"), "zh");
    }

    #[cfg(not(chain_no_sherpa))]
    #[test]
    fn layout_matches_c_header() {
        // Sizes printed from sherpa-onnx 1.13.8's c-api.h on a 64-bit target.
        assert_eq!(std::mem::size_of::<ffi::OfflineRecognizerConfig>(), 608);
        assert_eq!(std::mem::size_of::<ffi::OfflineModelConfig>(), 504);
        assert_eq!(std::mem::size_of::<ffi::VadModelConfig>(), 88);
        assert_eq!(std::mem::size_of::<ffi::SpeechSegment>(), 24);
    }
}
