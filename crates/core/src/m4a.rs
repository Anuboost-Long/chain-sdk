//! Streams mono f32 samples into an AAC `.m4a` with the OS's own encoder,
//! so a long recording never sits in memory and nothing is added to the
//! app's size. macOS: AudioToolbox's ExtAudioFile (plain C, so no Swift or
//! objc bridge). Elsewhere `create` is `Unsupported` — Windows would use
//! Media Foundation's sink writer, see
//! agent-docs/capabilities/tts/research/WINDOWS.md.

use std::path::Path;

#[derive(Debug)]
pub enum M4aError {
    Unsupported(String),
    Failed(String),
}

/// Mono speech needs nothing more; a minute is about 0.5 MB.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BIT_RATE: u32 = 64_000;

#[cfg(target_os = "macos")]
pub use macos::M4aWriter;

#[cfg(not(target_os = "macos"))]
pub struct M4aWriter;

#[cfg(not(target_os = "macos"))]
impl M4aWriter {
    pub fn create(_path: &Path, _sample_rate: u32) -> Result<Self, M4aError> {
        Err(M4aError::Unsupported("AAC encoding isn't implemented on this platform yet".to_string()))
    }

    pub fn write(&mut self, _samples: &[f32]) -> Result<(), M4aError> {
        unreachable!("create never succeeds here")
    }

    pub fn finish(self) -> Result<(), M4aError> {
        unreachable!("create never succeeds here")
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;

    use super::*;

    type OSStatus = i32;
    type ExtAudioFileRef = *mut c_void;
    type CFURLRef = *const c_void;

    #[repr(C)]
    struct AudioStreamBasicDescription {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C)]
    struct AudioBuffer {
        number_channels: u32,
        data_byte_size: u32,
        data: *mut c_void,
    }

    #[repr(C)]
    struct AudioBufferList {
        number_buffers: u32,
        buffers: [AudioBuffer; 1],
    }

