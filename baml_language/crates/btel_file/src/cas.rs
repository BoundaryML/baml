//! Project/home-scoped immutable CAS publication. Concurrent recording writers
//! may race on the same digest; only a complete, closed blob acquires that name.
use std::{
    fs,
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use btel_snapshot::{Snapshot, SnapshotId};

use crate::{LocalDeliveryError, io_error};

/// Resolve beneath the unversioned CAS root, isolating each blob format version.
pub fn cas_path(root: &Path, id: SnapshotId) -> PathBuf {
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

pub(super) fn write_snapshot(root: &Path, snapshot: &Snapshot) -> Result<(), LocalDeliveryError> {
    let destination = cas_path(root, snapshot.id());
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
                || header[12..] != *snapshot.id().as_bytes()
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
        snapshot
            .write_blob(&mut writer)
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
