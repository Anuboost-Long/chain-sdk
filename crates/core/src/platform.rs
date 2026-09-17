//! Platform capability — see /agent-docs/capabilities/platform/CONTRACT.md.
//! Mirrors the PlatformInfo shape defined in
//! /capabilities/platform/contract.ts. Keep both in sync by hand until a
//! codegen step exists.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub runtime_version: String,
}

/// The current OS/arch isn't one Chain supports yet (see CONTRACT.md —
/// the caller should surface this as ChainError { code: "UNSUPPORTED" }).
#[derive(Debug)]
pub struct UnsupportedPlatform;

/// `runtime_version` is supplied by the caller (the Tauri command layer)
/// rather than hardcoded here, so this crate stays runtime-agnostic —
/// Chain Core doesn't need to know it's Tauri underneath.
pub fn get_platform_info(runtime_version: impl Into<String>) -> Result<PlatformInfo, UnsupportedPlatform> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => return Err(UnsupportedPlatform),
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return Err(UnsupportedPlatform),
    };
    Ok(PlatformInfo {
        os,
        arch,
        runtime_version: runtime_version.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_info_on_a_supported_platform() {
        let info = get_platform_info("1.0.0").expect("this platform should be supported");
        assert!(info.os == "macos" || info.os == "windows");
        assert!(info.arch == "arm64" || info.arch == "x64");
        assert_eq!(info.runtime_version, "1.0.0");
    }
}