    const fn four_cc(code: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*code)
    }

    const FILE_TYPE_M4A: u32 = four_cc(b"m4af");
    const FORMAT_AAC: u32 = four_cc(b"aac ");
    const FORMAT_LINEAR_PCM: u32 = four_cc(b"lpcm");
    const FLAG_IS_FLOAT: u32 = 1;
    const FLAG_IS_PACKED: u32 = 1 << 3;
    const FILE_FLAG_ERASE: u32 = 1;
    const PROPERTY_CLIENT_FORMAT: u32 = four_cc(b"cfmt");
    const PROPERTY_AUDIO_CONVERTER: u32 = four_cc(b"acnv");
    const PROPERTY_CONVERTER_CONFIG: u32 = four_cc(b"acfg");
    const CONVERTER_ENCODE_BIT_RATE: u32 = four_cc(b"brat");
    const CONVERTER_APPLICABLE_BIT_RATES: u32 = four_cc(b"aebr");

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn ExtAudioFileCreateWithURL(
            url: CFURLRef,
            file_type: u32,
            stream_desc: *const AudioStreamBasicDescription,
            channel_layout: *const c_void,
            flags: u32,
            out_file: *mut ExtAudioFileRef,
        ) -> OSStatus;
        fn ExtAudioFileSetProperty(file: ExtAudioFileRef, id: u32, size: u32, data: *const c_void) -> OSStatus;
        fn ExtAudioFileGetProperty(file: ExtAudioFileRef, id: u32, size: *mut u32, data: *mut c_void) -> OSStatus;
        fn ExtAudioFileWrite(file: ExtAudioFileRef, frames: u32, data: *const AudioBufferList) -> OSStatus;
        fn ExtAudioFileDispose(file: ExtAudioFileRef) -> OSStatus;
        fn AudioConverterSetProperty(converter: *mut c_void, id: u32, size: u32, data: *const c_void) -> OSStatus;
        fn AudioConverterGetPropertyInfo(converter: *mut c_void, id: u32, size: *mut u32, writable: *mut u8) -> OSStatus;
        fn AudioConverterGetProperty(converter: *mut c_void, id: u32, size: *mut u32, data: *mut c_void) -> OSStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            length: isize,
            is_directory: u8,
        ) -> CFURLRef;
        fn CFRelease(object: *const c_void);
    }

    /// OSStatus values are usually four-character codes (`'fmt?'`); show
    /// them that way when they are, since that's what Apple's docs list.
    fn describe(status: OSStatus) -> String {
        let bytes = status.to_be_bytes();
        if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
            format!("'{}'", String::from_utf8_lossy(&bytes))
        } else {
            status.to_string()
        }
    }

    fn check(status: OSStatus, what: &str) -> Result<(), M4aError> {
        if status == 0 {
            Ok(())
        } else {
            Err(M4aError::Failed(format!("couldn't {what} (OSStatus {})", describe(status))))
        }
    }

    #[repr(C)]
    struct AudioValueRange {
        minimum: f64,
        maximum: f64,
    }

    /// `BIT_RATE`, or the highest the encoder allows below it: at 8/16 kHz
    /// mono it can't do 64 kbps and fails every write ('!dat') if asked to.
    unsafe fn applicable_bit_rate(converter: *mut c_void) -> u32 {
        let mut size = 0u32;
        if AudioConverterGetPropertyInfo(converter, CONVERTER_APPLICABLE_BIT_RATES, &mut size, std::ptr::null_mut()) != 0 {
            return BIT_RATE;
        }
        let mut ranges: Vec<AudioValueRange> = Vec::with_capacity(size as usize / std::mem::size_of::<AudioValueRange>());
        if AudioConverterGetProperty(converter, CONVERTER_APPLICABLE_BIT_RATES, &mut size, ranges.as_mut_ptr().cast()) != 0 {
            return BIT_RATE;
        }
        ranges.set_len(size as usize / std::mem::size_of::<AudioValueRange>());
        ranges
            .iter()
            .map(|range| range.maximum as u32)
            .filter(|&rate| rate <= BIT_RATE)
            .max()
            .unwrap_or(BIT_RATE)
    }

    pub struct M4aWriter {
        file: ExtAudioFileRef,
    }

    impl M4aWriter {
        pub fn create(path: &Path, sample_rate: u32) -> Result<Self, M4aError> {
            let aac = AudioStreamBasicDescription {
                sample_rate: sample_rate as f64,
                format_id: FORMAT_AAC,
                format_flags: 0,
                bytes_per_packet: 0,
                frames_per_packet: 1024,
                bytes_per_frame: 0,
                channels_per_frame: 1,
                bits_per_channel: 0,
                reserved: 0,
            };
            let samples = AudioStreamBasicDescription {
                sample_rate: sample_rate as f64,
                format_id: FORMAT_LINEAR_PCM,
                format_flags: FLAG_IS_FLOAT | FLAG_IS_PACKED,
                bytes_per_packet: 4,
                frames_per_packet: 1,
                bytes_per_frame: 4,
                channels_per_frame: 1,
                bits_per_channel: 32,
                reserved: 0,
            };
            let path = path.as_os_str().as_bytes();
            // SAFETY: plain CF/AudioToolbox calls; the URL is released
            // once the file holds its own reference.
            unsafe {
                let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), path.as_ptr(), path.len() as isize, 0);
                if url.is_null() {
                    return Err(M4aError::Failed("couldn't make a URL for the output file".to_string()));
                }
                let mut file: ExtAudioFileRef = std::ptr::null_mut();
                let status =
                    ExtAudioFileCreateWithURL(url, FILE_TYPE_M4A, &aac, std::ptr::null(), FILE_FLAG_ERASE, &mut file);
                CFRelease(url);
                check(status, "create the audio file")?;
                let writer = Self { file };
                check(
                    ExtAudioFileSetProperty(
                        file,
                        PROPERTY_CLIENT_FORMAT,
                        std::mem::size_of::<AudioStreamBasicDescription>() as u32,
                        (&samples as *const AudioStreamBasicDescription).cast(),
                    ),
                    "set the sample format",
                )?;
                writer.set_bit_rate();
                Ok(writer)
            }
        }

        /// Best effort: the encoder keeps its default rate if the
        /// converter can't be reached.
        unsafe fn set_bit_rate(&self) {
            let mut converter: *mut c_void = std::ptr::null_mut();
            let mut size = std::mem::size_of::<*mut c_void>() as u32;
            if ExtAudioFileGetProperty(self.file, PROPERTY_AUDIO_CONVERTER, &mut size, (&mut converter as *mut *mut c_void).cast())
                != 0
                || converter.is_null()
            {
                return;
            }
            let bit_rate = applicable_bit_rate(converter);
            if AudioConverterSetProperty(converter, CONVERTER_ENCODE_BIT_RATE, 4, (&bit_rate as *const u32).cast()) == 0 {
                // A null config tells ExtAudioFile to apply the converter change.
                let config: *const c_void = std::ptr::null();
                ExtAudioFileSetProperty(
                    self.file,
                    PROPERTY_CONVERTER_CONFIG,
                    std::mem::size_of::<*const c_void>() as u32,
                    (&config as *const *const c_void).cast(),
                );
            }
        }

        pub fn write(&mut self, samples: &[f32]) -> Result<(), M4aError> {
            if samples.is_empty() {
                return Ok(());
            }
            let buffers = AudioBufferList {
                number_buffers: 1,
                buffers: [AudioBuffer {
                    number_channels: 1,
                    data_byte_size: std::mem::size_of_val(samples) as u32,
                    data: samples.as_ptr().cast_mut().cast(),
                }],
            };
            // SAFETY: the buffer list points at `samples`, alive for the call;
            // ExtAudioFileWrite only reads it.
            check(unsafe { ExtAudioFileWrite(self.file, samples.len() as u32, &buffers) }, "encode audio")
        }

        /// Flushes the encoder and writes the file's index. Without it the
        /// file isn't playable.
        pub fn finish(self) -> Result<(), M4aError> {
            let file = self.file;
            std::mem::forget(self);
            // SAFETY: `file` is open and disposed exactly once (Drop skipped).
            check(unsafe { ExtAudioFileDispose(file) }, "finish the audio file")
        }
    }

    impl Drop for M4aWriter {
        fn drop(&mut self) {
            // SAFETY: open until here; the caller deletes the unfinished file.
            unsafe { ExtAudioFileDispose(self.file) };
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn writes_a_playable_m4a() {
        let path = std::env::temp_dir().join(format!("chain-m4a-test-{}.m4a", std::process::id()));
        let tone: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.3).collect();
        let mut writer = M4aWriter::create(&path, 24_000).unwrap();
        writer.write(&tone).unwrap();
        writer.write(&tone).unwrap();
        writer.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        // Two seconds at 64 kbps is ~16 KB; WAV would be 96 KB.
        assert!(bytes.len() < 40_000, "{} bytes", bytes.len());
    }

    #[test]
    fn encodes_at_telephone_rates() {
        for rate in [8_000, 16_000] {
            let path = std::env::temp_dir().join(format!("chain-m4a-test-{}-{rate}.m4a", std::process::id()));
            let mut writer = M4aWriter::create(&path, rate).unwrap();
            writer.write(&vec![0.1; rate as usize]).unwrap();
            writer.finish().unwrap();
            std::fs::remove_file(&path).unwrap();
        }
    }
}
