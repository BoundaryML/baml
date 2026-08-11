//! Target-neutral value artifact sink abstractions.

use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobRef {
    pub algorithm: String,
    pub digest: String,
    pub size_bytes: usize,
}

impl BlobRef {
    pub const ALGORITHM_SHA256: &'static str = "sha256";
    const SHA256_HEX_LEN: usize = 64;

    #[must_use]
    pub fn sha256(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(&mut hex, "{byte:02x}");
        }
        Self {
            algorithm: Self::ALGORITHM_SHA256.to_string(),
            digest: hex,
            size_bytes: bytes.len(),
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.algorithm != Self::ALGORITHM_SHA256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported blob algorithm `{}`", self.algorithm),
            ));
        }
        if self.digest.len() != Self::SHA256_HEX_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid sha256 blob digest length {}; expected {} hex characters",
                    self.digest.len(),
                    Self::SHA256_HEX_LEN
                ),
            ));
        }
        if !self.digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid sha256 blob digest; expected only hex characters",
            ));
        }
        Ok(())
    }

    fn normalized_digest(&self) -> io::Result<String> {
        self.validate()?;
        Ok(self.digest.to_ascii_lowercase())
    }

    fn verify_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() != self.size_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "blob size mismatch for {}; expected {} bytes, got {} bytes",
                    self.digest,
                    self.size_bytes,
                    bytes.len()
                ),
            ));
        }
        let actual = Self::sha256(bytes);
        if actual.digest != self.digest.to_ascii_lowercase() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "blob digest mismatch for {}; computed {}",
                    self.digest, actual.digest
                ),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    #[must_use]
    pub fn for_boundary_dir(boundary_dir: impl AsRef<Path>) -> Self {
        Self::new(boundary_dir.as_ref().join("blobs"))
    }

    pub fn path_for(&self, blob_ref: &BlobRef) -> io::Result<PathBuf> {
        let digest = blob_ref.normalized_digest()?;
        let prefix = digest.get(..2).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "blob digest has no prefix")
        })?;
        Ok(self
            .root
            .join(&blob_ref.algorithm)
            .join(prefix)
            .join(format!("{digest}.blob")))
    }

    pub fn write_blob(&self, bytes: &[u8]) -> io::Result<BlobRef> {
        let blob_ref = BlobRef::sha256(bytes);
        let path = self.path_for(&blob_ref)?;
        if path.is_file() {
            return Ok(blob_ref);
        }

        let parent = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "blob path has no parent")
        })?;
        fs::create_dir_all(parent)?;
        let tmp_path = parent.join(format!(
            ".{}.{}.{}.tmp",
            blob_ref.digest,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)?;
            tmp.write_all(bytes)?;
            tmp.flush()?;
            tmp.sync_all()?;
        }
        if path.is_file() {
            let _ = fs::remove_file(&tmp_path);
            return Ok(blob_ref);
        }
        fs::rename(&tmp_path, &path)?;
        Ok(blob_ref)
    }

    pub fn read_blob(&self, blob_ref: &BlobRef) -> io::Result<Option<Vec<u8>>> {
        let path = self.path_for(blob_ref)?;
        match fs::read(path) {
            Ok(bytes) => {
                blob_ref.verify_bytes(&bytes)?;
                Ok(Some(bytes))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueArtifactRef {
    NativeFile {
        path: std::path::PathBuf,
    },
    Bytes {
        len: usize,
        truncated: bool,
        dropped_bytes: usize,
        dropped_chunks: usize,
    },
}

pub trait ValueArtifactSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<ValueArtifactRef>;
    /// Make everything written so far durable (§6.6 D-milestone hook).
    /// No-op for sinks without a durability boundary of their own.
    fn sync_data(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileValueArtifactSink {
    file: File,
    path: PathBuf,
    bytes_written: u64,
    dir_synced: bool,
}

impl FileValueArtifactSink {
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            file: File::create(path)?,
            path: path.to_path_buf(),
            bytes_written: 0,
            dir_synced: false,
        })
    }

    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}

impl ValueArtifactSink for FileValueArtifactSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)?;
        self.bytes_written = self
            .bytes_written
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<ValueArtifactRef> {
        self.file.flush()?;
        Ok(ValueArtifactRef::NativeFile {
            path: self.path.clone(),
        })
    }

    /// Fsync the segment and (once) its directory — `File::flush` alone is
    /// a no-op for `std::fs::File`, so this is the only durability point a
    /// `.bamlvalue` segment has.
    fn sync_data(&mut self) -> io::Result<()> {
        self.file.sync_data()?;
        if !self.dir_synced {
            if let Some(dir) = self.path.parent() {
                crate::fsutil::fsync_dir(dir)?;
            }
            self.dir_synced = true;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ByteValueArtifactSink {
    bytes: Vec<u8>,
    max_bytes: Option<usize>,
    dropped_bytes: usize,
    dropped_chunks: usize,
}

impl ByteValueArtifactSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes: Some(max_bytes),
            dropped_bytes: 0,
            dropped_chunks: 0,
        }
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn truncated(&self) -> bool {
        self.dropped_bytes > 0 || self.dropped_chunks > 0
    }
}

