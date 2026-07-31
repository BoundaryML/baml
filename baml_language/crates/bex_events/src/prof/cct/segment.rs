//! The BCCT segment container (§6.2): one custom framing for the active
//! (WAL-shaped) and sealed roles. Columns are contiguous fixed-width
//! little-endian arrays, 8-byte aligned, so zero-copy Arrow views are
//! possible without the Arrow container.
//!
//! - Header: 112 B, fsynced at create.
//! - Block: 32 B header (`DBLK`) + column-major payload + 16 B trailer
//!   (crc32c over header+payload, monotonic block_seq, commit marker).
//!   A block is committed iff magic, CRC, seq, and marker all validate.
//! - Recovery: scan from the header, accept committed blocks, stop at the
//!   first failure. Reads never mutate.
//! - Seal (by append — no rewrite, no crash window): a `footer_index`
//!   block, then the 48 B `BCCTFOOT ... TSEG` trailer. A valid trailer ⇒
//!   sealed (mmap + index); anything else ⇒ active/torn ⇒ block scan.
//!
//! The writer is sink-generic: native files and the wasm in-memory sink
//! share every byte of framing (§5.12).

use super::crc32c::crc32c;

pub const SEGMENT_MAGIC: [u8; 4] = *b"BCCT";
pub const BLOCK_MAGIC: [u8; 4] = *b"DBLK";
pub const FOOTER_MAGIC: [u8; 8] = *b"BCCTFOOT";
pub const END_MAGIC: [u8; 4] = *b"TSEG";
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 112;
pub const BLOCK_HEADER_LEN: usize = 32;
pub const BLOCK_TRAILER_LEN: usize = 16;
pub const SEAL_TRAILER_LEN: usize = 48;
/// Blocks start on this alignment (zero-padded between blocks).
pub const BLOCK_ALIGN: usize = 64;
/// The §6.2 commit marker: the final qword of a committed block.
pub const COMMIT_MARKER: u64 = 0xB10C_C077_17ED_B10C;

/// §6.3 block kinds (v1). Readers skip unknown kinds (forward compat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockKind {
    CctDelta = 1,
    NodeBirth = 2,
    SpawnEdge = 3,
    Watermark = 4,
    PartitionBind = 5,
    FooterIndex = 6,
    // 7 reserved (the seal trailer is out-of-band, not a framed block).
    NodeTotal = 8,
    CctHist = 9,
    LlmDelta = 10,
    ModelBirth = 11,
    Marker = 12,
    Instance = 13,
}

impl BlockKind {
    #[must_use]
    pub fn from_u8(value: u8) -> Option<BlockKind> {
        Some(match value {
            1 => BlockKind::CctDelta,
            2 => BlockKind::NodeBirth,
            3 => BlockKind::SpawnEdge,
            4 => BlockKind::Watermark,
            5 => BlockKind::PartitionBind,
            6 => BlockKind::FooterIndex,
            8 => BlockKind::NodeTotal,
            9 => BlockKind::CctHist,
            10 => BlockKind::LlmDelta,
            11 => BlockKind::ModelBirth,
            12 => BlockKind::Marker,
            13 => BlockKind::Instance,
            _ => return None,
        })
    }
}

/// The 112-byte segment header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentHeader {
    pub process_euid: [u8; 16],
    pub engine_id: u64,
    pub session_seg_seq: u32,
    pub started_epoch_ns: u64,
    pub clock_kind: u8,
    pub clock_quality: u8,
    pub tick_ns_numer: u64,
    pub tick_ns_denom: u64,
    pub revision_id: [u8; 32],
}

