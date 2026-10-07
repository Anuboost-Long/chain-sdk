fn main() {
    // chain-core's speech bridge is Swift and links Swift Concurrency by
    // @rpath when the build targets macOS < 12, which `tauri dev` always
    // does. The OS copy lives in /usr/lib/swift (macOS 12+), so point the
    // app binary there. A library's build script can't add this for its
    // dependents, which is why it lives here.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    // desktop.window's startup options are package.json's "chain.window";
    // src/window.rs compiles in the nearest package.json above this crate.
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let package_json = manifest_dir
        .ancestors()
        .map(|dir| dir.join("package.json"))
        .find(|path| path.is_file())
        .expect("no package.json above the native project");
    println!("cargo:rustc-env=CHAIN_PACKAGE_JSON={}", package_json.display());
    tauri_build::build()
}
