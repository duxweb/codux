use std::{fs, path::PathBuf};
use url::Url;

pub(super) struct PetInstallRequestInternal {
    pub(super) zip_url: Url,
    pub(super) slug: String,
    pub(super) display_name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) image_url: Option<Url>,
}

pub(super) struct StagingCleanup(pub(super) PathBuf);

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
