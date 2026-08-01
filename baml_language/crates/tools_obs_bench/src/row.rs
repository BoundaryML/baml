use std::path::Path;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ArtifactIdentity {
    pub(crate) path: String,
    pub(crate) bytes: u64,
}

impl ArtifactIdentity {
    pub(crate) fn read(path: &Path) -> std::io::Result<(Self, Vec<u8>)> {
        let bytes = std::fs::read(path)?;
        Ok((
            Self {
                path: path.display().to_string(),
                bytes: bytes.len() as u64,
            },
            bytes,
        ))
    }
}
