//! Lazy, bounded CAS access. Blobs stay on disk; a missing or corrupt blob
//! is an explicit outcome, and a later read may find a blob that arrived since.
use std::{fs, io, io::Read, path::PathBuf, sync::Arc};

use btel_snapshot::{BlobError, CasId, DecodeLimits, DecodedSnapshot};

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

/// Why a blob did not yield a verified snapshot.
#[derive(Clone, Debug)]
pub enum CasUnavailable {
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

impl CasUnavailable {
    /// Stable diagnostic code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing => "cas_missing",
            Self::Unreadable(_) => "cas_unreadable",
            Self::TooLarge { .. } => "cas_too_large",
            Self::Corrupt(BlobError::IdMismatch { .. }) => "cas_id_mismatch",
            Self::Corrupt(BlobError::Limit(_)) => "cas_decode_limit",
            Self::Corrupt(BlobError::Version(_)) => "cas_unsupported_version",
            Self::Corrupt(BlobError::Magic | BlobError::Truncated | BlobError::Invalid(_)) => {
                "cas_corrupt"
            }
        }
    }
}

pub struct CasLoad {
    pub snapshot: Result<Arc<DecodedSnapshot>, CasUnavailable>,
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

    pub fn path(&self, id: CasId) -> PathBuf {
        for &version in btel_settings::snapshot::READABLE_BLOB_VERSIONS {
            let path = btel_file::cas_path_versioned(&self.root, id, version);
            // Only a missing file permits fallback; unreadable current data
            // must retain its error rather than silently selecting older data.
            if !matches!(path.try_exists(), Ok(false)) {
                return path;
            }
        }
        btel_file::cas_path(&self.root, id)
    }

    /// Read, decode and verify one blob.
    pub fn load(&self, id: CasId) -> CasLoad {
        let path = self.path(id);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return CasLoad {
                    snapshot: Err(CasUnavailable::Missing),
                    bytes_read: 0,
                };
            }
            Err(error) => {
                return CasLoad {
                    snapshot: Err(CasUnavailable::Unreadable(error.to_string())),
                    bytes_read: 0,
                };
            }
        };
        let limit = self.limits.max_blob_bytes;
        let mut bytes = Vec::new();
        if let Err(error) = file.take(limit + 1).read_to_end(&mut bytes) {
            return CasLoad {
                snapshot: Err(CasUnavailable::Unreadable(error.to_string())),
                bytes_read: bytes.len() as u64,
            };
        }
        let bytes_read = bytes.len() as u64;
        if bytes_read > limit {
            return CasLoad {
                snapshot: Err(CasUnavailable::TooLarge {
                    len: bytes_read,
                    limit,
                }),
                bytes_read,
            };
        }
        let snapshot = match btel_snapshot::decode_blob(&bytes, &self.limits.decode) {
            // The path is derived from the ID, but only the recomputed graph
            // identity proves the content belongs to it.
            Ok(snapshot) if snapshot.id == id => Ok(Arc::new(snapshot)),
            Ok(snapshot) => Err(CasUnavailable::Corrupt(BlobError::IdMismatch {
                declared: id,
                computed: snapshot.id,
            })),
            Err(error) => Err(CasUnavailable::Corrupt(error)),
        };
        CasLoad {
            snapshot,
            bytes_read,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_blobs_load_from_their_original_versioned_directory() {
        let directory = tempfile::tempdir().unwrap();
        let store = CasStore::new(directory.path().to_owned(), CasLimits::default());
        for line in
            include_str!("../../btel_snapshot/tests/fixtures/format_v3_external.hex").lines()
        {
            let bytes: Vec<_> = line
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            let id = CasId::from_bytes(bytes[12..28].try_into().unwrap());
            let path = btel_file::cas_path_versioned(directory.path(), id, 3);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert_eq!(store.path(id), path);
            let loaded = store.load(id);
            assert_eq!(loaded.snapshot.unwrap().id, id);
            assert_eq!(loaded.bytes_read, bytes.len() as u64);
            assert!(!btel_file::cas_path(directory.path(), id).exists());
        }
    }
}
