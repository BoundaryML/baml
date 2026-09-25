//! Lazy, bounded CAS access. Blobs stay on disk; a missing or corrupt blob
//! is an explicit outcome, and a later read may find a blob that arrived since.
use std::{fs, io, io::Read, path::PathBuf, sync::Arc};

use btel_snapshot::{BlobError, DecodeLimits, DecodedSnapshot, SnapshotId};

#[derive(Clone, Copy, Debug)]
pub struct CasLimits {
    pub max_blob_bytes: u64,
    pub decode: DecodeLimits,
}
impl Default for CasLimits {
    fn default() -> Self {
        Self {
            max_blob_bytes: 64 << 20,
            decode: DecodeLimits::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum CasOutcome {
    Available(Arc<DecodedSnapshot>),
    /// No blob at the expected path (not delivered, removed, or another
    /// blob format version).
    Missing,
    Unreadable(String),
    TooLarge {
        len: u64,
        limit: u64,
    },
    /// Undecodable or failing identity verification.
    Corrupt(BlobError),
}

impl CasOutcome {
    /// Stable diagnostic code for an unavailable outcome.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Available(_) => "available",
            Self::Missing => "cas_missing",
            Self::Unreadable(_) => "cas_unreadable",
            Self::TooLarge { .. } => "cas_too_large",
            Self::Corrupt(BlobError::IdMismatch { .. }) => "cas_id_mismatch",
            Self::Corrupt(BlobError::Limit(_)) => "cas_decode_limit",
            Self::Corrupt(BlobError::Version(_)) => "cas_unsupported_version",
            Self::Corrupt(_) => "cas_corrupt",
        }
    }
}

pub struct CasLoad {
    pub outcome: CasOutcome,
    pub bytes_read: u64,
}

#[derive(Clone, Debug)]
pub struct CasStore {
    root: PathBuf,
    limits: CasLimits,
}

impl CasStore {
    /// `root` is the unversioned CAS directory (`.baml/btel/cas`).
    pub fn new(root: PathBuf, limits: CasLimits) -> Self {
        Self { root, limits }
    }

    pub fn limits(&self) -> &CasLimits {
        &self.limits
    }

    pub fn path(&self, id: SnapshotId) -> PathBuf {
        btel_file::cas_path(&self.root, id)
    }

    /// Read, decode and verify one blob.
    pub fn load(&self, id: SnapshotId) -> CasLoad {
        let path = self.path(id);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return CasLoad {
                    outcome: CasOutcome::Missing,
                    bytes_read: 0,
                };
            }
            Err(error) => {
                return CasLoad {
                    outcome: CasOutcome::Unreadable(error.to_string()),
                    bytes_read: 0,
                };
            }
        };
        let limit = self.limits.max_blob_bytes;
        let mut bytes = Vec::new();
        if let Err(error) = file.take(limit + 1).read_to_end(&mut bytes) {
            return CasLoad {
                outcome: CasOutcome::Unreadable(error.to_string()),
                bytes_read: bytes.len() as u64,
            };
        }
        let bytes_read = bytes.len() as u64;
        if bytes_read > limit {
            return CasLoad {
                outcome: CasOutcome::TooLarge {
                    len: bytes_read,
                    limit,
                },
                bytes_read,
            };
        }
        let outcome = match btel_snapshot::decode_blob(&bytes, &self.limits.decode) {
            // The path is derived from the ID, but only the recomputed graph
            // identity proves the content belongs to it.
            Ok(snapshot) if snapshot.id == id => CasOutcome::Available(Arc::new(snapshot)),
            Ok(snapshot) => CasOutcome::Corrupt(BlobError::IdMismatch {
                declared: id,
                computed: snapshot.id,
            }),
            Err(error) => CasOutcome::Corrupt(error),
        };
        CasLoad {
            outcome,
            bytes_read,
        }
    }
}
