//! Read and validate one completed recording file.
use std::{fs, io::Read, path::Path};

use btel_recorder::{RecordingId, proto};

/// A validated file with the identity of the exact bytes that were decoded.
pub struct FileRead {
    pub len: u64,
    /// XXH3-128 of the file contents: change detection, not a checksum the
    /// producer vouches for.
    pub content_hash: [u8; 16],
    pub file: proto::RecordingFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileError {
    /// The file vanished or could not be read.
    Io(String),
    TooLarge {
        len: u64,
        limit: u64,
    },
    /// Undecodable, or inconsistent with its name/recording.
    Invalid(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read recording file: {error}"),
            Self::TooLarge { len, limit } => {
                write!(
                    f,
                    "recording file is {len} bytes; the reader limit is {limit}"
                )
            }
            Self::Invalid(reason) => write!(f, "invalid recording file: {reason}"),
        }
    }
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, FileError> {
    let file = fs::File::open(path).map_err(|e| FileError::Io(e.to_string()))?;
    let len = file
        .metadata()
        .map_err(|e| FileError::Io(e.to_string()))?
        .len();
    if len > max_bytes {
        return Err(FileError::TooLarge {
            len,
            limit: max_bytes,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    // Bound the read even if the file grows after the size check.
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| FileError::Io(e.to_string()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(FileError::TooLarge {
            len: bytes.len() as u64,
            limit: max_bytes,
        });
    }
    Ok(bytes)
}

pub fn content_hash(bytes: &[u8]) -> [u8; 16] {
    xxhash_rust::xxh3::xxh3_128(bytes).to_le_bytes()
}

/// Read, hash and validate a completed file.
pub fn read_file(
    path: &Path,
    id: RecordingId,
    sequence: u64,
    max_bytes: u64,
) -> Result<FileRead, FileError> {
    let bytes = read_bounded(path, max_bytes)?;
    let file = btel_file::validate_file(&bytes, id, sequence).map_err(FileError::Invalid)?;
    Ok(FileRead {
        len: bytes.len() as u64,
        content_hash: content_hash(&bytes),
        file,
    })
}

/// Hash contents without decoding, for files whose metadata changed.
pub fn hash_file(path: &Path, max_bytes: u64) -> Result<(u64, [u8; 16]), FileError> {
    let bytes = read_bounded(path, max_bytes)?;
    Ok((bytes.len() as u64, content_hash(&bytes)))
}
