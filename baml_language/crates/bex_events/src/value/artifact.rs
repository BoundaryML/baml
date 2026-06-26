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

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn path_for(&self, blob_ref: &BlobRef) -> PathBuf {
        let prefix = blob_ref.digest.get(..2).unwrap_or("xx");
        self.root
            .join(&blob_ref.algorithm)
            .join(prefix)
            .join(format!("{}.blob", blob_ref.digest))
    }

    pub fn write_blob(&self, bytes: &[u8]) -> io::Result<BlobRef> {
        let blob_ref = BlobRef::sha256(bytes);
        let path = self.path_for(&blob_ref);
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
        let path = self.path_for(blob_ref);
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
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
}

#[derive(Debug)]
pub struct FileValueArtifactSink {
    file: File,
    path: PathBuf,
    bytes_written: u64,
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
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sync(&mut self) -> io::Result<ValueArtifactRef> {
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(ValueArtifactRef::NativeFile {
            path: self.path.clone(),
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
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
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
        let root = std::env::temp_dir().join(format!(
            "bamlvalue-blob-store-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
        let root = std::env::temp_dir().join(format!(
            "bamlvalue-blob-store-temp-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = BlobStore::new(&root);
        let bytes = b"large body";
        let blob_ref = BlobRef::sha256(bytes);
        let final_path = store.path_for(&blob_ref);
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
}
