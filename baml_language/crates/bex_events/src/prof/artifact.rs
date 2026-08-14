//! Target-neutral profile artifact sink abstractions.
//!
//! Native code can persist `.bamlprof` chunks to files. WASM and browser-only
//! hosts need the same bytes without pretending they have native paths.

use std::io;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileArtifactRef {
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

pub trait ProfileArtifactSink {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<ProfileArtifactRef>;
}

#[derive(Debug, Default)]
pub struct ByteProfileArtifactSink {
    bytes: Vec<u8>,
    max_bytes: Option<usize>,
    dropped_bytes: usize,
    dropped_chunks: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteProfileArtifactStats {
    pub retained_bytes: usize,
    pub max_bytes: Option<usize>,
    pub dropped_bytes: usize,
    pub dropped_chunks: usize,
}

impl ByteProfileArtifactSink {
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
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn stats(&self) -> ByteProfileArtifactStats {
        ByteProfileArtifactStats {
            retained_bytes: self.bytes.len(),
            max_bytes: self.max_bytes,
            dropped_bytes: self.dropped_bytes,
            dropped_chunks: self.dropped_chunks,
        }
    }

    #[must_use]
    pub fn truncated(&self) -> bool {
        self.dropped_bytes > 0 || self.dropped_chunks > 0
    }
}

impl ProfileArtifactSink for ByteProfileArtifactSink {
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

    fn flush(&mut self) -> io::Result<ProfileArtifactRef> {
        Ok(ProfileArtifactRef::Bytes {
            len: self.bytes.len(),
            truncated: self.truncated(),
            dropped_bytes: self.dropped_bytes,
            dropped_chunks: self.dropped_chunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteProfileArtifactSink, ProfileArtifactRef, ProfileArtifactSink};

    #[test]
    fn byte_sink_accumulates_chunks_and_reports_length() {
        let mut sink = ByteProfileArtifactSink::new();
        sink.write_chunk(b"abc").unwrap();
        sink.write_chunk(b"def").unwrap();
        assert_eq!(sink.bytes(), b"abcdef");
        assert_eq!(
            sink.flush().unwrap(),
            ProfileArtifactRef::Bytes {
                len: 6,
                truncated: false,
                dropped_bytes: 0,
                dropped_chunks: 0
            }
        );
        assert_eq!(sink.into_bytes(), b"abcdef");
    }

    #[test]
    fn bounded_byte_sink_truncates_with_drop_diagnostics() {
        let mut sink = ByteProfileArtifactSink::with_max_bytes(5);
        sink.write_chunk(b"abc").unwrap();
        sink.write_chunk(b"defgh").unwrap();
        sink.write_chunk(b"ij").unwrap();

        assert_eq!(sink.bytes(), b"abcde");
        assert_eq!(
            sink.stats(),
            super::ByteProfileArtifactStats {
                retained_bytes: 5,
                max_bytes: Some(5),
                dropped_bytes: 5,
                dropped_chunks: 2,
            }
        );
        assert_eq!(
            sink.flush().unwrap(),
            ProfileArtifactRef::Bytes {
                len: 5,
                truncated: true,
                dropped_bytes: 5,
                dropped_chunks: 2
            }
        );
    }
}
