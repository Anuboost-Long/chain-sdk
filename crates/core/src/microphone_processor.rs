//! Cleans up the microphone before mixing, with SpeexDSP (vendor/speexdsp,
//! built by build.rs): for "both", its adaptive echo canceller with the
//! system-audio tap as the reference; then its preprocessor for residual
//! echo and noise suppression. With echo cancellation it also tells
//! whether someone is speaking into the microphone — the mixer only
//! raises the microphone then, so leftover echo is never turned up. Why Speex and what each
//! setting does: agent-docs/capabilities/audio-recorder/research/PROCESSING.md.

use std::ffi::{c_int, c_void};

#[repr(C)]
struct SpeexEchoState {
    _private: [u8; 0],
}

#[repr(C)]
struct SpeexPreprocessState {
    _private: [u8; 0],
}

extern "C" {
    fn speex_echo_state_init(frame_size: c_int, filter_length: c_int) -> *mut SpeexEchoState;
    fn speex_echo_state_destroy(state: *mut SpeexEchoState);
    fn speex_echo_ctl(state: *mut SpeexEchoState, request: c_int, value: *mut c_void) -> c_int;
    fn speex_echo_cancellation(state: *mut SpeexEchoState, recorded: *const i16, played: *const i16, out: *mut i16);
    fn speex_preprocess_state_init(frame_size: c_int, sample_rate: c_int) -> *mut SpeexPreprocessState;
    fn speex_preprocess_state_destroy(state: *mut SpeexPreprocessState);
    fn speex_preprocess_ctl(state: *mut SpeexPreprocessState, request: c_int, value: *mut c_void) -> c_int;
    fn speex_preprocess_run(state: *mut SpeexPreprocessState, samples: *mut i16) -> c_int;
}

// From speex_echo.h / speex_preprocess.h.
const SPEEX_ECHO_SET_SAMPLING_RATE: c_int = 24;
const SPEEX_PREPROCESS_SET_NOISE_SUPPRESS: c_int = 18;
const SPEEX_PREPROCESS_SET_ECHO_STATE: c_int = 24;

/// How long after the computer plays a sound the microphone can still hear
/// it: output and input latency, plus the room's reverberation.
const ECHO_TAIL_SECONDS: f64 = 0.2;
/// Speex's own default: steady noise drops a lot, voices stay natural.
/// Stronger settings leave "musical noise" artefacts.
const NOISE_SUPPRESS_DB: i32 = -15;
/// The canceller takes 20–30 dB off echo alone; a voice it has no
/// reference for comes through almost whole. So the microphone counts as
/// someone speaking while less than this much was taken off.
const SPEAKING_REMOVED_DB: f32 = 15.0;
/// How quickly the before/after levels follow the sound, per frame.
const LEVEL_SMOOTHING: f32 = 0.3;

/// What to do to the microphone. Echo cancellation needs the system audio;
/// gain control is the mixer's (`audio_recorder::Mixer`), steered by this
/// processor's speech detection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Processing {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
}

impl Processing {
    pub fn any(self) -> bool {
        self.echo_cancellation || self.noise_suppression || self.auto_gain_control
    }
}

pub struct MicrophoneProcessor {
    /// Null without echo cancellation.
    echo: *mut SpeexEchoState,
    preprocess: *mut SpeexPreprocessState,
    frame: usize,
    /// Smoothed power of the microphone before and after cancellation.
    heard_power: f32,
    cleaned_power: f32,
    pending_microphone: Vec<f32>,
    pending_system: Vec<f32>,
    microphone: Vec<i16>,
    system: Vec<i16>,
    cleaned: Vec<i16>,
}

