//! Packages capability — see /agent-docs/capabilities/packages/CONTRACT.md.
//! Archives the app runs from (a VS Code extension's language server, run
//! with the user's own `java`/`node`). The same download, SHA-256 check,
//! safe unpack and atomic replace as models.rs, in a store of its own, but
//! laid out exactly as the archive is and with the paths handed to the
//! app. Nothing is ever marked executable or run here.

use std::path::Path;

use serde::Serialize;

use crate::models::{Download, InstallTicket, InstalledModel, Models};
pub use crate::models::{Installs, ModelManifest as PackageManifest, ModelsError as PackagesError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub id: String,
    pub path: String,
    pub size_bytes: u64,
    pub installed_at: String,
}

#[derive(Clone)]
pub struct Packages(Models);

impl Packages {
    pub fn open(dir: &Path) -> Result<Self, PackagesError> {
        Models::open_store(dir, false).map(Self)
    }

    pub async fn download(
        &self,
        manifest: &PackageManifest,
        ticket: &InstallTicket,
        on_progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Download, PackagesError> {
        self.0.download(manifest, ticket, on_progress).await
    }

    /// Blocking.
    pub fn install(&self, download: Download, ticket: &InstallTicket) -> Result<InstalledPackage, PackagesError> {
        self.0.install(download, ticket).map(|installed| self.with_path(installed))
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>, PackagesError> {
        Ok(self.0.list()?.into_iter().map(|installed| self.with_path(installed)).collect())
    }

    /// Idempotent.
    pub fn remove(&self, id: &str) -> Result<(), PackagesError> {
        self.0.remove(id)
    }

    /// The absolute path of `file` inside package `id`; `NotFound` when
    /// either isn't there.
    pub fn resolve(&self, id: &str, file: &str) -> Result<String, PackagesError> {
        Ok(self.0.resolve(id, file)?.to_string_lossy().into_owned())
    }

    fn with_path(&self, installed: InstalledModel) -> InstalledPackage {
        InstalledPackage {
            path: self.0.path_of(&installed.id).to_string_lossy().into_owned(),
            id: installed.id,
            size_bytes: installed.size_bytes,
            installed_at: installed.installed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_a_missing_package_and_unsafe_names() {
        let dir = std::env::temp_dir().join(format!("chain-packages-test-{}", std::process::id()));
        let packages = Packages::open(&dir).unwrap();
        assert!(packages.list().unwrap().is_empty());
        assert!(matches!(packages.resolve("sonarlint", "extension/package.json"), Err(PackagesError::NotFound(_))));
        assert!(matches!(packages.resolve("Bad_Id", "x"), Err(PackagesError::InvalidArgument(_))));
        packages.remove("sonarlint").unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
