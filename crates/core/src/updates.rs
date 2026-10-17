//! Updates capability — see /agent-docs/capabilities/updates/CONTRACT.md.
//! The Tauri-free half of a built app replacing itself: reading
//! package.json's "chain.updates", deciding whether this run can update,
//! and swapping the downloaded app into place. Checking the feed,
//! downloading and verifying the signature are tauri-plugin-updater's,
//! driven from packages/cli/templates/updates.rs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum UpdatesError {
    InvalidConfig(String),
    Install(String),
}

/// package.json's "chain.updates".
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub feed: String,
    pub public_key: String,
}

impl Config {
    /// None when package.json has no "chain.updates".
    pub fn from_package_json(package_json: &str) -> Result<Option<Self>, UpdatesError> {
        let invalid = |m: String| UpdatesError::InvalidConfig(format!("package.json \"chain.updates\": {m}"));
        let package: serde_json::Value = serde_json::from_str(package_json).map_err(|e| invalid(e.to_string()))?;
        let Some(updates) = package.pointer("/chain/updates") else { return Ok(None) };
        let config: Self = serde_json::from_value(updates.clone()).map_err(|e| invalid(e.to_string()))?;
        let feed = reqwest::Url::parse(&config.feed).map_err(|e| invalid(format!("feed {:?}: {e}", config.feed)))?;
        if !(feed.scheme() == "https" || (feed.scheme() == "http" && crate::http::is_loopback(&feed))) {
            return Err(invalid(format!("feed must be https (http only for a loopback host), got {:?}", config.feed)));
        }
        if config.public_key.trim().is_empty() {
            return Err(invalid("publicKey is empty — `chain updates key` prints it".to_string()));
        }
        Ok(Some(config))
    }

    /// Plain http, which the updater only accepts when told to.
    pub fn insecure_feed(&self) -> bool {
        self.feed.starts_with("http:")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unsupported {
    NotConfigured,
    DevBuild,
    ReadOnly,
    UnsupportedPlatform,
}

impl Unsupported {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotConfigured => "this app has no update feed (package.json \"chain.updates\")",
            Self::DevBuild => "this is a development run; only a built app can update itself",
            Self::ReadOnly => "this user can't replace the app where it's installed",
            Self::UnsupportedPlatform => "updating isn't supported on this platform yet",
        }
    }
}

/// The `.app` bundle an executable runs from (`X.app/Contents/MacOS/x`).
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension().is_some_and(|extension| extension == "app");
    is_bundle.then(|| bundle.to_path_buf())
}

#[cfg(unix)]
fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    // SAFETY: a valid NUL-terminated path.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

/// The bundle this run can replace, or why it can't. Replacing needs
/// write access to the bundle and to the folder holding it.
pub fn replaceable_bundle(exe: &Path, configured: bool, dev_build: bool) -> Result<PathBuf, Unsupported> {
    if !cfg!(target_os = "macos") {
        return Err(Unsupported::UnsupportedPlatform);
    }
    if !configured {
        return Err(Unsupported::NotConfigured);
    }
    let bundle = bundle_of(exe).filter(|_| !dev_build).ok_or(Unsupported::DevBuild)?;
    #[cfg(unix)]
    {
        let parent = bundle.parent().ok_or(Unsupported::ReadOnly)?;
        if !writable(&bundle) || !writable(parent) {
            return Err(Unsupported::ReadOnly);
        }
    }
    Ok(bundle)
}