impl MicrophoneProcessor {
    pub fn new(sample_rate: f64, processing: Processing) -> Self {
        // Speex works on fixed frames; ~10 ms, a power of two for its FFT.
        let frame = ((sample_rate / 100.0).round() as usize).next_power_of_two();
        let tail = ((sample_rate * ECHO_TAIL_SECONDS) as usize).div_ceil(frame) * frame;
        let mut rate = sample_rate.round() as c_int;
        // Noise suppression can't be switched off on its own: turning the
        // preprocessor's denoiser off also stops its residual echo
        // suppression. 0 dB leaves the voice untouched instead.
        let mut noise_suppress_db: i32 = if processing.noise_suppression { NOISE_SUPPRESS_DB } else { 0 };
        // SAFETY: init returns owned states freed once in Drop; the ctl
        // values are live locals of the type each request reads.
        unsafe {
            let preprocess = speex_preprocess_state_init(frame as c_int, rate);
            speex_preprocess_ctl(
                preprocess,
                SPEEX_PREPROCESS_SET_NOISE_SUPPRESS,
                (&mut noise_suppress_db as *mut i32).cast(),
            );
            let echo = if processing.echo_cancellation {
                let echo = speex_echo_state_init(frame as c_int, tail as c_int);
                speex_echo_ctl(echo, SPEEX_ECHO_SET_SAMPLING_RATE, (&mut rate as *mut c_int).cast());
                speex_preprocess_ctl(preprocess, SPEEX_PREPROCESS_SET_ECHO_STATE, echo.cast());
                echo
            } else {
                std::ptr::null_mut()
            };
            Self {
                echo,
                preprocess,
                frame,
                heard_power: 0.0,
                cleaned_power: 0.0,
                pending_microphone: Vec::new(),
                pending_system: Vec::new(),
                microphone: vec![0; frame],
                system: vec![0; frame],
                cleaned: vec![0; frame],
            }
        }
    }

    /// Takes blocks of any length — with `system` sample-aligned for
    /// "both" — and sets `cleaned` (and `system_out`) to whole frames
    /// processed so far, still aligned. Up to one frame waits for the next
    /// call; what's left at the end of a recording (under ~10 ms) is dropped.
    /// The system audio itself passes through untouched. Returns whether
    /// someone is speaking into the microphone, as far as echo
    /// cancellation can tell: echo it left over doesn't count. Without
    /// echo cancellation it can't tell, and says yes.
    pub fn process(
        &mut self,
        microphone: &[f32],
        system: Option<&[f32]>,
        cleaned: &mut Vec<f32>,
        system_out: &mut Vec<f32>,
    ) -> bool {
        self.pending_microphone.extend_from_slice(microphone);
        let mut available = self.pending_microphone.len();
        if let Some(system) = system {
            self.pending_system.extend_from_slice(system);
            available = available.min(self.pending_system.len());
        }
        cleaned.clear();
        system_out.clear();
        let ready = available / self.frame * self.frame;
        for start in (0..ready).step_by(self.frame) {
            let range = start..start + self.frame;
            to_i16(&self.pending_microphone[range.clone()], &mut self.cleaned);
            if system.is_some() {
                system_out.extend_from_slice(&self.pending_system[range.clone()]);
            }
            // SAFETY: every buffer holds exactly `frame` samples, the frame
            // size both states were created with.
            unsafe {
                if !self.echo.is_null() && system.is_some() {
                    to_i16(&self.pending_system[range], &mut self.system);
                    self.microphone.copy_from_slice(&self.cleaned);
                    speex_echo_cancellation(self.echo, self.microphone.as_ptr(), self.system.as_ptr(), self.cleaned.as_mut_ptr());
                }
                speex_preprocess_run(self.preprocess, self.cleaned.as_mut_ptr());
            }
            follow(&mut self.heard_power, &self.microphone);
            follow(&mut self.cleaned_power, &self.cleaned);
            cleaned.extend(self.cleaned.iter().map(|&s| f32::from(s) / 32768.0));
        }
        self.pending_microphone.drain(..ready);
        self.pending_system.drain(..ready.min(self.pending_system.len()));
        self.echo.is_null() || self.cleaned_power * 10f32.powf(SPEAKING_REMOVED_DB / 10.0) > self.heard_power
    }
}

fn follow(power: &mut f32, frame: &[i16]) {
    let frame_power = frame.iter().map(|&s| f32::from(s) * f32::from(s)).sum::<f32>() / frame.len() as f32;
    *power += (frame_power - *power) * LEVEL_SMOOTHING;
}

