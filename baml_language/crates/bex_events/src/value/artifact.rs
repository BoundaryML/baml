//! Target-neutral value artifact sink abstractions.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

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
}

impl ValueArtifactSink for FileValueArtifactSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
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
        ByteValueArtifactSink, FileValueArtifactSink, ValueArtifactRef, ValueArtifactSink,
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
}