/// Unpacks the update (a gzipped tar holding one `.app`) next to
/// `bundle` and swaps the two in one atomic exchange, so either the old
/// or the new app is in place at every moment. On failure `bundle` is
/// untouched.
#[cfg(target_os = "macos")]
pub fn install(bundle: &Path, archive: &[u8]) -> Result<(), UpdatesError> {
    let fail = |what: &str, e: &dyn std::fmt::Display| UpdatesError::Install(format!("{what}: {e}"));
    let parent = bundle.parent().ok_or_else(|| UpdatesError::Install("the app has no folder".to_string()))?;
    let name = bundle.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    // Beside the app: the same disk, which the exchange needs.
    let staging = parent.join(format!(".{name}.update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir(&staging).map_err(|e| fail("couldn't make room for the update", &e))?;
    let result = (|| {
        tar::Archive::new(flate2::read::GzDecoder::new(archive))
            .unpack(&staging)
            .map_err(|e| fail("the update couldn't be unpacked", &e))?;
        let new_bundle = single_app(&staging)?;
        swap(&new_bundle, bundle).map_err(|e| fail("the update couldn't be swapped in", &e))?;
        // So Launch Services and the Finder see the new bundle.
        let _ = std::process::Command::new("touch").arg(bundle).status();
        Ok(())
    })();
    // After a swap this holds the old app; before one, the unpacked update.
    let _ = std::fs::remove_dir_all(&staging);
    result
}

#[cfg(target_os = "macos")]
fn single_app(staging: &Path) -> Result<PathBuf, UpdatesError> {
    let apps: Vec<PathBuf> = std::fs::read_dir(staging)
        .map_err(|e| UpdatesError::Install(e.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .collect();
    match apps.as_slice() {
        [app] if app.join("Contents/Info.plist").is_file() => Ok(app.clone()),
        _ => Err(UpdatesError::Install("the update doesn't hold exactly one app".to_string())),
    }
}

#[cfg(target_os = "macos")]
fn swap(a: &Path, b: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let a = std::ffi::CString::new(a.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(b.as_os_str().as_bytes())?;
    // SAFETY: valid NUL-terminated paths.
    match unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) } {
        0 => Ok(()),
        _ => Err(std::io::Error::last_os_error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_needs_an_https_feed_and_a_key() {
        let package = |updates: &str| format!(r#"{{"name":"x","chain":{{"updates":{updates}}}}}"#);
        assert_eq!(Config::from_package_json(r#"{"name":"x"}"#).unwrap(), None);
        let ok = Config::from_package_json(&package(r#"{"feed":"https://example.com/latest.json","publicKey":"abc"}"#)).unwrap().unwrap();
        assert!(!ok.insecure_feed());
        let local = Config::from_package_json(&package(r#"{"feed":"http://127.0.0.1:8765/latest.json","publicKey":"abc"}"#)).unwrap().unwrap();
        assert!(local.insecure_feed());
        assert!(Config::from_package_json(&package(r#"{"feed":"http://example.com/latest.json","publicKey":"abc"}"#)).is_err());
        assert!(Config::from_package_json(&package(r#"{"feed":"https://example.com/latest.json","publicKey":" "}"#)).is_err());
        assert!(Config::from_package_json(&package(r#"{"feed":"https://example.com/latest.json"}"#)).is_err());
        assert!(Config::from_package_json(&package(r#"{"feed":"https://e.com/l.json","publicKey":"a","token":"x"}"#)).is_err());
    }

    #[test]
    fn finds_the_bundle_an_executable_runs_from() {
        assert_eq!(bundle_of(Path::new("/Applications/Lazify.app/Contents/MacOS/lazify")), Some(PathBuf::from("/Applications/Lazify.app")));
        assert_eq!(bundle_of(Path::new("/repo/.chain/native/target/debug/lazify")), None);
        assert_eq!(bundle_of(Path::new("/x/Contents/MacOS/lazify")), None);
    }

    #[cfg(target_os = "macos")]
    mod macos {
        use super::*;
        use std::io::Write;

        fn temp_dir(name: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("chain-updates-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn app(dir: &Path, version: &str) -> PathBuf {
            let bundle = dir.join("Test.app");
            std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
            std::fs::write(bundle.join("Contents/Info.plist"), version).unwrap();
            std::fs::write(bundle.join("Contents/MacOS/test"), version).unwrap();
            bundle
        }

        fn archive_of(bundle: &Path) -> Vec<u8> {
            let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
            tar.append_dir_all("Test.app", bundle).unwrap();
            let mut gz = tar.into_inner().unwrap();
            gz.flush().unwrap();
            gz.finish().unwrap()
        }

        #[test]
        fn replaceable_only_from_a_writable_bundle() {
            let dir = temp_dir("support");
            let bundle = app(&dir, "1");
            let exe = bundle.join("Contents/MacOS/test");
            assert_eq!(replaceable_bundle(&exe, true, false), Ok(bundle.clone()));
            assert_eq!(replaceable_bundle(&exe, false, false), Err(Unsupported::NotConfigured));
            assert_eq!(replaceable_bundle(&exe, true, true), Err(Unsupported::DevBuild));
            assert_eq!(replaceable_bundle(Path::new("/tmp/test"), true, false), Err(Unsupported::DevBuild));
            assert_eq!(
                replaceable_bundle(Path::new("/System/Applications/Calculator.app/Contents/MacOS/Calculator"), true, false),
                Err(Unsupported::ReadOnly)
            );
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn installs_the_new_app_in_place_and_leaves_nothing_behind() {
            let old_dir = temp_dir("old");
            let new_dir = temp_dir("new");
            let installed = app(&old_dir, "1.0.0");
            let update = archive_of(&app(&new_dir, "1.1.0"));
            install(&installed, &update).unwrap();
            assert_eq!(std::fs::read_to_string(installed.join("Contents/Info.plist")).unwrap(), "1.1.0");
            assert_eq!(std::fs::read_dir(&old_dir).unwrap().count(), 1, "only the app is left");
            std::fs::remove_dir_all(&old_dir).unwrap();
            std::fs::remove_dir_all(&new_dir).unwrap();
        }

        #[test]
        fn a_bad_update_leaves_the_app_untouched() {
            let dir = temp_dir("bad");
            let installed = app(&dir, "1.0.0");
            assert!(install(&installed, b"not a gzip").is_err());
            let empty = {
                let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                let mut tar = tar::Builder::new(&mut gz);
                tar.finish().unwrap();
                drop(tar);
                gz.finish().unwrap()
            };
            assert!(matches!(install(&installed, &empty), Err(UpdatesError::Install(m)) if m.contains("exactly one app")));
            assert_eq!(std::fs::read_to_string(installed.join("Contents/Info.plist")).unwrap(), "1.0.0");
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}
