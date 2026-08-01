use std::{
    fs::File,
    io::{self, Read, Write},
    sync::mpsc,
    thread,
};

use super::{
    format::{
        BCCT_HEADER_LEN, BCCT_TRAILER_LEN, BLOCK_COMMIT_MARKER, BLOCK_HEADER_LEN,
        BLOCK_TRAILER_LEN, BcctHeader, BlockHeader, BlockKind, FooterTrailer, crc32c_slices,
        get_u32, get_u64, invalid_data, put_u32, put_u64,
    },
    rows::{BlockRows, CctDeltaRow, FooterIndexRow, WatermarkRow},
};

const MAX_PAYLOAD_LEN: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppendOutcome {
    pub block_seq: u32,
    pub offset: u64,
    pub encoded_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawBlock {
    pub kind: u8,
    pub flags: u8,
    pub row_count: u32,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub payload: Vec<u8>,
    pub node_bounds: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockIndexEntry {
    pub kind: u8,
    pub offset: u64,
    pub row_count: u32,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    /// `u32::MAX` means the block has no node-id column.
    pub node_id_min: u32,
    /// `u32::MAX` means the block has no node-id column.
    pub node_id_max: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckpointCadence {
    delta_bytes_since_checkpoint: u64,
}

impl CheckpointCadence {
    #[must_use]
    pub fn delta_bytes_since_checkpoint(self) -> u64 {
        self.delta_bytes_since_checkpoint
    }

    pub fn record_delta(&mut self, encoded_bytes: u64) {
        self.delta_bytes_since_checkpoint = self
            .delta_bytes_since_checkpoint
            .saturating_add(encoded_bytes);
    }

    #[must_use]
    pub fn should_checkpoint(self, encoded_checkpoint_bytes: u64) -> bool {
        encoded_checkpoint_bytes != 0
            && self.delta_bytes_since_checkpoint >= encoded_checkpoint_bytes
    }

    pub fn checkpoint_written(&mut self) {
        self.delta_bytes_since_checkpoint = 0;
    }
}

pub struct BcctWriter<W> {
    sink: W,
    offset: u64,
    next_block_seq: u32,
    total_rows: u64,
    index: Vec<BlockIndexEntry>,
    sealed: bool,
}

impl<W: Write> BcctWriter<W> {
    pub fn create(mut sink: W, header: &BcctHeader) -> io::Result<Self> {
        sink.write_all(&header.encode())?;
        Ok(Self {
            sink,
            offset: BCCT_HEADER_LEN as u64,
            next_block_seq: 1,
            total_rows: 0,
            index: Vec::new(),
            sealed: false,
        })
    }

    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.offset
    }

    #[must_use]
    pub fn next_block_seq(&self) -> u32 {
        self.next_block_seq
    }

    #[must_use]
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }

    pub fn append(
        &mut self,
        rows: &BlockRows,
        first_ts_ns: u64,
        last_ts_ns: u64,
    ) -> io::Result<AppendOutcome> {
        if self.sealed {
            return Err(io::Error::other("cannot append to a sealed BCCT segment"));
        }
        if rows.kind_raw() == BlockKind::FooterIndex as u8 {
            return Err(io::Error::other(
                "footer-index blocks are written only by seal",
            ));
        }
        let payload = rows.encode_payload()?;
        self.append_encoded(RawBlock {
            kind: rows.kind_raw(),
            flags: 0,
            row_count: rows.row_count(),
            first_ts_ns,
            last_ts_ns,
            payload,
            node_bounds: node_bounds(rows),
        })
    }

    pub fn append_raw(&mut self, block: RawBlock) -> io::Result<AppendOutcome> {
        if self.sealed {
            return Err(io::Error::other("cannot append to a sealed BCCT segment"));
        }
        if block.kind == BlockKind::FooterIndex as u8 {
            return Err(io::Error::other(
                "footer-index blocks are written only by seal",
            ));
        }
        self.append_encoded(block)
    }

    pub fn append_cct_delta(
        &mut self,
        cadence: &mut CheckpointCadence,
        rows: Vec<CctDeltaRow>,
        first_ts_ns: u64,
        last_ts_ns: u64,
    ) -> io::Result<AppendOutcome> {
        let outcome = self.append(&BlockRows::CctDelta(rows), first_ts_ns, last_ts_ns)?;
        cadence.record_delta(outcome.encoded_len);
        Ok(outcome)
    }

    pub fn append_checkpoint_if_due(
        &mut self,
        cadence: &mut CheckpointCadence,
        totals: &[CctDeltaRow],
        first_ts_ns: u64,
        last_ts_ns: u64,
    ) -> io::Result<Option<AppendOutcome>> {
        if totals.is_empty() {
            return Ok(None);
        }
        let rows = BlockRows::NodeTotal(totals.to_vec());
        let payload = rows.encode_payload()?;
        let encoded_len = framed_len(payload.len())?;
        if !cadence.should_checkpoint(encoded_len) {
            return Ok(None);
        }
        let outcome = self.append_encoded(RawBlock {
            kind: BlockKind::NodeTotal as u8,
            flags: 0,
            row_count: rows.row_count(),
            first_ts_ns,
            last_ts_ns,
            payload,
            node_bounds: node_bounds(&rows),
        })?;
        cadence.checkpoint_written();
        Ok(Some(outcome))
    }

    pub fn append_watermark(&mut self, watermark: WatermarkRow) -> io::Result<AppendOutcome> {
        self.append(
            &BlockRows::Watermark(vec![watermark]),
            watermark.drained_through_ts_ns,
            watermark.drained_through_ts_ns,
        )
    }

    pub fn seal(&mut self) -> io::Result<FooterTrailer> {
        if self.sealed {
            return Err(io::Error::other("BCCT segment is already sealed"));
        }
        self.flush()?;
        let index_rows = self
            .index
            .iter()
            .map(|entry| FooterIndexRow {
                kind: entry.kind,
                offset: entry.offset,
                row_count: entry.row_count,
                first_ts_ns: entry.first_ts_ns,
                last_ts_ns: entry.last_ts_ns,
                node_id_min: entry.node_id_min,
                node_id_max: entry.node_id_max,
            })
            .collect::<Vec<_>>();
        let rows = BlockRows::FooterIndex(index_rows);
        let payload = rows.encode_payload()?;
        let index_offset = self.offset;
        let index_outcome = self.append_encoded(RawBlock {
            kind: BlockKind::FooterIndex as u8,
            flags: 0,
            row_count: rows.row_count(),
            first_ts_ns: self.index.first().map_or(0, |entry| entry.first_ts_ns),
            last_ts_ns: self.index.last().map_or(0, |entry| entry.last_ts_ns),
            payload,
            node_bounds: None,
        })?;
        let trailer = FooterTrailer {
            index_offset,
            index_len: index_outcome.encoded_len,
            total_rows: self.total_rows.saturating_sub(u64::from(rows.row_count())),
        };
        self.sink.write_all(&trailer.encode())?;
        self.offset = self.offset.saturating_add(BCCT_TRAILER_LEN as u64);
        self.sink.flush()?;
        self.sealed = true;
        Ok(trailer)
    }

    pub fn into_inner(self) -> W {
        self.sink
    }

    fn append_encoded(&mut self, mut block: RawBlock) -> io::Result<AppendOutcome> {
        let payload = &mut block.payload;
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(invalid_data("BCCT block payload exceeds limit"));
        }
        let padding = (8 - payload.len() % 8) % 8;
        payload.resize(payload.len() + padding, 0);
        let payload_len = u32::try_from(payload.len())
            .map_err(|_| invalid_data("BCCT block payload exceeds u32"))?;
        let header = BlockHeader {
            kind: block.kind,
            flags: block.flags,
            row_count: block.row_count,
            payload_len,
            first_ts_ns: block.first_ts_ns,
            last_ts_ns: block.last_ts_ns,
        }
        .encode();
        let crc = crc32c_parts(&header, payload);
        let mut trailer = [0_u8; BLOCK_TRAILER_LEN];
        put_u32(&mut trailer, 0, crc);
        put_u32(&mut trailer, 4, self.next_block_seq);
        put_u64(&mut trailer, 8, BLOCK_COMMIT_MARKER);

        let offset = self.offset;
        self.sink.write_all(&header)?;
        self.sink.write_all(payload)?;
        self.sink.write_all(&trailer)?;
        let encoded_len = framed_len(payload.len())?;
        let (node_id_min, node_id_max) = block.node_bounds.unwrap_or((u32::MAX, u32::MAX));
        self.index.push(BlockIndexEntry {
            kind: block.kind,
            offset,
            row_count: block.row_count,
            first_ts_ns: block.first_ts_ns,
            last_ts_ns: block.last_ts_ns,
            node_id_min,
            node_id_max,
        });
        let outcome = AppendOutcome {
            block_seq: self.next_block_seq,
            offset,
            encoded_len,
        };
        self.offset = self.offset.saturating_add(encoded_len);
        self.total_rows = self.total_rows.saturating_add(u64::from(block.row_count));
        self.next_block_seq = self
            .next_block_seq
            .checked_add(1)
            .ok_or_else(|| invalid_data("BCCT block sequence exhausted"))?;
        Ok(outcome)
    }
}

