use std::io;

pub const BCCT_HEADER_LEN: usize = 112;
pub const BLOCK_HEADER_LEN: usize = 32;
pub const BLOCK_TRAILER_LEN: usize = 16;
pub const BCCT_TRAILER_LEN: usize = 48;
pub const BCCT_FORMAT_VERSION: u16 = 1;
pub(crate) const BLOCK_COMMIT_MARKER: u64 = u64::from_le_bytes(*b"BCCTCMIT");

const HEADER_MAGIC: &[u8; 4] = b"BCCT";
const BLOCK_MAGIC: &[u8; 4] = b"DBLK";
const FOOTER_MAGIC: &[u8; 8] = b"BCCTFOOT";
const FOOTER_END_MAGIC: &[u8; 4] = b"TSEG";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockDescriptor {
    pub kind: u8,
    pub quality: u8,
    pub tick_ns_numer: u32,
    pub tick_ns_denom: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BcctHeader {
    pub process_euid: [u8; 16],
    pub engine_id: u64,
    pub session_seg_seq: u32,
    pub started_epoch_ns: u64,
    pub clock: ClockDescriptor,
    pub revision_id: [u8; 32],
}

impl BcctHeader {
    #[must_use]
    pub fn encode(&self) -> [u8; BCCT_HEADER_LEN] {
        let mut bytes = [0_u8; BCCT_HEADER_LEN];
        bytes[0..4].copy_from_slice(HEADER_MAGIC);
        put_u16(&mut bytes, 4, BCCT_FORMAT_VERSION);
        put_u16(
            &mut bytes,
            6,
            u16::try_from(BCCT_HEADER_LEN).expect("fixed header length fits u16"),
        );
        bytes[8..24].copy_from_slice(&self.process_euid);
        put_u64(&mut bytes, 24, self.engine_id);
        put_u32(&mut bytes, 32, self.session_seg_seq);
        put_u32(&mut bytes, 36, 64);
        put_u64(&mut bytes, 40, self.started_epoch_ns);
        bytes[48] = self.clock.kind;
        bytes[49] = self.clock.quality;
        put_u32(&mut bytes, 52, self.clock.tick_ns_numer);
        put_u32(&mut bytes, 56, self.clock.tick_ns_denom);
        bytes[60..92].copy_from_slice(&self.revision_id);
        let crc = crc32c(&bytes[..108]);
        put_u32(&mut bytes, 108, crc);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < BCCT_HEADER_LEN {
            return Err(invalid_data("truncated BCCT header"));
        }
        if &bytes[0..4] != HEADER_MAGIC {
            return Err(invalid_data("invalid BCCT header magic"));
        }
        if get_u16(bytes, 4) != BCCT_FORMAT_VERSION {
            return Err(invalid_data("unsupported BCCT format version"));
        }
        if usize::from(get_u16(bytes, 6)) != BCCT_HEADER_LEN {
            return Err(invalid_data("invalid BCCT header length"));
        }
        if get_u32(bytes, 36) != 64 {
            return Err(invalid_data("unsupported BCCT block alignment"));
        }
        if get_u32(bytes, 108) != crc32c(&bytes[..108]) {
            return Err(invalid_data("BCCT header CRC mismatch"));
        }
        Ok(Self {
            process_euid: bytes[8..24].try_into().expect("fixed-width header field"),
            engine_id: get_u64(bytes, 24),
            session_seg_seq: get_u32(bytes, 32),
            started_epoch_ns: get_u64(bytes, 40),
            clock: ClockDescriptor {
                kind: bytes[48],
                quality: bytes[49],
                tick_ns_numer: get_u32(bytes, 52),
                tick_ns_denom: get_u32(bytes, 56),
            },
            revision_id: bytes[60..92].try_into().expect("fixed-width header field"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockKind {
    CctDelta = 1,
    NodeBirth = 2,
    SpawnEdge = 3,
    Watermark = 4,
    PartitionBind = 5,
    FooterIndex = 6,
    Reserved7 = 7,
    NodeTotal = 8,
    CctHistogram = 9,
    LlmDelta = 10,
    ModelBirth = 11,
    Marker = 12,
    Instance = 13,
}

impl BlockKind {
    #[must_use]
    pub fn from_raw(raw: u8) -> Option<Self> {
        Some(match raw {
            1 => Self::CctDelta,
            2 => Self::NodeBirth,
            3 => Self::SpawnEdge,
            4 => Self::Watermark,
            5 => Self::PartitionBind,
            6 => Self::FooterIndex,
            7 => Self::Reserved7,
            8 => Self::NodeTotal,
            9 => Self::CctHistogram,
            10 => Self::LlmDelta,
            11 => Self::ModelBirth,
            12 => Self::Marker,
            13 => Self::Instance,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub kind: u8,
    pub flags: u8,
    pub row_count: u32,
    pub payload_len: u32,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
}

impl BlockHeader {
    #[must_use]
    pub fn encode(self) -> [u8; BLOCK_HEADER_LEN] {
        let mut bytes = [0_u8; BLOCK_HEADER_LEN];
        bytes[0..4].copy_from_slice(BLOCK_MAGIC);
        bytes[4] = self.kind;
        bytes[5] = self.flags;
        put_u32(&mut bytes, 8, self.row_count);
        put_u32(&mut bytes, 12, self.payload_len);
        put_u64(&mut bytes, 16, self.first_ts_ns);
        put_u64(&mut bytes, 24, self.last_ts_ns);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < BLOCK_HEADER_LEN {
            return Err(invalid_data("truncated BCCT block header"));
        }
        if &bytes[..4] != BLOCK_MAGIC {
            return Err(invalid_data("invalid BCCT block magic"));
        }
        if bytes[6] != 0 || bytes[7] != 0 {
            return Err(invalid_data("non-zero BCCT block reserved bytes"));
        }
        Ok(Self {
            kind: bytes[4],
            flags: bytes[5],
            row_count: get_u32(bytes, 8),
            payload_len: get_u32(bytes, 12),
            first_ts_ns: get_u64(bytes, 16),
            last_ts_ns: get_u64(bytes, 24),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FooterTrailer {
    pub index_offset: u64,
    pub index_len: u64,
    pub total_rows: u64,
}

impl FooterTrailer {
    #[must_use]
    pub fn encode(self) -> [u8; BCCT_TRAILER_LEN] {
        let mut bytes = [0_u8; BCCT_TRAILER_LEN];
        bytes[..8].copy_from_slice(FOOTER_MAGIC);
        put_u64(&mut bytes, 8, self.index_offset);
        put_u64(&mut bytes, 16, self.index_len);
        put_u64(&mut bytes, 24, self.total_rows);
        let crc = crc32c(&bytes[..40]);
        put_u32(&mut bytes, 40, crc);
        bytes[44..48].copy_from_slice(FOOTER_END_MAGIC);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != BCCT_TRAILER_LEN {
            return Err(invalid_data("truncated BCCT footer trailer"));
        }
        if &bytes[..8] != FOOTER_MAGIC || &bytes[44..48] != FOOTER_END_MAGIC {
            return Err(invalid_data("invalid BCCT footer trailer magic"));
        }
        if get_u32(bytes, 40) != crc32c(&bytes[..40]) {
            return Err(invalid_data("BCCT footer trailer CRC mismatch"));
        }
        Ok(Self {
            index_offset: get_u64(bytes, 8),
            index_len: get_u64(bytes, 16),
            total_rows: get_u64(bytes, 24),
        })
    }
}

/// Castagnoli CRC-32C, with the conventional initial/final complement.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    crc32c_slices(&[bytes])
}

pub(crate) fn crc32c_slices(parts: &[&[u8]]) -> u32 {
    let mut crc = !0_u32;
    for bytes in parts {
        for byte in *bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
            }
        }
    }
    !crc
}

pub(crate) fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(crate) fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("checked slice"))
}

pub(crate) fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked slice"))
}

pub(crate) fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked slice"))
}

pub(crate) fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
