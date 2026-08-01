use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use crate::ids::BoundaryId;

use super::Cid;

const MANIFEST_MAGIC: &[u8; 4] = b"BCID";
const MANIFEST_VERSION: u16 = 1;
const MANIFEST_HEADER_LEN_U16: u16 = 32;
const MANIFEST_HEADER_LEN: usize = 32;
const MANIFEST_RECORD_MAGIC: &[u8; 4] = b"CIDR";
const MANIFEST_RECORD_LEN: usize = 40;
const MANIFEST_END_MAGIC: &[u8; 4] = b"CEND";
const MANIFEST_END_LEN: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CidManifest {
    pub boundary_id: BoundaryId,
    pub cids: Vec<Cid>,
    pub sealed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestReadOutcome {
    pub manifest: CidManifest,
    /// A partial/invalid final record is ignored but made visible.
    pub truncated: bool,
}

#[derive(Debug)]
pub struct CidManifestWriter {
    path: PathBuf,
    file: File,
    boundary_id: BoundaryId,
    count: u64,
    sealed: bool,
}

impl CidManifestWriter {
    pub fn create(path: impl AsRef<Path>, boundary_id: BoundaryId) -> io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&encode_header(boundary_id))?;
        file.sync_data()?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            boundary_id,
            count: 0,
            sealed: false,
        })
    }

    pub fn open_unsealed(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let outcome = CidManifestReader::read(path)?;
        if outcome.truncated {
            return Err(invalid_data(
                "cannot reopen a BCID manifest with a torn tail",
            ));
        }
        if outcome.manifest.sealed {
            return Err(invalid_data("cannot reopen a sealed BCID manifest"));
        }
        let file = OpenOptions::new().read(true).append(true).open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            boundary_id: outcome.manifest.boundary_id,
            count: u64::try_from(outcome.manifest.cids.len())
                .map_err(|_| invalid_data("BCID record count does not fit u64"))?,
            sealed: false,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn boundary_id(&self) -> BoundaryId {
        self.boundary_id
    }

    pub fn append(&mut self, cid: Cid) -> io::Result<()> {
        if self.sealed {
            return Err(io::Error::other("cannot append to a sealed BCID manifest"));
        }
        let mut record = [0_u8; MANIFEST_RECORD_LEN];
        record[..4].copy_from_slice(MANIFEST_RECORD_MAGIC);
        record[4..36].copy_from_slice(cid.as_bytes());
        let crc = crc32c(&record[..36]);
        record[36..40].copy_from_slice(&crc.to_le_bytes());
        self.file.write_all(&record)?;
        self.count = self.count.saturating_add(1);
        Ok(())
    }

    pub fn append_all(&mut self, cids: impl IntoIterator<Item = Cid>) -> io::Result<()> {
        for cid in cids {
            self.append(cid)?;
        }
        Ok(())
    }

    pub fn sync_data(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_data()
    }

    /// Append a seal trailer after all roots commit.
    pub fn seal(mut self) -> io::Result<PathBuf> {
        self.sync_data()?;
        let mut trailer = [0_u8; MANIFEST_END_LEN];
        trailer[..4].copy_from_slice(MANIFEST_END_MAGIC);
        trailer[4..12].copy_from_slice(&self.count.to_le_bytes());
        let crc = crc32c(&trailer[..12]);
        trailer[12..16].copy_from_slice(&crc.to_le_bytes());
        self.file.write_all(&trailer)?;
        self.file.sync_all()?;
        self.sealed = true;
        sync_parent(&self.path)?;
        Ok(self.path.clone())
    }
}

pub struct CidManifestReader;

