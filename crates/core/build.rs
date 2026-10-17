//! Two native pieces for chain-core:
//!
//! - macOS: compiles swift/*.swift into one static library — the speech
//!   capability's SpeechAnalyzer bridge (src/speech.rs), vision's
//!   RecognizeDocumentsRequest bridge (src/vision.rs) and the audio
//!   recorder's Core Audio capture (src/audio_recorder.rs) and the window
//!   capability's drag regions (src/window.rs), the pdf capability's
//!   renderer (src/pdf.rs) and the share menu (src/share.rs). Needs
//!   `swiftc`, which every Mac that can build a Tauri app already has
//!   through the Xcode Command Line Tools.
//! - Every desktop target: fetches sherpa-onnx's official **no-TTS** static
//!   libraries (pinned version, SHA-256 checked) and links them — the
//!   engine behind model-based transcription (src/sherpa/). No-TTS means
//!   no espeak-ng (GPL-3); see agent-docs/capabilities/models/research/LICENSING.md.
//!   Set CHAIN_SHERPA_ONNX_ARCHIVE_DIR to a folder holding the archive to
//!   build offline.
//! - Every target: compiles the vendored SpeexDSP 1.2.1 echo canceller
//!   (vendor/speexdsp, BSD-3) — the audio recorder's echo cancellation,
//!   noise suppression and gain control (src/microphone_processor.rs). Only needs the C compiler a Tauri build
//!   already uses.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

// Swift concurrency ships in the OS from macOS 12, so that's the floor
// for the bridge; SpeechAnalyzer itself is checked at runtime (macOS 26).
const SWIFT_MIN_MACOS: &str = "12.0";

fn main() {
    println!("cargo:rerun-if-changed=swift/ChainSpeech.swift");
    println!("cargo:rerun-if-changed=swift/ChainVision.swift");
    println!("cargo:rerun-if-changed=swift/ChainRecorder.swift");
    println!("cargo:rerun-if-changed=swift/ChainWindow.swift");
    println!("cargo:rerun-if-changed=swift/ChainPdf.swift");
    println!("cargo:rerun-if-changed=swift/ChainShare.swift");
    println!("cargo:rerun-if-changed=swift/ChainAttention.swift");
    println!("cargo:rerun-if-env-changed=CHAIN_SHERPA_ONNX_ARCHIVE_DIR");
    println!("cargo::rustc-check-cfg=cfg(chain_no_sherpa)");
    link_sherpa_onnx();
    build_speexdsp();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_swift_bridge();
    }
}

const SHERPA_ONNX_VERSION: &str = "1.13.8";

/// (target os, target arch) → release archive and its SHA-256 (from the
/// GitHub release's own digests).
const SHERPA_ONNX_ARCHIVES: &[(&str, &str, &str, &str)] = &[
    ("macos", "aarch64", "osx-arm64-static-no-tts-lib", "3d7f9b8a496694af13d9802c33b8133231e397bdef302f543d19468765e83136"),
    ("macos", "x86_64", "osx-x64-static-no-tts-lib", "8ffc3ede9f997fec547b5c99b8e2073b8e04fd48a5f65e1a6d5d314ad0106ad9"),
    // MD = the dynamic CRT, which is what Rust's MSVC targets link by default.
    ("windows", "x86_64", "win-x64-static-MD-Release-no-tts-lib", "542348e56e827b59c6d249fd0dfd38dc34b7bd0c521a1ebe9eb6e2db154fa15d"),
    ("linux", "x86_64", "linux-x64-static-no-tts-lib", "36f2ebd0b9aa09248ff6461a30dcb7edfe7eecf2deb67dfb0ceb3aaf4906051f"),
];

/// The TTS-enabled builds, used only with the `tts` feature — they add
/// piper_phonemize and espeak-ng (GPL-3.0). An app opts in explicitly; see
/// agent-docs/capabilities/models/research/LICENSING.md.
const SHERPA_ONNX_TTS_ARCHIVES: &[(&str, &str, &str, &str)] = &[
    ("macos", "aarch64", "osx-arm64-static-lib", "9091bf160dc7fdacedbc906b212badf53c2993f4e5277a0e03998e96c31d60da"),
    ("macos", "x86_64", "osx-x64-static-lib", "a3f88da3e54c850a12d61431e73f8affcd1f13738b75b768847dd79541835b4b"),
    ("windows", "x86_64", "win-x64-static-MD-Release-lib", "a0f44cd91486e448c2be1f1d3662edb4f473ca3cb38a803cee158760cc588428"),
    ("linux", "x86_64", "linux-x64-static-lib", "e1fdc5b67530e15741ef897fa5ffff297056f3bf0c6d829a27af9225a4c4b5a6"),
];

/// Extra libraries the TTS-enabled builds need, after the shared ones.
const SHERPA_ONNX_TTS_LIBS: &[&str] = &["piper_phonemize", "espeak-ng", "ucd"];

/// Link order matters for static archives: dependents before dependencies.
/// Without the `tts` feature, piper_phonemize/espeak-ng/ucd are absent.
const SHERPA_ONNX_LIBS: &[&str] = &[
    "sherpa-onnx-c-api",
    "sherpa-onnx-core",
    "kaldi-decoder-core",
    "sherpa-onnx-kaldifst-core",
    "sherpa-onnx-fstfar",
    "sherpa-onnx-fst",
    "kaldi-native-fbank-core",
    "kissfft-float",
    "onnxruntime",
    "ssentencepiece_core",
];

