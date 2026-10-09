//! A media file's first sound track, decoded to mono samples — what the
//! speech engines read (see agent-docs/capabilities/speech/CONTRACT.md).
//! symphonia demuxes audio and video containers (MP4/MOV/M4V, WebM and
//! Matroska, Ogg, WAV, ...) and decodes most codecs; Opus, which symphonia
//! lacks, goes through the pure-Rust opus-decoder crate.

use std::fs::File;
use std::io::Seek;
use std::path::Path;

use opus_decoder::OpusMultistreamDecoder;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

use crate::speech::SpeechError;

/// Opus decodes to any of its rates directly; speech engines want 16 kHz.
const OPUS_RATE: u32 = 16_000;
/// The longest Opus packet, 120 ms, in samples per channel at `OPUS_RATE`.
const OPUS_MAX_FRAME: usize = OPUS_RATE as usize * 120 / 1000;

enum Codec {
    Symphonia(Box<dyn Decoder>),
    Opus { decoder: OpusMultistreamDecoder, channels: usize, skip: usize },
}

pub struct Sound {
    format: Box<dyn FormatReader>,
    codec: Codec,
    track: u32,
    time_base: Option<TimeBase>,
    pub sample_rate: u32,
    /// Length in seconds, when the container says (browser-recorded WebM often doesn't).
    pub seconds: Option<f64>,
    decoded: u64,
    started: bool,
    /// Shares the reader's file position, so progress can fall back to bytes read.
    position: File,
    bytes: u64,
}

fn unreadable(e: DecodeError) -> SpeechError {
    SpeechError::Other(format!("couldn't read the audio: {e}"))
}

/// Channel count, pre-skip (at 48 kHz) and channel mapping from an OpusHead
/// (RFC 7845 §5.1) — Ogg's identification header and Matroska's CodecPrivate.
fn opus_decoder(head: &[u8]) -> Result<(OpusMultistreamDecoder, usize, usize), SpeechError> {
    let invalid = || SpeechError::Other("couldn't read the audio: invalid Opus header".to_string());
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return Err(invalid());
    }
    let channels = usize::from(head[9]);
    let pre_skip = usize::from(u16::from_le_bytes([head[10], head[11]]));
    let (streams, coupled, mapping) = match head[18] {
        0 => (1, usize::from(channels == 2), (0..channels as u8).collect::<Vec<_>>()),
        _ => {
            let table = head.get(19..21 + channels).ok_or_else(invalid)?;
            (usize::from(table[0]), usize::from(table[1]), table[2..].to_vec())
        }
    };
    let decoder = OpusMultistreamDecoder::new(OPUS_RATE, channels, streams, coupled, &mapping)
        .map_err(|e| SpeechError::Other(format!("couldn't read the audio: {e}")))?;
    Ok((decoder, channels, pre_skip * OPUS_RATE as usize / 48_000))
}

