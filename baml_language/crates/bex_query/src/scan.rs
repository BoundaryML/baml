use bex_events::prof::storage::{
    BCCT_HEADER_LEN, BCCT_TRAILER_LEN, BLOCK_HEADER_LEN, BLOCK_TRAILER_LEN, BcctHeader,
    BlockHeader, BlockKind, BlockRows, FooterTrailer, ScannedBlock, SegmentState, crc32c,
};

use crate::{ByteRange, ByteSource, FileId, QueryError, QueryPoll, SourceSnapshot};

const MAX_PAYLOAD_LEN: usize = 256 * 1024 * 1024;
const BLOCK_COMMIT_MARKER: u64 = u64::from_le_bytes(*b"BCCTCMIT");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BcctScan {
    pub file: FileId,
    pub source: SourceSnapshot,
    pub header: BcctHeader,
    pub blocks: Vec<ScannedBlock>,
    pub committed_len: u64,
    pub state: SegmentState,
}

#[must_use = "the caller must service NeedData before retrying"]
pub fn scan_bcct(source: &dyn ByteSource, file: FileId) -> Result<QueryPoll<BcctScan>, QueryError> {
    let committed_len = source.committed_len(file);
    let generation = source.generation(file);
    let snapshot = SourceSnapshot {
        committed_len,
        generation,
    };
    if committed_len < BCCT_HEADER_LEN as u64 {
        return Err(QueryError::invalid_data(format!(
            "BCCT file {} has a truncated header ({committed_len} bytes)",
            file.0
        )));
    }
    let Some(header_bytes) = view(source, file, 0, BCCT_HEADER_LEN as u64)? else {
        return Ok(need(file, 0, BCCT_HEADER_LEN as u64));
    };
    let header = BcctHeader::decode(&header_bytes)?;

    let mut footer = None;
    let mut scan_limit = committed_len;
    if committed_len >= (BCCT_HEADER_LEN + BCCT_TRAILER_LEN) as u64 {
        let trailer_start = committed_len - BCCT_TRAILER_LEN as u64;
        let Some(trailer_bytes) = view(source, file, trailer_start, committed_len)? else {
            return Ok(need(file, trailer_start, committed_len));
        };
        if let Ok(candidate) = FooterTrailer::decode(&trailer_bytes) {
            footer = Some(candidate);
            scan_limit = trailer_start;
        }
    }

    let mut offset = BCCT_HEADER_LEN as u64;
    let mut expected_seq = 1_u32;
    let mut blocks = Vec::new();
    let mut stopped_on_failure = false;
    while offset < scan_limit {
        let header_end = offset.saturating_add(BLOCK_HEADER_LEN as u64);
        if header_end > scan_limit {
            stopped_on_failure = true;
            break;
        }
        let Some(block_header_bytes) = view(source, file, offset, header_end)? else {
            return Ok(need(file, offset, header_end));
        };
        let Ok(block_header) = BlockHeader::decode(&block_header_bytes) else {
            stopped_on_failure = true;
            break;
        };
        let payload_len = block_header.payload_len as usize;
        if payload_len > MAX_PAYLOAD_LEN || !payload_len.is_multiple_of(8) {
            stopped_on_failure = true;
            break;
        }
        let payload_end = header_end.saturating_add(payload_len as u64);
        let frame_end = payload_end.saturating_add(BLOCK_TRAILER_LEN as u64);
        if frame_end > scan_limit {
            stopped_on_failure = true;
            break;
        }
        let Some(frame_tail) = view(source, file, header_end, frame_end)? else {
            return Ok(need(file, header_end, frame_end));
        };
        let payload = &frame_tail[..payload_len];
        let block_trailer = &frame_tail[payload_len..];
        let mut crc_input = Vec::with_capacity(BLOCK_HEADER_LEN + payload_len);
        crc_input.extend_from_slice(&block_header_bytes);
        crc_input.extend_from_slice(payload);
        let crc_matches = read_u32(block_trailer, 0) == crc32c(&crc_input);
        let block_seq = read_u32(block_trailer, 4);
        let sequence_matches = block_seq == expected_seq;
        let marker_matches = read_u64(block_trailer, 8) == BLOCK_COMMIT_MARKER;
        if !crc_matches || !sequence_matches || !marker_matches {
            stopped_on_failure = true;
            break;
        }
        blocks.push(ScannedBlock {
            offset,
            encoded_len: frame_end - offset,
            header: block_header,
            block_seq,
            payload: payload.to_vec(),
        });
        expected_seq = expected_seq.saturating_add(1);
        offset = frame_end;
    }

    let state = if let Some(trailer) = footer {
        if validate_footer(&blocks, offset, scan_limit, trailer) && !stopped_on_failure {
            SegmentState::Sealed(trailer)
        } else {
            SegmentState::Torn
        }
    } else if stopped_on_failure {
        SegmentState::Torn
    } else {
        SegmentState::Active
    };
    let committed_len = if matches!(state, SegmentState::Sealed(_)) {
        snapshot.committed_len
    } else {
        offset
    };
    Ok(QueryPoll::Ready(BcctScan {
        file,
        source: snapshot,
        header,
        blocks,
        committed_len,
        state,
    }))
}

fn validate_footer(
    blocks: &[ScannedBlock],
    offset: u64,
    scan_limit: u64,
    trailer: FooterTrailer,
) -> bool {
    if offset != scan_limit {
        return false;
    }
    let Some(index_block) = blocks.last() else {
        return false;
    };
    if index_block.header.kind != BlockKind::FooterIndex as u8
        || index_block.offset != trailer.index_offset
        || index_block.encoded_len != trailer.index_len
        || trailer.index_offset.saturating_add(trailer.index_len) != scan_limit
    {
        return false;
    }
    let Ok(BlockRows::FooterIndex(rows)) = index_block.decode_rows() else {
        return false;
    };
    if rows.len() + 1 != blocks.len()
        || !rows.iter().zip(blocks).all(|(row, block)| {
            row.kind == block.header.kind
                && row.offset == block.offset
                && row.row_count == block.header.row_count
                && row.first_ts_ns == block.header.first_ts_ns
                && row.last_ts_ns == block.header.last_ts_ns
        })
    {
        return false;
    }
    blocks
        .iter()
        .filter(|block| block.header.kind != BlockKind::FooterIndex as u8)
        .map(|block| u64::from(block.header.row_count))
        .sum::<u64>()
        == trailer.total_rows
}

fn view(
    source: &dyn ByteSource,
    file: FileId,
    start: u64,
    end: u64,
) -> Result<Option<crate::ByteView>, QueryError> {
    let range = ByteRange::new(file, start, end)?;
    Ok(source.view(&range))
}

fn need<T>(file: FileId, start: u64, end: u64) -> QueryPoll<T> {
    QueryPoll::NeedData {
        ranges: vec![ByteRange { file, start, end }],
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed-width checked slice"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed-width checked slice"),
    )
}
