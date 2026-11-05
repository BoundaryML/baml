//! Where a store's recordings and CAS blobs live. The caller resolves
//! project configuration; explicit trace directories need no project.
use std::path::{Path, PathBuf};

use btel_settings::local_files::{CAS_DIRECTORY, RECORDINGS_DIRECTORY};
use btel_snapshot::CasId;

/// A Btel root: `recordings/<id>/<sequence>.btel` plus shared CAS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLayout {
    /// The directory holding `recordings/` and `cas/`, and derived state.
    pub root: PathBuf,
    pub recordings: PathBuf,
    /// Unversioned CAS root; blob paths add the format version.
    pub cas: PathBuf,
}

impl SourceLayout {
    /// Open the directory containing `recordings/`, `cas/` and the query index.
    pub fn for_btel(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            recordings: root.join("recordings"),
            cas: root.join("cas"),
            root,
        }
    }

    /// Recognize an explicitly selected trace directory before project walk-up.
    /// Accept a state directory (`.baml` or `BAML_HOME`), its `btel` child,
    /// or the `recordings` directory. Never scan recording contents or the CAS.
    pub fn from_trace_directory(path: &Path) -> Option<Self> {
        if !path.is_dir() {
            return None;
        }
        if path.join("btel").is_dir() || path.file_name().is_some_and(|name| name == ".baml") {
            return Some(Self::for_btel(path.join("btel")));
        }
        if path.join("recordings").is_dir() || path.file_name().is_some_and(|name| name == "btel") {
            return Some(Self::for_btel(path));
        }
        if path.file_name().is_some_and(|name| name == "recordings") {
            return path.parent().map(Self::for_btel);
        }
        None
    }

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

    pub fn blob_path(&self, id: CasId) -> PathBuf {
        btel_file::cas_path(&self.cas, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trace-directory recognition must not require files or create a query index.
    #[test]
    fn explicit_trace_directories_keep_their_layout_even_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join(".baml");
        std::fs::create_dir(&state).unwrap();
        let expected = SourceLayout::for_project(tmp.path());
        assert_eq!(
            SourceLayout::from_trace_directory(&state),
            Some(expected.clone())
        );
        std::fs::create_dir(state.join("btel")).unwrap();
        assert_eq!(
            SourceLayout::from_trace_directory(&state.join("btel")),
            Some(expected.clone())
        );
        std::fs::create_dir(state.join("btel/recordings")).unwrap();
        assert_eq!(
            SourceLayout::from_trace_directory(&state.join("btel/recordings")),
            Some(expected.clone())
        );
        assert_eq!(SourceLayout::from_trace_directory(tmp.path()), None);
        assert_eq!(
            SourceLayout::from_trace_directory(&state.join("missing")),
            None
        );
        let custom = tmp.path().join("traces");
        std::fs::rename(state, &custom).unwrap();
        assert_eq!(
            SourceLayout::from_trace_directory(&custom),
            Some(SourceLayout::for_btel(custom.join("btel")))
        );
        assert!(!expected.root.join("query.sqlite").exists());
    }
}