fn link_sherpa_onnx() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let tts = env::var_os("CARGO_FEATURE_TTS").is_some();
    let archives = if tts { SHERPA_ONNX_TTS_ARCHIVES } else { SHERPA_ONNX_ARCHIVES };
    let Some(&(_, _, variant, sha256)) = archives.iter().find(|(o, a, _, _)| *o == os && *a == arch) else {
        // No engine for this target: src/sherpa/ compiles to UNSUPPORTED stubs.
        println!("cargo:rustc-cfg=chain_no_sherpa");
        return;
    };
    let name = format!("sherpa-onnx-v{SHERPA_ONNX_VERSION}-{variant}");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lib_dir = out_dir.join(&name).join("lib");

    if !lib_dir.join(marker_lib(&os)).exists() {
        let archive = out_dir.join(format!("{name}.tar.bz2"));
        match env::var_os("CHAIN_SHERPA_ONNX_ARCHIVE_DIR") {
            Some(dir) => {
                fs::copy(Path::new(&dir).join(archive.file_name().unwrap()), &archive)
                    .expect("CHAIN_SHERPA_ONNX_ARCHIVE_DIR doesn't hold the expected sherpa-onnx archive");
            }
            None => download(
                &format!("https://github.com/k2-fsa/sherpa-onnx/releases/download/v{SHERPA_ONNX_VERSION}/{name}.tar.bz2"),
                &archive,
            ),
        }
        let actual = sha256_of(&archive);
        assert_eq!(actual, sha256, "{} doesn't match its pinned SHA-256 — refusing to link it", archive.display());
        let file = fs::File::open(&archive).unwrap();
        tar::Archive::new(bzip2::read::BzDecoder::new(file)).unpack(&out_dir).expect("couldn't unpack sherpa-onnx");
        fs::remove_file(&archive).ok();
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    for lib in SHERPA_ONNX_LIBS {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    if tts {
        for lib in SHERPA_ONNX_TTS_LIBS {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }
    match os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
        }
        "linux" => {
            for lib in ["stdc++", "m", "pthread", "dl"] {
                println!("cargo:rustc-link-lib=dylib={lib}");
            }
        }
        _ => {}
    }
}

/// Unmodified upstream sources from the official 1.2.1 release tarball,
/// only the files the echo canceller and preprocessor need;
/// speexdsp_config_types.h is what its configure would generate. Always
/// optimized: at -O0 it costs real-time headroom in a debug app.
fn build_speexdsp() {
    println!("cargo:rerun-if-changed=vendor/speexdsp");
    cc::Build::new()
        .files(["mdf.c", "preprocess.c", "fftwrap.c", "filterbank.c", "smallft.c"].map(|f| format!("vendor/speexdsp/libspeexdsp/{f}")))
        .include("vendor/speexdsp/include")
        .define("FLOATING_POINT", None)
        // smallft, not kiss_fft: sherpa-onnx already links a kissfft whose
        // symbols would clash.
        .define("USE_SMALLFT", None)
        .define("EXPORT", Some(""))
        .opt_level(3)
        .warnings(false)
        .compile("chain_speexdsp");
}

fn marker_lib(os: &str) -> &'static str {
    if os == "windows" { "sherpa-onnx-c-api.lib" } else { "libsherpa-onnx-c-api.a" }
}

/// `curl` ships with macOS and with Windows 10+, so the build script needs
/// no HTTP client of its own.
fn download(url: &str, to: &Path) {
    let status = Command::new("curl")
        .args(["--fail", "--location", "--silent", "--show-error", "--output"])
        .arg(to)
        .arg(url)
        .status()
        .expect("chain-core needs `curl` to fetch sherpa-onnx");
    assert!(status.success(), "couldn't download {url}");
}

fn sha256_of(path: &Path) -> String {
    let mut file = fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn build_swift_bridge() {
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        other => other,
    }
    .to_string();
    let target = format!("{arch}-apple-macos{SWIFT_MIN_MACOS}");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let sdk = run("xcrun", &["--sdk", "macosx", "--show-sdk-path"]);

    let status = Command::new("swiftc")
        .args(["-emit-library", "-static", "-parse-as-library", "-O", "-swift-version", "5"])
        .args(["-module-name", "ChainSwift", "-target", &target, "-sdk", sdk.trim()])
        .args(["swift/ChainSpeech.swift", "swift/ChainVision.swift", "swift/ChainRecorder.swift", "swift/ChainWindow.swift", "swift/ChainPdf.swift", "swift/ChainShare.swift", "swift/ChainAttention.swift"])
        .arg("-o")
        .arg(out_dir.join("libChainSwift.a"))
        .status()
        .expect("chain-core needs `swiftc` to build on macOS — install the Xcode Command Line Tools (`xcode-select --install`)");
    assert!(status.success(), "swiftc failed to compile swift/*.swift");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ChainSwift");
    // The OS's Swift runtime first: the toolchain directory also holds
    // back-deployment copies of libswift_Concurrency with an @rpath install
    // name, which an app binary can't resolve. The toolchain directory is
    // still needed after it, for the static compatibility shims.
    println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk.trim());
    // chain-core's own test binaries; apps add this in their own build.rs.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    let info = run("swiftc", &["-print-target-info", "-target", &target]);
    let info: serde_json::Value = serde_json::from_str(&info).expect("swiftc -print-target-info returned bad JSON");
    for path in info["paths"]["runtimeLibraryPaths"].as_array().into_iter().flatten() {
        println!("cargo:rustc-link-search=native={}", path.as_str().unwrap_or_default());
    }
    for framework in ["Foundation", "AVFoundation", "Speech", "Vision", "ImageIO", "CoreGraphics", "CoreAudio", "AppKit", "WebKit", "CoreText"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}

fn run(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap_or_else(|e| panic!("couldn't run {program}: {e}"));
    assert!(output.status.success(), "{program} {} failed", args.join(" "));
    String::from_utf8(output.stdout).expect("non-UTF-8 output")
}
