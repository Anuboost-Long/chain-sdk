//! Debugging aid for the audio recorder: with `CHAIN_RECORDER_TRACKS_DIR`
//! set in the app's environment, each recording also writes its separate
//! sources there as 32-bit float WAVs, sample-aligned — the raw microphone,
//! the processed microphone, the microphone as it enters the mix (after the
//! mixer's levelling) and the system audio. The mixed file alone can't show
//! how much echo or noise was removed. Not part of the contract.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub struct RecorderTracks {
    microphone: WavWriter,
    processed: WavWriter,
    microphone_in_mix: WavWriter,
    system: WavWriter,
}

impl RecorderTracks {
    /// None unless the environment variable names a folder; a folder that
    /// can't be written to is reported and skipped, never failing the take.
    pub fn from_env(sample_rate: f64) -> Option<Self> {
        let dir = std::env::var_os("CHAIN_RECORDER_TRACKS_DIR")?;
        let dir = Path::new(&dir);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let open = |name: &str| WavWriter::create(&dir.join(format!("{stamp}-{name}.wav")), sample_rate.round() as u32);
        let tracks = std::fs::create_dir_all(dir).and_then(|_| {
            Ok(Self {
                microphone: open("microphone")?,
                processed: open("microphone-processed")?,
                microphone_in_mix: open("microphone-in-mix")?,
                system: open("system")?,
            })
        });
        tracks.map_err(|e| eprintln!("chain recorder tracks: couldn't write to {}: {e}", dir.display())).ok()
    }

    pub fn write(&mut self, microphone: Option<&[f32]>, processed: Option<&[f32]>, gain: f32, system: Option<&[f32]>) {
        let result = (|| {
            if let Some(microphone) = microphone {
                self.microphone.write(microphone.iter().copied())?;
            }
            if let Some(processed) = processed {
                self.processed.write(processed.iter().copied())?;
                self.microphone_in_mix.write(processed.iter().map(|s| s * gain))?;
            }
            if let Some(system) = system {
                self.system.write(system.iter().copied())?;
            }
            std::io::Result::Ok(())
        })();
        if let Err(e) = result {
            eprintln!("chain recorder tracks: {e}");
        }
    }

    pub fn finish(self) {
        for track in [self.microphone, self.processed, self.microphone_in_mix, self.system] {
            if let Err(e) = track.finish() {
                eprintln!("chain recorder tracks: {e}");
            }
        }
    }
}

struct WavWriter {
    file: BufWriter<File>,
    samples: u32,
}

impl WavWriter {
    fn create(path: &Path, sample_rate: u32) -> std::io::Result<Self> {
        let mut writer = Self { file: BufWriter::new(File::create(path)?), samples: 0 };
        writer.header(sample_rate)?;
        Ok(writer)
    }

    /// Mono IEEE float; the two sizes are patched in by `finish`.
    fn header(&mut self, sample_rate: u32) -> std::io::Result<()> {
        let f = &mut self.file;
        f.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&3u16.to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?;
        f.write_all(&sample_rate.to_le_bytes())?;
        f.write_all(&(sample_rate * 4).to_le_bytes())?;
        f.write_all(&4u16.to_le_bytes())?;
        f.write_all(&32u16.to_le_bytes())?;
        f.write_all(b"data\0\0\0\0")
    }

    fn write(&mut self, samples: impl Iterator<Item = f32>) -> std::io::Result<()> {
        for sample in samples {
            self.file.write_all(&sample.to_le_bytes())?;
            self.samples += 1;
        }
        Ok(())
    }

    fn finish(mut self) -> std::io::Result<()> {
        let data = self.samples * 4;
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&(36 + data).to_le_bytes())?;
        self.file.seek(SeekFrom::Start(40))?;
        self.file.write_all(&data.to_le_bytes())?;
        self.file.flush()
    }
}
