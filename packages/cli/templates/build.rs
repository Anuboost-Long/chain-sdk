fn main() {
    // chain-core's speech bridge is Swift and links Swift Concurrency by
    // @rpath when the build targets macOS < 12, which `tauri dev` always
    // does. The OS copy lives in /usr/lib/swift (macOS 12+), so point the
    // app binary there. A library's build script can't add this for its
    // dependents, which is why it lives here.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::build()
}