impl SegmentHeader {
    #[must_use]
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..4].copy_from_slice(&SEGMENT_MAGIC);
        out[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        out[6..8].copy_from_slice(&u16::try_from(HEADER_LEN).unwrap().to_le_bytes());
        out[8..24].copy_from_slice(&self.process_euid);
        out[24..32].copy_from_slice(&self.engine_id.to_le_bytes());
        out[32..36].copy_from_slice(&self.session_seg_seq.to_le_bytes());
        out[36..40].copy_from_slice(&u32::try_from(BLOCK_ALIGN).unwrap().to_le_bytes());
        out[40..48].copy_from_slice(&self.started_epoch_ns.to_le_bytes());
        out[48] = self.clock_kind;
        out[49] = self.clock_quality;
        // 50..52 reserved padding.
        out[52..60].copy_from_slice(&self.tick_ns_numer.to_le_bytes());
        out[60..68].copy_from_slice(&self.tick_ns_denom.to_le_bytes());
        out[68..100].copy_from_slice(&self.revision_id);
        // 100..108 reserved.
        let crc = crc32c(&out[0..108]);
        out[108..112].copy_from_slice(&crc.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<SegmentHeader, SegmentError> {
        if bytes.len() < HEADER_LEN {
            return Err(SegmentError::TruncatedHeader);
        }
        if bytes[0..4] != SEGMENT_MAGIC {
            return Err(SegmentError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != FORMAT_VERSION {
            return Err(SegmentError::UnsupportedVersion(version));
        }
        let stored_crc = u32::from_le_bytes(bytes[108..112].try_into().unwrap());
        if crc32c(&bytes[0..108]) != stored_crc {
            return Err(SegmentError::HeaderCrc);
        }
        Ok(SegmentHeader {
            process_euid: bytes[8..24].try_into().unwrap(),
            engine_id: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            session_seg_seq: u32::from_le_bytes(bytes[32..36].try_into().unwrap()),
            started_epoch_ns: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            clock_kind: bytes[48],
            clock_quality: bytes[49],
            tick_ns_numer: u64::from_le_bytes(bytes[52..60].try_into().unwrap()),
            tick_ns_denom: u64::from_le_bytes(bytes[60..68].try_into().unwrap()),
            revision_id: bytes[68..100].try_into().unwrap(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentError {
    TruncatedHeader,
    BadMagic,
    UnsupportedVersion(u16),
    HeaderCrc,
}

/// One decoded block (payload borrowed from the segment bytes).
#[derive(Debug, Clone, Copy)]
pub struct Block<'a> {
    pub kind: u8,
    pub flags: u8,
    pub row_count: u32,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub block_seq: u32,
    pub payload: &'a [u8],
    /// Byte offset of the block header in the segment.
    pub offset: usize,
}

/// Where and why a scan stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEnd {
    /// Clean end at the sealed footer trailer.
    Sealed,
    /// Ran off the end of the bytes with every block committed (active
    /// segment, cleanly flushed so far).
    ActiveEnd,
    /// A torn/uncommitted tail begins at this offset; bytes before it are
    /// fully committed ("aggregates complete through T; tail lost ≤ delta").
    Torn { offset: usize },
}

/// A scanned segment: header + committed blocks + how it ended.
pub struct SegmentContents<'a> {
    pub header: SegmentHeader,
    pub blocks: Vec<Block<'a>>,
    pub end: ScanEnd,
}

/// Encode one framed block (header + payload + trailer, plus leading pad
/// to [`BLOCK_ALIGN`] from `at_offset`).
#[must_use]
pub fn encode_block(
    at_offset: usize,
    kind: BlockKind,
    flags: u8,
    row_count: u32,
    first_ts_ns: u64,
    last_ts_ns: u64,
    payload: &[u8],
    block_seq: u32,
) -> Vec<u8> {
    let pad = at_offset.next_multiple_of(BLOCK_ALIGN) - at_offset;
    let mut out = vec![0u8; pad];
    let mut header = [0u8; BLOCK_HEADER_LEN];
    header[0..4].copy_from_slice(&BLOCK_MAGIC);
    header[4] = kind as u8;
    header[5] = flags;
    // 6..8 reserved padding.
    header[8..12].copy_from_slice(&row_count.to_le_bytes());
    header[12..16].copy_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
    header[16..24].copy_from_slice(&first_ts_ns.to_le_bytes());
    header[24..32].copy_from_slice(&last_ts_ns.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(payload);
    let mut crc_input = Vec::with_capacity(BLOCK_HEADER_LEN + payload.len());
    crc_input.extend_from_slice(&header);
    crc_input.extend_from_slice(payload);
    let crc = crc32c(&crc_input);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&block_seq.to_le_bytes());
    out.extend_from_slice(&COMMIT_MARKER.to_le_bytes());
    out
}

/// Encode the 48-byte seal trailer.
#[must_use]
pub fn encode_seal_trailer(
    index_offset: u64,
    index_len: u64,
    total_rows: u64,
) -> [u8; SEAL_TRAILER_LEN] {
    let mut out = [0u8; SEAL_TRAILER_LEN];
    out[0..8].copy_from_slice(&FOOTER_MAGIC);
    out[8..16].copy_from_slice(&index_offset.to_le_bytes());
    out[16..24].copy_from_slice(&index_len.to_le_bytes());
    out[24..32].copy_from_slice(&total_rows.to_le_bytes());
    // 32..40 reserved.
    let crc = crc32c(&out[0..40]);
    out[40..44].copy_from_slice(&crc.to_le_bytes());
    out[44..48].copy_from_slice(&END_MAGIC);
    out
}

/// Scan a segment's bytes: header, then committed blocks until the seal
/// trailer, the end of bytes, or the first torn block. Never mutates.
pub fn scan_segment(bytes: &[u8]) -> Result<SegmentContents<'_>, SegmentError> {
    let header = SegmentHeader::decode(bytes)?;
    let mut blocks = Vec::new();
    let mut offset = HEADER_LEN;
    let mut expected_seq: u32 = 0;
    // Sealed fast path: a valid trailer at the very end.
    let sealed = bytes.len() >= SEAL_TRAILER_LEN && {
        let tail = &bytes[bytes.len() - SEAL_TRAILER_LEN..];
        tail[0..8] == FOOTER_MAGIC
            && tail[44..48] == END_MAGIC
            && crc32c(&tail[0..40]) == u32::from_le_bytes(tail[40..44].try_into().unwrap())
    };
    let scan_end = if sealed {
        bytes.len() - SEAL_TRAILER_LEN
    } else {
        bytes.len()
    };
    loop {
        let aligned = offset.next_multiple_of(BLOCK_ALIGN);
        if aligned >= scan_end {
            return Ok(SegmentContents {
                header,
                blocks,
                end: if sealed {
                    ScanEnd::Sealed
                } else {
                    ScanEnd::ActiveEnd
                },
            });
        }
        let start = aligned;
        let end = match block_end(bytes, start, scan_end) {
            Some(end) => end,
            None => {
                return Ok(SegmentContents {
                    header,
                    blocks,
                    end: if sealed {
                        // A sealed segment with a bad block is corruption,
                        // but the committed prefix is still served; the
                        // torn offset names the damage.
                        ScanEnd::Torn { offset: start }
                    } else {
                        ScanEnd::Torn { offset: start }
                    },
                });
            }
        };
        let head = &bytes[start..start + BLOCK_HEADER_LEN];
        let payload_len = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
        let payload = &bytes[start + BLOCK_HEADER_LEN..start + BLOCK_HEADER_LEN + payload_len];
        let trailer = &bytes[end - BLOCK_TRAILER_LEN..end];
        let block_seq = u32::from_le_bytes(trailer[4..8].try_into().unwrap());
        if block_seq != expected_seq {
            return Ok(SegmentContents {
                header,
                blocks,
                end: ScanEnd::Torn { offset: start },
            });
        }
        expected_seq += 1;
        blocks.push(Block {
            kind: head[4],
            flags: head[5],
            row_count: u32::from_le_bytes(head[8..12].try_into().unwrap()),
            first_ts_ns: u64::from_le_bytes(head[16..24].try_into().unwrap()),
            last_ts_ns: u64::from_le_bytes(head[24..32].try_into().unwrap()),
            block_seq,
            payload,
            offset: start,
        });
        offset = end;
    }
}

/// Validate the block at `start`; return its end offset when committed.
fn block_end(bytes: &[u8], start: usize, limit: usize) -> Option<usize> {
    if start + BLOCK_HEADER_LEN > limit {
        return None;
    }
    let head = &bytes[start..start + BLOCK_HEADER_LEN];
    if head[0..4] != BLOCK_MAGIC {
        return None;
    }
    let payload_len = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    let end = start
        .checked_add(BLOCK_HEADER_LEN)?
        .checked_add(payload_len)?
        .checked_add(BLOCK_TRAILER_LEN)?;
    if end > limit {
        return None;
    }
    let trailer = &bytes[end - BLOCK_TRAILER_LEN..end];
    let marker = u64::from_le_bytes(trailer[8..16].try_into().ok()?);
    if marker != COMMIT_MARKER {
        return None;
    }
    let stored_crc = u32::from_le_bytes(trailer[0..4].try_into().ok()?);
    let crc = super::crc32c::extend(0, &bytes[start..end - BLOCK_TRAILER_LEN]);
    if crc != stored_crc {
        return None;
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> SegmentHeader {
        SegmentHeader {
            process_euid: [7; 16],
            engine_id: 42,
            session_seg_seq: 1,
            started_epoch_ns: 1_700_000_000,
            clock_kind: 3,
            clock_quality: 1,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
            revision_id: [9; 32],
        }
    }

    fn segment_with_blocks(payloads: &[&[u8]]) -> Vec<u8> {
        let mut bytes = test_header().encode().to_vec();
        for (seq, payload) in payloads.iter().enumerate() {
            let block = encode_block(
                bytes.len(),
                BlockKind::CctDelta,
                0,
                1,
                10,
                20,
                payload,
                u32::try_from(seq).unwrap(),
            );
            bytes.extend_from_slice(&block);
        }
        bytes
    }

    #[test]
    fn header_roundtrips_and_rejects_corruption() {
        let header = test_header();
        let bytes = header.encode();
        assert_eq!(SegmentHeader::decode(&bytes).unwrap(), header);
        let mut bad = bytes;
        bad[30] ^= 1;
        assert_eq!(SegmentHeader::decode(&bad), Err(SegmentError::HeaderCrc));
        assert_eq!(
            SegmentHeader::decode(&bytes[..50]),
            Err(SegmentError::TruncatedHeader)
        );
    }

    #[test]
    fn active_scan_accepts_committed_blocks() {
        let bytes = segment_with_blocks(&[b"aaaaaaa", b"bbbbbbbbbbb"]);
        let contents = scan_segment(&bytes).unwrap();
        assert_eq!(contents.blocks.len(), 2);
        assert_eq!(contents.end, ScanEnd::ActiveEnd);
        assert_eq!(contents.blocks[0].payload, b"aaaaaaa");
        assert_eq!(contents.blocks[1].block_seq, 1);
        // Blocks start 64-aligned.
        assert_eq!(contents.blocks[0].offset % BLOCK_ALIGN, 0);
        assert_eq!(contents.blocks[1].offset % BLOCK_ALIGN, 0);
    }

    #[test]
    fn torn_tail_truncates_to_last_committed_block_at_every_offset() {
        let bytes = segment_with_blocks(&[b"aaaaaaa", b"bbbbbbbbbbb"]);
        let first_block_end = {
            let contents = scan_segment(&bytes).unwrap();
            let first = contents.blocks[0];
            first.offset + BLOCK_HEADER_LEN + first.payload.len() + BLOCK_TRAILER_LEN
        };
        for cut in HEADER_LEN..bytes.len() {
            let contents = scan_segment(&bytes[..cut]).unwrap();
            if cut < bytes.len() {
                assert!(
                    contents.blocks.len() < 2 || cut >= bytes.len(),
                    "cut {cut} kept both blocks"
                );
            }
            if cut < first_block_end {
                assert!(contents.blocks.is_empty(), "cut {cut} inside first block");
            }
            // Never an error, never a partial block.
            for block in &contents.blocks {
                assert!(block.payload == b"aaaaaaa" || block.payload == b"bbbbbbbbbbb");
            }
        }
    }

    #[test]
    fn corrupt_middle_block_stops_scan_at_its_offset() {
        let mut bytes = segment_with_blocks(&[b"aaaaaaa", b"bbbbbbbbbbb"]);
        let second_offset = scan_segment(&bytes).unwrap().blocks[1].offset;
        bytes[second_offset + BLOCK_HEADER_LEN] ^= 0xFF; // flip payload byte
        let contents = scan_segment(&bytes).unwrap();
        assert_eq!(contents.blocks.len(), 1);
        assert_eq!(
            contents.end,
            ScanEnd::Torn {
                offset: second_offset
            }
        );
    }

    #[test]
    fn seal_trailer_marks_sealed_and_scan_still_reads_blocks() {
        let mut bytes = segment_with_blocks(&[b"aaaaaaa"]);
        let index_block = encode_block(bytes.len(), BlockKind::FooterIndex, 0, 0, 0, 0, b"", 1);
        let index_offset = bytes.len();
        bytes.extend_from_slice(&index_block);
        let trailer = encode_seal_trailer(index_offset as u64, index_block.len() as u64, 1);
        bytes.extend_from_slice(&trailer);

        let contents = scan_segment(&bytes).unwrap();
        assert_eq!(contents.end, ScanEnd::Sealed);
        assert_eq!(contents.blocks.len(), 2, "footer_index is a normal block");

        // A seq gap is torn even in a sealed file.
        let mut gap = segment_with_blocks(&[b"aaaaaaa"]);
        let bad_block = encode_block(gap.len(), BlockKind::CctDelta, 0, 1, 0, 0, b"x", 7);
        gap.extend_from_slice(&bad_block);
        let contents = scan_segment(&gap).unwrap();
        assert_eq!(contents.blocks.len(), 1);
        assert!(matches!(contents.end, ScanEnd::Torn { .. }));
    }
}
