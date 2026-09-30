//! Project/home-scoped immutable CAS publication. Concurrent recording writers
//! may race on the same digest; only a complete, closed blob acquires that name.
//! A capture's blobs are published children first, so a published blob's
//! children were already published by this writer or found in place.
use std::{
    fs,
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use btel_snapshot::{Blob, BlobScratch, CasId, Snapshot};
use rustc_hash::FxHashSet;

use crate::{LocalDeliveryError, io_error};

/// Resolve beneath the unversioned CAS root, isolating each blob format version.
pub fn cas_path(root: &Path, id: CasId) -> PathBuf {
    use std::fmt::Write as _;
    let mut name = String::with_capacity(32);
    for byte in id.as_bytes() {
        write!(&mut name, "{byte:02x}").expect("write string");
    }
    root.join(format!("v{}", btel_snapshot::BLOB_VERSION))
        .join(&name[..2])
        .join(&name[2..4])
        .join(&name[4..6])
        .join(name)
}

/// Publishes captures beneath one CAS root. Blobs it published or found in
/// place are not checked again: like the recent-capture window, this assumes
/// nothing removes blobs while a recording runs.
pub(super) struct CasWriter {
    root: PathBuf,
    scratch: BlobScratch,
    /// Bounded by [`KNOWN_BLOB_IDS`](btel_settings::local_files::KNOWN_BLOB_IDS).
    known: FxHashSet<CasId>,
}

impl CasWriter {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            scratch: BlobScratch::default(),
            known: FxHashSet::default(),
        }
    }

    pub(super) fn write(&mut self, snapshot: &Snapshot) -> Result<(), LocalDeliveryError> {
        for blob in snapshot.blobs() {
            if self.known.contains(&blob.id()) {
                continue;
            }
            write_blob(&self.root, blob, &mut self.scratch)?;
            if self.known.len() >= btel_settings::local_files::KNOWN_BLOB_IDS {
                self.known.clear();
            }
            self.known.insert(blob.id());
        }
        Ok(())
    }
}

fn write_blob(
    root: &Path,
    blob: Blob<'_>,
    scratch: &mut BlobScratch,
) -> Result<(), LocalDeliveryError> {
    let destination = cas_path(root, blob.id());
    match fs::File::open(&destination) {
        Ok(mut existing) => {
            // Existing immutable entries are reused. Validate their envelope;
            // full graph validation belongs to the offline decoder, not delivery.
            let mut header = [0_u8; 28];
            existing
                .read_exact(&mut header)
                .map_err(|e| io_error(&destination, &e))?;
            if header[..8] != btel_snapshot::BLOB_MAGIC
                || header[8..12] != btel_snapshot::BLOB_VERSION.to_le_bytes()
                || header[12..] != *blob.id().as_bytes()
            {
                return Err(LocalDeliveryError(format!(
                    "invalid CAS header: {}",
                    destination.display()
                )));
            }
            return Ok(());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&destination, &error)),
    }
    let directory = destination.parent().expect("sharded CAS path");
    fs::create_dir_all(directory).map_err(|e| io_error(directory, &e))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".btel-")
        .suffix(".part")
        .tempfile_in(directory)
        .map_err(|e| io_error(directory, &e))?;
    {
        let mut writer = BufWriter::with_capacity(
            btel_settings::local_files::CAS_WRITE_BUFFER_BYTES,
            temp.as_file_mut(),
        );
        blob.write(scratch, &mut writer)
            .map_err(|e| io_error(&destination, &e))?;
        writer.flush().map_err(|e| io_error(&destination, &e))?;
    }
    // Closing first and no-clobber publication work for shared roots as well as
    // one writer. TempPath cleans up partial files on error/racing publication.
    match temp.into_temp_path().persist_noclobber(&destination) {
        Ok(()) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(io_error(&destination, &error.error)),
    }
}

#[cfg(test)]
mod tests {
    use btel_snapshot::{Limits, SnapshotPool, SnapshotValue};

    use super::*;

    #[test]
    fn known_blobs_are_forgotten_at_capacity() {
        let root = tempfile::tempdir().unwrap();
        let pool = SnapshotPool::new(2, Limits::default());
        let snapshot = |n| {
            pool.try_acquire()
                .unwrap()
                .finish_value(SnapshotValue::Int(n), &mut btel_snapshot::Shaper::default())
        };
        let mut writer = CasWriter::new(root.path().to_owned());
        let capacity = btel_settings::local_files::KNOWN_BLOB_IDS;
        writer.known.extend((1..capacity).map(|n| {
            let mut id = [0; 16];
            id[..8].copy_from_slice(&u64::try_from(n).unwrap().to_le_bytes());
            CasId::from_bytes(id)
        }));
        let (first, second) = (snapshot(1), snapshot(2));
        writer.write(&first).unwrap();
        assert_eq!(writer.known.len(), capacity);
        writer.write(&second).unwrap();
        assert_eq!(
            writer.known.iter().copied().collect::<Vec<_>>(),
            [second.root_id()]
        );
        for snapshot in [&first, &second] {
            assert!(cas_path(root.path(), snapshot.root_id()).exists());
        }
    }
}