fn to_i16(samples: &[f32], out: &mut [i16]) {
    for (o, s) in out.iter_mut().zip(samples) {
        *o = (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
    }
}

impl Drop for MicrophoneProcessor {
    fn drop(&mut self) {
        // SAFETY: created in `new`, destroyed once; the preprocessor only
        // borrows the echo state, so it goes first.
        unsafe {
            speex_preprocess_state_destroy(self.preprocess);
            if !self.echo.is_null() {
                speex_echo_state_destroy(self.echo);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;
    const ECHO: Processing = Processing { echo_cancellation: true, noise_suppression: false, auto_gain_control: false };

    /// Deterministic white-ish noise, standing in for a lecture.
    fn noise(seed: u32, frames: usize, amplitude: f32) -> Vec<f32> {
        let mut state = seed;
        (0..frames)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                amplitude * ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    /// What a laptop microphone hears of `played`: 30 ms later, quieter, and
    /// smeared by a short decaying room response.
    fn echo_of(played: &[f32]) -> Vec<f32> {
        let delay = (RATE * 0.03) as usize;
        let room: Vec<f32> = (0..480).map(|i| 0.05 * (-(i as f32) / 80.0).exp() * if i % 7 == 0 { -1.0 } else { 1.0 }).collect();
        (0..played.len())
            .map(|n| room.iter().enumerate().filter_map(|(k, g)| n.checked_sub(delay + k).map(|i| g * played[i])).sum())
            .collect()
    }

    fn power(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// Feeds `microphone`/`system` in 441-frame blocks (deliberately not a
    /// frame multiple, like a real IO cycle) and returns the cleaned microphone.
    fn run(processing: Processing, microphone: &[f32], system: Option<&[f32]>) -> Vec<f32> {
        let mut processor = MicrophoneProcessor::new(RATE, processing);
        let (mut cleaned, mut system_out, mut all) = (Vec::new(), Vec::new(), Vec::new());
        for (i, m) in microphone.chunks(441).enumerate() {
            let s = system.map(|s| &s[i * 441..(i * 441 + m.len())]);
            processor.process(m, s, &mut cleaned, &mut system_out);
            assert_eq!(system_out.len(), if s.is_some() { cleaned.len() } else { 0 });
            all.extend_from_slice(&cleaned);
        }
        all
    }

    fn tone(amplitude: f32, hz: f32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| amplitude * (i as f32 * 2.0 * std::f32::consts::PI * hz / RATE as f32).sin()).collect()
    }

    /// A tone that comes and goes every half second, like phrases of speech.
    fn phrases(amplitude: f32, seconds: usize) -> Vec<f32> {
        let half = RATE as usize / 2;
        tone(amplitude, 300.0, RATE as usize * seconds)
            .into_iter()
            .enumerate()
            .map(|(i, s)| if (i / half) % 2 == 0 { s } else { 0.0 })
            .collect()
    }

    fn db(a: f32, b: f32) -> f32 {
        10.0 * (a / b).log10()
    }

    #[test]
    fn removes_most_of_the_echo_once_adapted() {
        let seconds = 8;
        let played = noise(1, RATE as usize * seconds, 0.3);
        let heard = echo_of(&played);
        let cleaned = run(ECHO, &heard, Some(&played));
        // Judge the last two seconds, after adapting.
        let tail = cleaned.len() - RATE as usize * 2;
        let reduction_db = 10.0 * (power(&heard[tail..cleaned.len()]) / power(&cleaned[tail..])).log10();
        assert!(reduction_db > 20.0, "echo only reduced by {reduction_db:.1} dB");
    }

    #[test]
    fn keeps_the_voice_when_the_computer_is_silent() {
        let voice: Vec<f32> = (0..RATE as usize * 3).map(|i| 0.2 * (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / RATE as f32).sin()).collect();
        let silence = vec![0.0; voice.len()];
        let cleaned = run(ECHO, &voice, Some(&silence));
        let from = RATE as usize;
        let kept_db = 10.0 * (power(&cleaned[from..]) / power(&voice[from..cleaned.len()])).log10();
        assert!(kept_db.abs() < 1.0, "voice changed by {kept_db:.1} dB");
    }

    #[test]
    fn keeps_the_voice_over_the_computer() {
        let seconds = 8;
        let played = noise(2, RATE as usize * seconds, 0.3);
        let voice: Vec<f32> = (0..played.len()).map(|i| 0.1 * (i as f32 * 2.0 * std::f32::consts::PI * 300.0 / RATE as f32).sin()).collect();
        let heard: Vec<f32> = echo_of(&played).iter().zip(&voice).map(|(e, v)| e + v).collect();
        let cleaned = run(ECHO, &heard, Some(&played));
        let tail = cleaned.len() - RATE as usize * 2;
        let kept_db = 10.0 * (power(&cleaned[tail..]) / power(&voice[tail..cleaned.len()])).log10();
        assert!(kept_db > -6.0, "voice dropped by {kept_db:.1} dB");
    }

    #[test]
    fn holds_back_less_than_a_frame() {
        let mut processor = MicrophoneProcessor::new(RATE, ECHO);
        let (mut cleaned, mut system_out) = (Vec::new(), Vec::new());
        processor.process(&[0.0; 300], Some(&[0.0; 300]), &mut cleaned, &mut system_out);
        assert!(cleaned.is_empty());
        processor.process(&[0.0; 300], Some(&[0.0; 300]), &mut cleaned, &mut system_out);
        assert_eq!((cleaned.len(), system_out.len()), (512, 512));
        assert_eq!(processor.pending_microphone.len(), 88);
    }

    #[test]
    fn noise_suppression_quiets_steady_noise_between_phrases() {
        let seconds = 8;
        let voice = phrases(0.2, seconds);
        let fan = noise(4, voice.len(), 0.02);
        let heard: Vec<f32> = voice.iter().zip(&fan).map(|(v, n)| v + n).collect();
        let processing = Processing { noise_suppression: true, ..Processing::default() };
        let cleaned = run(processing, &heard, None);
        // The last second: 7–7.5 s is a phrase, 7.5–8 s a gap.
        let half = RATE as usize / 2;
        let (phrase, gap) = (RATE as usize * 7 - 2048, RATE as usize * 7 + half);
        let quiet = gap + 4096..cleaned.len().min(gap + half - 4096);
        let noise_drop = db(power(&heard[quiet.clone()]), power(&cleaned[quiet]));
        let voice_kept = db(power(&cleaned[phrase + 2048..phrase + half]), power(&voice[phrase + 2048..phrase + half]));
        assert!(noise_drop > 8.0, "noise only dropped {noise_drop:.1} dB");
        assert!(voice_kept > -3.0, "voice dropped {voice_kept:.1} dB");
    }

    #[test]
    fn noise_suppression_off_leaves_the_microphone_alone() {
        let heard = phrases(0.2, 3);
        let cleaned = run(Processing { echo_cancellation: true, ..Processing::default() }, &heard, None);
        let from = RATE as usize;
        assert!(db(power(&cleaned[from..]), power(&heard[from..cleaned.len()])).abs() < 1.0);
    }

    #[test]
    fn detects_a_voice_but_not_the_echo_left_over() {
        let seconds = 10;
        let played = noise(5, RATE as usize * seconds, 0.3);
        let echo = echo_of(&played);
        let voice = phrases(0.05, seconds);
        let mut processor = MicrophoneProcessor::new(RATE, ECHO);
        let (mut cleaned, mut system_out) = (Vec::new(), Vec::new());
        let (mut echo_only, mut voiced) = ((0, 0), (0, 0));
        for (i, (e, v)) in echo.chunks(512).zip(voice.chunks(512)).enumerate() {
            let second = i * 512 / RATE as usize;
            // First 5 s: only the computer, after adapting; then a voice too.
            let heard: Vec<f32> = if second < 5 { e.to_vec() } else { e.iter().zip(v).map(|(e, v)| e + v).collect() };
            let speaking = processor.process(&heard, Some(&played[i * 512..i * 512 + e.len()]), &mut cleaned, &mut system_out);
            let in_phrase = (i * 512 / (RATE as usize / 2)) % 2 == 0;
            match second {
                2..=4 => echo_only = (echo_only.0 + usize::from(speaking), echo_only.1 + 1),
                6.. if in_phrase => voiced = (voiced.0 + usize::from(speaking), voiced.1 + 1),
                _ => {}
            }
        }
        let echo_rate = echo_only.0 as f32 / echo_only.1 as f32;
        let voice_rate = voiced.0 as f32 / voiced.1 as f32;
        assert!(echo_rate < 0.1, "echo taken for speech {:.0}% of the time", echo_rate * 100.0);
        assert!(voice_rate > 0.8, "voice found only {:.0}% of the time", voice_rate * 100.0);
    }
}