impl BcctWriter<File> {
    pub fn sync_data(&mut self) -> io::Result<()> {
        self.flush()?;
        self.sink.sync_data()
    }

    pub fn sync_all(&mut self) -> io::Result<()> {
        self.flush()?;
        self.sink.sync_all()
    }

    pub fn async_sync_worker(&self) -> io::Result<AsyncFileSync> {
        AsyncFileSync::new(&self.sink)
    }

    /// Writes the watermark, flushes it to the OS, then requests D1 on the
    /// helper thread. The completion ticket is the watermark block sequence.
    pub fn append_watermark_and_request_sync(
        &mut self,
        worker: &AsyncFileSync,
        watermark: WatermarkRow,
    ) -> io::Result<AppendOutcome> {
        let outcome = self.append_watermark(watermark)?;
        self.flush()?;
        worker.request(u64::from(outcome.block_seq))?;
        Ok(outcome)
    }

    /// Native seal protocol: D1 existing blocks, append footer/trailer, then
    /// D2-sync file contents. Directory anchoring is performed by the layout
    /// primitive after create/rename.
    pub fn seal_synced(&mut self) -> io::Result<FooterTrailer> {
        self.sync_data()?;
        let trailer = self.seal()?;
        self.sync_all()?;
        Ok(trailer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentState {
    Active,
    Torn,
    Sealed(FooterTrailer),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedBlock {
    pub offset: u64,
    pub encoded_len: u64,
    pub header: BlockHeader,
    pub block_seq: u32,
    pub payload: Vec<u8>,
}

impl ScannedBlock {
    #[must_use]
    pub fn known_kind(&self) -> Option<BlockKind> {
        BlockKind::from_raw(self.header.kind)
    }

    pub fn decode_rows(&self) -> io::Result<BlockRows> {
        if self.header.flags & 1 != 0 {
            return Err(invalid_data(
                "zstd-compressed BCCT blocks are not supported by this build",
            ));
        }
        BlockRows::decode_payload(self.header.kind, self.header.row_count, &self.payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanResult {
    pub header: BcctHeader,
    pub blocks: Vec<ScannedBlock>,
    pub committed_len: u64,
    pub state: SegmentState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedIndex {
    pub header: BcctHeader,
    pub trailer: FooterTrailer,
    pub rows: Vec<FooterIndexRow>,
}

/// Probes only the fixed file header, footer-index block, and 48-byte seal
/// trailer. Indexed data payloads are neither copied nor checksummed.
pub fn probe_sealed_index(bytes: &[u8]) -> io::Result<Option<SealedIndex>> {
    let header = BcctHeader::decode(bytes)?;
    let Some(trailer_offset) = bytes.len().checked_sub(BCCT_TRAILER_LEN) else {
        return Ok(None);
    };
    let Ok(trailer) = FooterTrailer::decode(&bytes[trailer_offset..]) else {
        return Ok(None);
    };
    let index_offset = usize::try_from(trailer.index_offset)
        .map_err(|_| invalid_data("BCCT footer index offset overflow"))?;
    let index_len = usize::try_from(trailer.index_len)
        .map_err(|_| invalid_data("BCCT footer index length overflow"))?;
    let index_end = index_offset
        .checked_add(index_len)
        .ok_or_else(|| invalid_data("BCCT footer index range overflow"))?;
    if index_offset < BCCT_HEADER_LEN || index_end != trailer_offset {
        return Err(invalid_data("invalid BCCT footer index range"));
    }
    let header_end = index_offset
        .checked_add(BLOCK_HEADER_LEN)
        .ok_or_else(|| invalid_data("BCCT footer index range overflow"))?;
    let block_header = BlockHeader::decode(
        bytes
            .get(index_offset..header_end)
            .ok_or_else(|| invalid_data("truncated BCCT footer index header"))?,
    )?;
    if block_header.kind != BlockKind::FooterIndex as u8 || block_header.flags != 0 {
        return Err(invalid_data("invalid BCCT footer index block kind"));
    }
    let payload_end = header_end
        .checked_add(block_header.payload_len as usize)
        .ok_or_else(|| invalid_data("BCCT footer index range overflow"))?;
    let block_end = payload_end
        .checked_add(BLOCK_TRAILER_LEN)
        .ok_or_else(|| invalid_data("BCCT footer index range overflow"))?;
    if block_end != index_end || !(block_header.payload_len as usize).is_multiple_of(8) {
        return Err(invalid_data("invalid BCCT footer index framed length"));
    }
    let header_bytes = &bytes[index_offset..header_end];
    let payload = &bytes[header_end..payload_end];
    let block_trailer = &bytes[payload_end..block_end];
    if get_u32(block_trailer, 0) != crc32c_parts(header_bytes, payload)
        || get_u64(block_trailer, 8) != BLOCK_COMMIT_MARKER
    {
        return Err(invalid_data("invalid BCCT footer index commit"));
    }
    let BlockRows::FooterIndex(rows) = BlockRows::decode_payload(
        BlockKind::FooterIndex as u8,
        block_header.row_count,
        payload,
    )?
    else {
        unreachable!("footer-index kind decodes to footer-index rows");
    };
    if get_u32(block_trailer, 4)
        != u32::try_from(rows.len())
            .ok()
            .and_then(|len| len.checked_add(1))
            .ok_or_else(|| invalid_data("BCCT footer index sequence overflow"))?
    {
        return Err(invalid_data("invalid BCCT footer index sequence"));
    }
    if rows
        .first()
        .is_some_and(|row| row.offset != BCCT_HEADER_LEN as u64)
        || rows.windows(2).any(|pair| pair[0].offset >= pair[1].offset)
        || rows
            .last()
            .is_some_and(|row| row.offset >= trailer.index_offset)
        || rows.iter().map(|row| u64::from(row.row_count)).sum::<u64>() != trailer.total_rows
    {
        return Err(invalid_data("invalid BCCT footer index contents"));
    }
    Ok(Some(SealedIndex {
        header,
        trailer,
        rows,
    }))
}

#[must_use = "recovery status must be inspected"]
pub fn scan_bcct_bytes(bytes: &[u8]) -> io::Result<ScanResult> {
    let header = BcctHeader::decode(bytes)?;
    let trailer = bytes
        .len()
        .checked_sub(BCCT_TRAILER_LEN)
        .and_then(|offset| FooterTrailer::decode(&bytes[offset..]).ok());
    let scan_limit = trailer.map_or(bytes.len(), |_| bytes.len() - BCCT_TRAILER_LEN);
    let mut offset = BCCT_HEADER_LEN;
    let mut expected_seq = 1_u32;
    let mut blocks = Vec::new();
    let mut stopped_on_failure = false;

    while offset < scan_limit {
        let Some(header_end) = offset.checked_add(BLOCK_HEADER_LEN) else {
            stopped_on_failure = true;
            break;
        };
        let Some(header_bytes) = bytes.get(offset..header_end) else {
            stopped_on_failure = true;
            break;
        };
        let Ok(block_header) = BlockHeader::decode(header_bytes) else {
            stopped_on_failure = true;
            break;
        };
        let payload_len = block_header.payload_len as usize;
        if payload_len > MAX_PAYLOAD_LEN || !payload_len.is_multiple_of(8) {
            stopped_on_failure = true;
            break;
        }
        let Some(payload_end) = header_end.checked_add(payload_len) else {
            stopped_on_failure = true;
            break;
        };
        let Some(trailer_end) = payload_end.checked_add(BLOCK_TRAILER_LEN) else {
            stopped_on_failure = true;
            break;
        };
        if trailer_end > scan_limit {
            stopped_on_failure = true;
            break;
        }
        let payload = &bytes[header_end..payload_end];
        let block_trailer = &bytes[payload_end..trailer_end];
        let crc_matches = get_u32(block_trailer, 0) == crc32c_parts(header_bytes, payload);
        let block_seq = get_u32(block_trailer, 4);
        let sequence_matches = block_seq == expected_seq;
        let marker_matches = get_u64(block_trailer, 8) == BLOCK_COMMIT_MARKER;
        if !crc_matches || !sequence_matches || !marker_matches {
            stopped_on_failure = true;
            break;
        }
        blocks.push(ScannedBlock {
            offset: offset as u64,
            encoded_len: (trailer_end - offset) as u64,
            header: block_header,
            block_seq,
            payload: payload.to_vec(),
        });
        expected_seq = expected_seq.saturating_add(1);
        offset = trailer_end;
    }

    if let Some(trailer) = trailer {
        let footer_valid = !stopped_on_failure
            && offset == scan_limit
            && blocks.last().is_some_and(|block| {
                block.header.kind == BlockKind::FooterIndex as u8
                    && block.offset == trailer.index_offset
                    && block.encoded_len == trailer.index_len
                    && trailer.index_offset.saturating_add(trailer.index_len) == scan_limit as u64
            })
            && blocks
                .last()
                .and_then(|block| block.decode_rows().ok())
                .is_some_and(|rows| match rows {
                    BlockRows::FooterIndex(rows) => {
                        rows.len() + 1 == blocks.len()
                            && rows.iter().zip(&blocks).all(|(row, block)| {
                                row.kind == block.header.kind
                                    && row.offset == block.offset
                                    && row.row_count == block.header.row_count
                                    && row.first_ts_ns == block.header.first_ts_ns
                                    && row.last_ts_ns == block.header.last_ts_ns
                            })
                    }
                    _ => false,
                })
            && blocks
                .iter()
                .filter(|block| block.header.kind != BlockKind::FooterIndex as u8)
                .map(|block| u64::from(block.header.row_count))
                .sum::<u64>()
                == trailer.total_rows;
        if footer_valid {
            return Ok(ScanResult {
                header,
                blocks,
                committed_len: bytes.len() as u64,
                state: SegmentState::Sealed(trailer),
            });
        }
        stopped_on_failure = true;
    }

    Ok(ScanResult {
        header,
        blocks,
        committed_len: offset as u64,
        state: if stopped_on_failure {
            SegmentState::Torn
        } else {
            SegmentState::Active
        },
    })
}

pub fn scan_bcct_reader(mut reader: impl Read) -> io::Result<ScanResult> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    scan_bcct_bytes(&bytes)
}

#[derive(Debug)]
pub struct FileSyncCompletion {
    pub ticket: u64,
    pub result: io::Result<()>,
}

enum SyncRequest {
    Sync(u64),
    Shutdown,
}

pub struct AsyncFileSync {
    request_tx: mpsc::Sender<SyncRequest>,
    completion_rx: mpsc::Receiver<FileSyncCompletion>,
    join: Option<thread::JoinHandle<()>>,
}

impl AsyncFileSync {
    pub fn new(file: &File) -> io::Result<Self> {
        let file = file.try_clone()?;
        let (request_tx, request_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("baml-bcct-fsync".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    match request {
                        SyncRequest::Sync(ticket) => {
                            let result = file.sync_data();
                            if completion_tx
                                .send(FileSyncCompletion { ticket, result })
                                .is_err()
                            {
                                break;
                            }
                        }
                        SyncRequest::Shutdown => break,
                    }
                }
            })?;
        Ok(Self {
            request_tx,
            completion_rx,
            join: Some(join),
        })
    }

    pub fn request(&self, ticket: u64) -> io::Result<()> {
        self.request_tx
            .send(SyncRequest::Sync(ticket))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "BCCT sync worker stopped"))
    }

    pub fn try_complete(&self) -> Option<FileSyncCompletion> {
        self.completion_rx.try_recv().ok()
    }

    pub fn wait_complete(&self) -> io::Result<FileSyncCompletion> {
        self.completion_rx
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "BCCT sync worker stopped"))
    }

    pub fn finish(mut self) -> io::Result<()> {
        let _ = self.request_tx.send(SyncRequest::Shutdown);
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| io::Error::other("BCCT sync worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for AsyncFileSync {
    fn drop(&mut self) {
        let _ = self.request_tx.send(SyncRequest::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn framed_len(payload_len: usize) -> io::Result<u64> {
    let len = BLOCK_HEADER_LEN
        .checked_add(payload_len)
        .and_then(|len| len.checked_add(BLOCK_TRAILER_LEN))
        .ok_or_else(|| invalid_data("BCCT framed length overflow"))?;
    u64::try_from(len).map_err(|_| invalid_data("BCCT framed length overflow"))
}

fn crc32c_parts(first: &[u8], second: &[u8]) -> u32 {
    crc32c_slices(&[first, second])
}

fn node_bounds(rows: &BlockRows) -> Option<(u32, u32)> {
    fn min_max(values: impl Iterator<Item = u32>) -> Option<(u32, u32)> {
        values.fold(None, |bounds, value| {
            Some(bounds.map_or((value, value), |(min, max)| {
                (min.min(value), max.max(value))
            }))
        })
    }
    match rows {
        BlockRows::CctDelta(rows) | BlockRows::NodeTotal(rows) => {
            min_max(rows.iter().map(|row| row.node_id))
        }
        BlockRows::NodeBirth(rows) => min_max(rows.iter().map(|row| row.node_id)),
        BlockRows::SpawnEdge(rows) => min_max(
            rows.iter()
                .flat_map(|row| [row.parent_node, row.child_root_node]),
        ),
        BlockRows::CctHistogram(rows) => min_max(rows.iter().map(|row| row.node_id)),
        BlockRows::LlmDelta(rows) => min_max(rows.iter().map(|row| row.node_id)),
        BlockRows::Marker(rows) => min_max(rows.iter().filter_map(|row| row.node_id)),
        _ => None,
    }
}