impl Sound {
    /// Opens the first sound track. A file with none — a silent video —
    /// is `NotFound`; one that can't be read or decoded is `Other`.
    pub fn open(path: &Path) -> Result<Self, SpeechError> {
        let file = File::open(path).map_err(|e| SpeechError::Other(format!("couldn't open the audio: {e}")))?;
        let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
        // A duplicated handle shares the file position with the reader's.
        let position = file.try_clone().map_err(|e| SpeechError::Other(format!("couldn't open the audio: {e}")))?;
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe()
            .format(&hint, MediaSourceStream::new(Box::new(file), Default::default()), &FormatOptions::default(), &MetadataOptions::default())
            .map_err(unreadable)?
            .format;
        // Video tracks carry no sample rate.
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.sample_rate.is_some())
            .ok_or_else(|| SpeechError::NotFound("the file has no sound track".to_string()))?;
        let params = &track.codec_params;
        let (codec, sample_rate) = if params.codec == CODEC_TYPE_OPUS {
            let (decoder, channels, skip) = opus_decoder(params.extra_data.as_deref().unwrap_or_default())?;
            (Codec::Opus { decoder, channels, skip }, OPUS_RATE)
        } else {
            let decoder = symphonia::default::get_codecs().make(params, &DecoderOptions::default()).map_err(unreadable)?;
            (Codec::Symphonia(decoder), params.sample_rate.unwrap_or_default())
        };
        let seconds = params.n_frames.map(|frames| match params.time_base {
            Some(base) => {
                let time = base.calc_time(frames);
                time.seconds as f64 + time.frac
            }
            None => frames as f64 / f64::from(sample_rate),
        });
        Ok(Self {
            track: track.id,
            time_base: params.time_base,
            format,
            codec,
            sample_rate,
            seconds: seconds.filter(|s| *s > 0.0),
            decoded: 0,
            started: false,
            position,
            bytes,
        })
    }

    /// The next run of mono samples at `sample_rate`; `None` at the end.
    pub fn next(&mut self) -> Result<Option<Vec<f32>>, SpeechError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(unreadable(e)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let mut mono = match &mut self.codec {
                Codec::Symphonia(decoder) => match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let channels = spec.channels.count().max(1);
                        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                        buffer.copy_interleaved_ref(decoded);
                        downmix(buffer.samples(), channels)
                    }
                    // A damaged packet is skipped, like players do.
                    Err(DecodeError::DecodeError(_)) => continue,
                    Err(e) => return Err(SpeechError::Other(format!("couldn't decode the audio: {e}"))),
                },
                Codec::Opus { decoder, channels, skip } => {
                    let mut pcm = vec![0.0; OPUS_MAX_FRAME * *channels];
                    let Ok(frames) = decoder.decode_float(&packet.data, &mut pcm, false) else { continue };
                    let mut mono = downmix(&pcm[..frames * *channels], *channels);
                    let skipped = (*skip).min(mono.len());
                    mono.drain(..skipped);
                    *skip -= skipped;
                    mono
                }
            };
            // A track that starts late keeps its place: times count from the start of the file.
            if !self.started {
                self.started = true;
                if let Some(base) = self.time_base {
                    let start = base.calc_time(packet.ts());
                    let lead = ((start.seconds as f64 + start.frac) * f64::from(self.sample_rate)) as usize;
                    mono.splice(0..0, std::iter::repeat_n(0.0, lead));
                }
            }
            self.decoded += mono.len() as u64;
            return Ok(Some(mono));
        }
    }

    /// How far through the sound decoding has got, 0–1: by length when the
    /// container gives one, else by bytes read.
    pub fn progress(&self) -> f64 {
        let fraction = match self.seconds {
            Some(seconds) => self.decoded as f64 / (seconds * f64::from(self.sample_rate)),
            None if self.bytes > 0 => {
                (&self.position).stream_position().map_or(0.0, |read| read as f64 / self.bytes as f64)
            }
            None => 0.0,
        };
        fraction.min(1.0)
    }
}

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    interleaved.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech").join(name)
    }

    /// Every fixture is the same 3.2 s spoken sentence.
    fn decode(name: &str) -> (Sound, Vec<f32>) {
        let mut sound = Sound::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let mut samples = Vec::new();
        let mut last = 0.0;
        while let Some(chunk) = sound.next().unwrap() {
            samples.extend(chunk);
            assert!(sound.progress() >= last, "{name}: progress went back");
            last = sound.progress();
        }
        (sound, samples)
    }

    #[test]
    fn decodes_the_sound_of_audio_and_video_files() {
        let names = ["lecture.mp4", "lecture.mov", "lecture.m4v", "lecture.webm", "lecture.mkv", "recorded.webm", "note.opus", "note.ogg"];
        for name in names {
            let (sound, samples) = decode(name);
            let seconds = samples.len() as f64 / f64::from(sound.sample_rate);
            assert!((3.1..3.5).contains(&seconds), "{name}: {seconds} s");
            let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
            assert!(peak > 0.1, "{name}: peak {peak} — decoded silence");
            assert!(sound.progress() > 0.9, "{name}: progress ended at {}", sound.progress());
        }
    }

    #[test]
    fn opus_decodes_straight_to_16_khz_and_knows_its_length() {
        for name in ["lecture.webm", "note.opus"] {
            let (sound, _) = decode(name);
            assert_eq!(sound.sample_rate, OPUS_RATE, "{name}");
            assert!(sound.seconds.is_some_and(|s| (3.1..3.5).contains(&s)), "{name}: {:?}", sound.seconds);
        }
        // Recorded in a browser: no duration, so progress comes from bytes read.
        assert_eq!(Sound::open(&fixture("recorded.webm")).unwrap().seconds, None);
    }

    #[test]
    fn a_video_without_sound_is_not_found() {
        assert!(matches!(Sound::open(&fixture("silent.mp4")), Err(SpeechError::NotFound(_))));
    }

    #[test]
    fn a_file_that_isnt_media_is_unreadable() {
        let path = std::env::temp_dir().join(format!("chain-sound-not-media-{}.webm", std::process::id()));
        std::fs::write(&path, b"not media at all").unwrap();
        let result = Sound::open(&path);
        std::fs::remove_file(&path).ok();
        assert!(matches!(result, Err(SpeechError::Other(_))));
    }
}
