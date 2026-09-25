//! Where one project's recordings and CAS blobs live. The caller resolves
//! the project; nothing here searches for it.
use std::path::{Path, PathBuf};

use btel_settings::local_files::{CAS_DIRECTORY, RECORDINGS_DIRECTORY};
use btel_snapshot::SnapshotId;

/// `<project>/.baml/btel`: `recordings/<id>/<sequence>.btel` plus shared CAS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLayout {
    /// The directory holding `recordings/` and `cas/`, and derived state.
    pub root: PathBuf,
    pub recordings: PathBuf,
    /// Unversioned CAS root; blob paths add the format version.
    pub cas: PathBuf,
}

impl SourceLayout {
    pub fn for_project(project_root: &Path) -> Self {
        let recordings = project_root.join(RECORDINGS_DIRECTORY);
        let cas = project_root.join(CAS_DIRECTORY);
        let root = recordings
            .parent()
            .expect("recordings directory has a parent")
            .to_path_buf();
        Self {
            root,
            recordings,
            cas,
        }
    }

    pub fn blob_path(&self, id: SnapshotId) -> PathBuf {
        btel_file::cas_path(&self.cas, id)
    }
}