impl ValueArtifactSink for ByteValueArtifactSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(max_bytes) = self.max_bytes else {
            self.bytes.extend_from_slice(bytes);
            return Ok(());
        };

        let remaining = max_bytes.saturating_sub(self.bytes.len());
        if remaining >= bytes.len() {
            self.bytes.extend_from_slice(bytes);
            return Ok(());
        }

        if remaining > 0 {
            self.bytes.extend_from_slice(&bytes[..remaining]);
        }
        self.dropped_bytes = self
            .dropped_bytes
            .saturating_add(bytes.len().saturating_sub(remaining));
        self.dropped_chunks = self.dropped_chunks.saturating_add(1);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<ValueArtifactRef> {
        Ok(ValueArtifactRef::Bytes {
            len: self.bytes.len(),
            truncated: self.truncated(),
            dropped_bytes: self.dropped_bytes,
            dropped_chunks: self.dropped_chunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlobRef, BlobStore, ByteValueArtifactSink, FileValueArtifactSink, ValueArtifactRef,
        ValueArtifactSink,
    };

    fn temp_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "bamlvalue-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn bounded_sink_truncates_with_drop_diagnostics() {
        let mut sink = ByteValueArtifactSink::with_max_bytes(4);
        sink.write_chunk(b"abc").unwrap();
        sink.write_chunk(b"def").unwrap();

        assert_eq!(sink.bytes(), b"abcd");
        assert_eq!(
            sink.flush().unwrap(),
            ValueArtifactRef::Bytes {
                len: 4,
                truncated: true,
                dropped_bytes: 2,
                dropped_chunks: 1,
            }
        );
    }

    #[test]
    fn file_sink_writes_native_artifact() {
        let path = std::env::temp_dir().join(format!(
            "bamlvalue-file-sink-{}-{:?}.bamlvalue",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut sink = FileValueArtifactSink::create(&path).unwrap();
        sink.write_chunk(b"abc").unwrap();
        assert_eq!(
            sink.flush().unwrap(),
            ValueArtifactRef::NativeFile { path: path.clone() }
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn blob_store_dedupes_by_sha256_digest() {
        let root = temp_root("blob-store");
        let store = BlobStore::new(&root);

        let first = store.write_blob(b"large body").unwrap();
        let second = store.write_blob(b"large body").unwrap();

        assert_eq!(first, second);
        assert_eq!(first.algorithm, BlobRef::ALGORITHM_SHA256);
        assert_eq!(first.size_bytes, "large body".len());
        assert_eq!(
            store.read_blob(&first).unwrap(),
            Some(b"large body".to_vec())
        );

        let mut blob_files = 0;
        for algorithm_entry in std::fs::read_dir(root.join("sha256")).unwrap() {
            let algorithm_entry = algorithm_entry.unwrap();
            for blob_entry in std::fs::read_dir(algorithm_entry.path()).unwrap() {
                let blob_entry = blob_entry.unwrap();
                if blob_entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "blob")
                {
                    blob_files += 1;
                }
            }
        }
        assert_eq!(blob_files, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn blob_store_temp_names_do_not_collide_with_stale_pid_temp_file() {
        let root = temp_root("blob-store-temp");
        let store = BlobStore::new(&root);
        let bytes = b"large body";
        let blob_ref = BlobRef::sha256(bytes);
        let final_path = store.path_for(&blob_ref).unwrap();
        let parent = final_path.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        let stale_tmp = parent.join(format!(".{}.{}.tmp", blob_ref.digest, std::process::id()));
        std::fs::write(&stale_tmp, b"stale temp").unwrap();

        let written = store.write_blob(bytes).unwrap();

        assert_eq!(written, blob_ref);
        assert_eq!(std::fs::read(final_path).unwrap(), bytes);
        assert_eq!(std::fs::read(stale_tmp).unwrap(), b"stale temp");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn blob_store_rejects_unsafe_blob_refs_before_path_joining() {
        let root = temp_root("blob-store-invalid-ref");
        let store = BlobStore::new(&root);
        let valid = BlobRef::sha256(b"body");

        let bad_algorithm = BlobRef {
            algorithm: "../sha256".to_string(),
            ..valid.clone()
        };
        assert!(store.path_for(&bad_algorithm).is_err());

        let bad_digest = BlobRef {
            digest: "../not-a-digest".to_string(),
            ..valid
        };
        assert!(store.path_for(&bad_digest).is_err());
        assert!(!root.exists());
    }

    #[test]
    fn blob_store_verifies_blob_size_and_digest_on_read() {
        let root = temp_root("blob-store-integrity");
        let store = BlobStore::new(&root);
        let blob_ref = store.write_blob(b"large body").unwrap();

        let wrong_size = BlobRef {
            size_bytes: blob_ref.size_bytes + 1,
            ..blob_ref.clone()
        };
        assert_eq!(
            store.read_blob(&wrong_size).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );

        let path = store.path_for(&blob_ref).unwrap();
        std::fs::write(&path, b"tampered").unwrap();
        assert_eq!(
            store.read_blob(&blob_ref).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