impl CidManifestReader {
    pub fn read(path: impl AsRef<Path>) -> io::Result<ManifestReadOutcome> {
        let mut file = File::open(path)?;
        let mut header = [0_u8; MANIFEST_HEADER_LEN];
        file.read_exact(&mut header)?;
        let boundary_id = decode_header(&header)?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)?;
        let mut cids = Vec::new();
        let mut offset = 0_usize;
        let mut sealed = false;
        let mut truncated = false;
        while offset < tail.len() {
            if tail[offset..].starts_with(MANIFEST_END_MAGIC) {
                if tail.len() - offset != MANIFEST_END_LEN {
                    truncated = true;
                    break;
                }
                let trailer = &tail[offset..];
                if get_u32(trailer, 12) != crc32c(&trailer[..12])
                    || get_u64(trailer, 4) != u64::try_from(cids.len()).unwrap_or(u64::MAX)
                {
                    truncated = true;
                    break;
                }
                sealed = true;
                offset = tail.len();
                continue;
            }
            if tail.len() - offset < MANIFEST_RECORD_LEN {
                truncated = true;
                break;
            }
            let record = &tail[offset..offset + MANIFEST_RECORD_LEN];
            if &record[..4] != MANIFEST_RECORD_MAGIC || get_u32(record, 36) != crc32c(&record[..36])
            {
                truncated = true;
                break;
            }
            cids.push(Cid::from_bytes(
                record[4..36]
                    .try_into()
                    .map_err(|_| invalid_data("short BCID record CID"))?,
            ));
            offset += MANIFEST_RECORD_LEN;
        }
        Ok(ManifestReadOutcome {
            manifest: CidManifest {
                boundary_id,
                cids,
                sealed,
            },
            truncated,
        })
    }
}

fn encode_header(boundary_id: BoundaryId) -> [u8; MANIFEST_HEADER_LEN] {
    let mut header = [0_u8; MANIFEST_HEADER_LEN];
    header[..4].copy_from_slice(MANIFEST_MAGIC);
    header[4..6].copy_from_slice(&MANIFEST_VERSION.to_le_bytes());
    header[6..8].copy_from_slice(&MANIFEST_HEADER_LEN_U16.to_le_bytes());
    header[8..24].copy_from_slice(&boundary_id.as_bytes());
    let crc = crc32c(&header[..28]);
    header[28..32].copy_from_slice(&crc.to_le_bytes());
    header
}

fn decode_header(header: &[u8; MANIFEST_HEADER_LEN]) -> io::Result<BoundaryId> {
    if &header[..4] != MANIFEST_MAGIC {
        return Err(invalid_data("invalid BCID magic"));
    }
    if get_u16(header, 4) != MANIFEST_VERSION
        || usize::from(get_u16(header, 6)) != MANIFEST_HEADER_LEN
    {
        return Err(invalid_data("unsupported BCID header"));
    }
    if header[24..28].iter().any(|byte| *byte != 0) {
        return Err(invalid_data("nonzero reserved BCID header bytes"));
    }
    if get_u32(header, 28) != crc32c(&header[..28]) {
        return Err(invalid_data("BCID header CRC mismatch"));
    }
    Ok(BoundaryId::from_bytes(
        header[8..24]
            .try_into()
            .map_err(|_| invalid_data("short BCID boundary id"))?,
    ))
}

fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    File::open(parent)?.sync_all()
}

fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    !crc
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("checked slice"))
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked slice"))
}

fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked slice"))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write as _};

    use crate::{
        ids::BoundaryId,
        value_cas::{CanonicalValue, encode_value_dag},
    };

    use super::{CidManifestReader, CidManifestWriter};

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "manifest-{}-{}.bamlcids",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn manifest_round_trips_and_seals() {
        let path = temp_path();
        let boundary = BoundaryId::from_bytes([8; 16]);
        let cid = encode_value_dag(&CanonicalValue::Int(7)).unwrap().root;
        let mut writer = CidManifestWriter::create(&path, boundary).unwrap();
        writer.append(cid).unwrap();
        writer.seal().unwrap();
        let result = CidManifestReader::read(&path).unwrap();
        assert_eq!(result.manifest.boundary_id, boundary);
        assert_eq!(result.manifest.cids, vec![cid]);
        assert!(result.manifest.sealed);
        assert!(!result.truncated);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reader_reports_torn_tail_without_mutating() {
        let path = temp_path();
        let boundary = BoundaryId::from_bytes([9; 16]);
        let cid = encode_value_dag(&CanonicalValue::Int(8)).unwrap().root;
        let mut writer = CidManifestWriter::create(&path, boundary).unwrap();
        writer.append(cid).unwrap();
        writer.sync_data().unwrap();
        drop(writer);
        let committed = fs::metadata(&path).unwrap().len();
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"CID").unwrap();
        drop(file);
        let result = CidManifestReader::read(&path).unwrap();
        assert_eq!(result.manifest.cids, vec![cid]);
        assert!(result.truncated);
        assert_eq!(fs::metadata(path).unwrap().len(), committed + 3);
    }
}
