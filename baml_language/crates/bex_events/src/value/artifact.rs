//! Target-neutral value artifact sink abstractions.

use std::io;

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
    use super::{ByteValueArtifactSink, ValueArtifactRef, ValueArtifactSink};

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
}
