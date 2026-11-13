/// The UUID scope shared by a recording and its public span identities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordingId([u8; 16]);

impl RecordingId {
    pub fn generate() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// The OS process a recording's engine ran in. Every engine of one process
/// shares `process_id`; the rest describes the process for debugging.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessInfo {
    pub process_id: [u8; 16],
    pub baml_version: String,
    /// `cli`, `lsp`, `pack`, `python`, ...
    pub host: String,
    pub command: Vec<String>,
    pub started_at_unix_ns: i64,
}

/// A git commit ID, in the repository's object format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHash {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

impl GitHash {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Sha1(bytes) => bytes,
            Self::Sha256(bytes) => bytes,
        }
    }
}

/// Where a build's package sat in git when it was built (BEP-088).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitInfo {
    /// URL of `origin` in `https` form with credentials removed; `None`
    /// when the repository has no `origin`.
    pub remote: Option<String>,
    /// The commit at `HEAD`.
    pub hash: GitHash,
    /// The package's files differ from `hash`: a tracked file is modified,
    /// staged or deleted, or a file is untracked and not ignored. The commit
    /// then only says what the build started from.
    pub dirty: bool,
    /// The branch `HEAD` is on; `None` when it is detached and the CI names
    /// no branch.
    pub branch: Option<String>,
    /// Where the package sits relative to the repository root, `/`-separated.
    /// Empty when the package is the root.
    pub package_path: String,
}

/// What ran the compiler for a build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildOrigin {
    Pack,
    Generate,
    Run,
    Test,
    /// A package compiled at run time.
    Dynamic,
}

impl BuildOrigin {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pack => "pack",
            Self::Generate => "generate",
            Self::Run => "run",
            Self::Test => "test",
            Self::Dynamic => "dynamic",
        }
    }
}

/// One run of the compiler building one package (BEP-088's `build` row).
/// Unlike a package's metadata it is not a function of the package: two
/// builds of the same code are two records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildRecord {
    /// Identifies this run of the compiler: a random UUID.
    pub build_id: [u8; 16],
    pub timestamp_unix_ns: i64,
    pub origin: BuildOrigin,
    /// `None` when the package is not in a git repository.
    pub git: Option<GitInfo>,
    /// Hash of the package's formatted, normalized source text.
    pub source_hash: [u8; 32],
    /// The package this build produced.
    pub compilation_unit_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStatus {
    /// The host finalized its SDK lifetime without reporting an exit outcome.
    Unknown,
    Success,
    Error,
    Panicked,
}

/// The end of a host's BAML lifetime, reported before shutting engines down.
/// SDK shutdown can report completion without knowing the OS process's outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessExit {
    pub status: ProcessStatus,
    pub at_unix_ns: i64,
}

/// Written by the host when it finalizes its BAML lifetime, read when the
/// recording ends. Internal engine shutdown, such as replacement, leaves it empty.
#[derive(Debug, Default)]
pub struct ProcessExitSlot(std::sync::Mutex<Option<ProcessExit>>);

impl ProcessExitSlot {
    pub fn set(&self, exit: ProcessExit) {
        let mut held = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SDK finalization must not erase an outcome the host already reported.
        if exit.status == ProcessStatus::Unknown
            && held.is_some_and(|exit| exit.status != ProcessStatus::Unknown)
        {
            return;
        }
        *held = Some(exit);
    }

    pub fn get(&self) -> Option<ProcessExit> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_completion_preserves_a_known_host_outcome() {
        for status in [
            ProcessStatus::Success,
            ProcessStatus::Error,
            ProcessStatus::Panicked,
        ] {
            let slot = ProcessExitSlot::default();
            let known = ProcessExit {
                status,
                at_unix_ns: 1,
            };
            slot.set(known);
            slot.set(ProcessExit {
                status: ProcessStatus::Unknown,
                at_unix_ns: 2,
            });
            assert_eq!(slot.get(), Some(known));
        }
    }
}
