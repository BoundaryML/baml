use bex_events::prof::storage::crc32c;

use crate::{
    Completeness, HARD_MAX_BYTES, LeftHeavyResponse, QueryError, RunListing, TimelineResponse,
};

pub const BQF_HEADER_LEN: usize = 40;
pub const BQF_COLUMN_DIRECTORY_LEN: usize = 24;
pub const BQF_CRC_LEN: usize = 4;
pub const BQF_VERSION: u16 = 1;

const MAGIC: &[u8; 4] = b"BQF1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum FrameKind {
    Timeline = 1,
    LeftHeavy = 2,
    Runs = 3,
    RunMeta = 4,
    Completeness = 5,
    Sandwich = 6,
    Search = 7,
    Diff = 8,
    ValueRefs = 9,
    ValueDag = 10,
    Query = 11,
}

impl FrameKind {
    fn from_raw(raw: u16) -> Result<Self, QueryError> {
        match raw {
            1 => Ok(Self::Timeline),
            2 => Ok(Self::LeftHeavy),
            3 => Ok(Self::Runs),
            4 => Ok(Self::RunMeta),
            5 => Ok(Self::Completeness),
            6 => Ok(Self::Sandwich),
            7 => Ok(Self::Search),
            8 => Ok(Self::Diff),
            9 => Ok(Self::ValueRefs),
            10 => Ok(Self::ValueDag),
            11 => Ok(Self::Query),
            _ => Err(QueryError::invalid_data(format!(
                "unknown BQF1 frame kind {raw}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameFlags(u32);

impl FrameFlags {
    pub const LOD_DEGRADED: Self = Self(1 << 0);
    pub const PARTIAL_TAIL: Self = Self(1 << 1);
    pub const MORE_LANES: Self = Self(1 << 2);
    pub const TRUNCATED: Self = Self(1 << 3);
    pub const CAPTURE_LOSS: Self = Self(1 << 4);
    pub const COMPLETE: Self = Self(1 << 5);

    #[must_use]
    pub fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub(crate) fn from_meta(meta: &Completeness) -> Self {
        let mut flags = Self::default();
        if meta.lod_degraded {
            flags.insert(Self::LOD_DEGRADED);
        }
        if meta.partial_tail {
            flags.insert(Self::PARTIAL_TAIL);
        }
        if meta.more_lanes {
            flags.insert(Self::MORE_LANES);
        }
        if meta.truncated {
            flags.insert(Self::TRUNCATED);
        }
        if !meta.capture_loss.is_empty() {
            flags.insert(Self::CAPTURE_LOSS);
        }
        if meta.complete {
            flags.insert(Self::COMPLETE);
        }
        flags
    }
}

impl std::ops::BitOr for FrameFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ColumnType {
    U8 = 1,
    U16 = 2,
    U32 = 3,
    U64 = 4,
    I64 = 5,
    Utf8 = 6,
}

impl ColumnType {
    fn from_raw(raw: u8) -> Result<Self, QueryError> {
        match raw {
            1 => Ok(Self::U8),
            2 => Ok(Self::U16),
            3 => Ok(Self::U32),
            4 => Ok(Self::U64),
            5 => Ok(Self::I64),
            6 => Ok(Self::Utf8),
            _ => Err(QueryError::invalid_data(format!(
                "unknown BQF1 column type {raw}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Column {
    U8 { id: u16, values: Vec<u8> },
    U16 { id: u16, values: Vec<u16> },
    U32 { id: u16, values: Vec<u32> },
    U64 { id: u16, values: Vec<u64> },
    I64 { id: u16, values: Vec<i64> },
    Utf8 { id: u16, values: Vec<String> },
}

impl Column {
    fn id(&self) -> u16 {
        match self {
            Self::U8 { id, .. }
            | Self::U16 { id, .. }
            | Self::U32 { id, .. }
            | Self::U64 { id, .. }
            | Self::I64 { id, .. }
            | Self::Utf8 { id, .. } => *id,
        }
    }

    fn kind(&self) -> ColumnType {
        match self {
            Self::U8 { .. } => ColumnType::U8,
            Self::U16 { .. } => ColumnType::U16,
            Self::U32 { .. } => ColumnType::U32,
            Self::U64 { .. } => ColumnType::U64,
            Self::I64 { .. } => ColumnType::I64,
            Self::Utf8 { .. } => ColumnType::Utf8,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::U8 { values, .. } => values.len(),
            Self::U16 { values, .. } => values.len(),
            Self::U32 { values, .. } => values.len(),
            Self::U64 { values, .. } => values.len(),
            Self::I64 { values, .. } => values.len(),
            Self::Utf8 { values, .. } => values.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    pub kind: FrameKind,
    pub flags: FrameFlags,
    pub request_id: u64,
    pub data_epoch: u64,
    pub ncols: u16,
    pub nrows: u32,
    pub directory_len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnDirectory {
    pub id: u16,
    pub kind: ColumnType,
    pub data_offset: u32,
    pub data_len: u32,
    pub aux_offset: u32,
    pub aux_len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BqfFrame {
    bytes: Vec<u8>,
}

impl BqfFrame {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, QueryError> {
        let header = decode_header(bytes)?;
        let expected_directory_len = usize::from(header.ncols)
            .checked_mul(BQF_COLUMN_DIRECTORY_LEN)
            .ok_or_else(|| QueryError::invalid_data("BQF1 directory length overflow"))?;
        if header.directory_len as usize != expected_directory_len {
            return Err(QueryError::invalid_data(
                "BQF1 directory length does not match ncols",
            ));
        }
        let minimum = BQF_HEADER_LEN
            .checked_add(expected_directory_len)
            .and_then(|value| value.checked_add(BQF_CRC_LEN))
            .ok_or_else(|| QueryError::invalid_data("BQF1 frame length overflow"))?;
        if bytes.len() < minimum {
            return Err(QueryError::invalid_data("truncated BQF1 frame"));
        }
        let crc_offset = bytes.len() - BQF_CRC_LEN;
        if read_u32(bytes, crc_offset) != crc32c(&bytes[..crc_offset]) {
            return Err(QueryError::invalid_data("BQF1 CRC mismatch"));
        }
        for index in 0..usize::from(header.ncols) {
            let directory = decode_directory(bytes, index)?;
            validate_column_bounds(bytes, expected_directory_len, directory)?;
            let expected_data_len = match directory.kind {
                ColumnType::U8 => usize::try_from(header.nrows).unwrap_or(usize::MAX),
                ColumnType::U16 => usize::try_from(header.nrows)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(2),
                ColumnType::U32 => usize::try_from(header.nrows)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(4),
                ColumnType::U64 | ColumnType::I64 => usize::try_from(header.nrows)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(8),
                ColumnType::Utf8 => usize::try_from(header.nrows)
                    .unwrap_or(usize::MAX)
                    .saturating_add(1)
                    .saturating_mul(4),
            };
            if directory.data_len as usize != expected_data_len {
                return Err(QueryError::invalid_data(format!(
                    "BQF1 column {} has invalid fixed data length",
                    directory.id
                )));
            }
            if directory.kind == ColumnType::Utf8 {
                validate_utf8_column(bytes, expected_directory_len, directory, header.nrows)?;
            } else if directory.aux_len != 0 {
                return Err(QueryError::invalid_data(
                    "non-string BQF1 column has auxiliary data",
                ));
            }
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    pub fn header(&self) -> Result<FrameHeader, QueryError> {
        decode_header(&self.bytes)
    }

    pub fn directory(&self, index: usize) -> Result<ColumnDirectory, QueryError> {
        let header = self.header()?;
        if index >= usize::from(header.ncols) {
            return Err(QueryError::invalid_request(
                "BQF1 column index is out of bounds",
            ));
        }
        decode_directory(&self.bytes, index)
    }
}

#[derive(Clone, Debug)]
pub struct BqfBuilder {
    kind: FrameKind,
    flags: FrameFlags,
    request_id: u64,
    data_epoch: u64,
    nrows: u32,
    columns: Vec<Column>,
}

impl BqfBuilder {
    #[must_use]
    pub fn new(kind: FrameKind, request_id: u64, data_epoch: u64, nrows: u32) -> Self {
        Self {
            kind,
            flags: FrameFlags::default(),
            request_id,
            data_epoch,
            nrows,
            columns: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_flags(mut self, flags: FrameFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn push(&mut self, column: Column) -> Result<(), QueryError> {
        if column.len() != self.nrows as usize {
            return Err(QueryError::invalid_request(format!(
                "column {} has {} rows, expected {}",
                column.id(),
                column.len(),
                self.nrows
            )));
        }
        if self
            .columns
            .iter()
            .any(|existing| existing.id() == column.id())
        {
            return Err(QueryError::invalid_request(format!(
                "duplicate BQF1 column id {}",
                column.id()
            )));
        }
        self.columns.push(column);
        Ok(())
    }

    pub fn finish(self, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        if max_bytes > HARD_MAX_BYTES {
            return Err(QueryError::invalid_request(format!(
                "max_bytes must not exceed {HARD_MAX_BYTES}"
            )));
        }
        let ncols = u16::try_from(self.columns.len())
            .map_err(|_| QueryError::invalid_request("too many BQF1 columns"))?;
        let directory_len = self
            .columns
            .len()
            .checked_mul(BQF_COLUMN_DIRECTORY_LEN)
            .ok_or_else(|| QueryError::invalid_request("BQF1 directory length overflow"))?;
        let payload_base = BQF_HEADER_LEN
            .checked_add(directory_len)
            .ok_or_else(|| QueryError::invalid_request("BQF1 payload offset overflow"))?;
        let mut payload = Vec::new();
        let mut directories = Vec::with_capacity(self.columns.len());
        for column in &self.columns {
            align_8(&mut payload);
            let data_offset = payload_base
                .checked_add(payload.len())
                .ok_or_else(|| QueryError::invalid_request("BQF1 column offset overflow"))?;
            encode_column_data(column, &mut payload)?;
            let data_len = payload_base
                .checked_add(payload.len())
                .and_then(|end| end.checked_sub(data_offset))
                .ok_or_else(|| QueryError::invalid_request("BQF1 column length overflow"))?;
            let (aux_offset, aux_len) = if let Column::Utf8 { values, .. } = column {
                align_8(&mut payload);
                let aux_offset = payload_base
                    .checked_add(payload.len())
                    .ok_or_else(|| QueryError::invalid_request("BQF1 string offset overflow"))?;
                for value in values {
                    payload.extend_from_slice(value.as_bytes());
                }
                let aux_len = payload_base
                    .checked_add(payload.len())
                    .and_then(|end| end.checked_sub(aux_offset))
                    .ok_or_else(|| QueryError::invalid_request("BQF1 string length overflow"))?;
                (aux_offset, aux_len)
            } else {
                (0, 0)
            };
            directories.push(ColumnDirectory {
                id: column.id(),
                kind: column.kind(),
                data_offset: to_u32(data_offset, "BQF1 data offset")?,
                data_len: to_u32(data_len, "BQF1 data length")?,
                aux_offset: to_u32(aux_offset, "BQF1 auxiliary offset")?,
                aux_len: to_u32(aux_len, "BQF1 auxiliary length")?,
            });
        }
        let frame_len = payload_base
            .checked_add(payload.len())
            .and_then(|value| value.checked_add(BQF_CRC_LEN))
            .ok_or_else(|| QueryError::invalid_request("BQF1 frame length overflow"))?;
        if frame_len > max_bytes {
            return Err(QueryError::BudgetExceeded {
                required: frame_len,
                max_bytes,
            });
        }
        let mut bytes = vec![0_u8; payload_base];
        bytes[0..4].copy_from_slice(MAGIC);
        put_u16(&mut bytes, 4, BQF_VERSION);
        put_u16(&mut bytes, 6, self.kind as u16);
        put_u32(&mut bytes, 8, self.flags.bits());
        put_u16(&mut bytes, 12, ncols);
        put_u32(&mut bytes, 16, self.nrows);
        put_u64(&mut bytes, 20, self.request_id);
        put_u64(&mut bytes, 28, self.data_epoch);
        put_u32(
            &mut bytes,
            36,
            to_u32(directory_len, "BQF1 directory length")?,
        );
        for (index, directory) in directories.iter().enumerate() {
            let offset = BQF_HEADER_LEN + index * BQF_COLUMN_DIRECTORY_LEN;
            put_u16(&mut bytes, offset, directory.id);
            bytes[offset + 2] = directory.kind as u8;
            put_u32(&mut bytes, offset + 4, directory.data_offset);
            put_u32(&mut bytes, offset + 8, directory.data_len);
            put_u32(&mut bytes, offset + 12, directory.aux_offset);
            put_u32(&mut bytes, offset + 16, directory.aux_len);
        }
        bytes.extend_from_slice(&payload);
        let crc = crc32c(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        Ok(BqfFrame { bytes })
    }
}

impl TimelineResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let data_epoch = data_epoch(&self.meta);
        let row_count = self.bands.len().saturating_add(self.exact_rects.len());
        let nrows = u32::try_from(row_count)
            .map_err(|_| QueryError::invalid_request("too many timeline rows"))?;
        let lane_threads = self
            .lanes
            .iter()
            .map(|lane| (lane.lane, lane.logical_thread_id))
            .collect::<std::collections::HashMap<_, _>>();
        let mut builder = BqfBuilder::new(FrameKind::Timeline, request_id, data_epoch, nrows)
            .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::U16 {
            id: 1,
            values: self
                .bands
                .iter()
                .map(|band| band.lane)
                .chain(self.exact_rects.iter().map(|rect| rect.lane))
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 2,
            values: self
                .bands
                .iter()
                .map(|band| lane_threads.get(&band.lane).copied().unwrap_or(0))
                .chain(self.exact_rects.iter().map(|rect| rect.logical_thread_id))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 3,
            values: self
                .bands
                .iter()
                .map(|band| band.bucket)
                .chain(self.exact_rects.iter().map(|_| u32::MAX))
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 4,
            values: self
                .bands
                .iter()
                .map(|band| band.start_ns)
                .chain(self.exact_rects.iter().map(|rect| rect.start_ns))
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 5,
            values: self
                .bands
                .iter()
                .map(|band| band.end_ns)
                .chain(self.exact_rects.iter().map(|rect| rect.end_ns))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 6,
            values: self
                .bands
                .iter()
                .map(|band| band.busy_ppm)
                .chain(self.exact_rects.iter().map(|_| 0))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 7,
            values: self
                .bands
                .iter()
                .map(|band| band.awaiting_ppm)
                .chain(self.exact_rects.iter().map(|_| 0))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 8,
            values: self
                .bands
                .iter()
                .map(|band| band.dominant_function_id)
                .chain(self.exact_rects.iter().map(|rect| rect.function_id))
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 9,
            values: self
                .bands
                .iter()
                .map(|band| band.error_count)
                .chain(self.exact_rects.iter().map(|_| 0))
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 10,
            values: self
                .bands
                .iter()
                .map(|_| 0)
                .chain(self.exact_rects.iter().map(|rect| rect.tier as u8))
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 11,
            values: self
                .bands
                .iter()
                .map(|_| 0)
                .chain(self.exact_rects.iter().map(|rect| rect.call_id))
                .collect(),
        })?;
        builder.push(Column::U32 {
            id: 12,
            values: self
                .bands
                .iter()
                .map(|_| 0)
                .chain(self.exact_rects.iter().map(|rect| rect.node_id))
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 13,
            values: self
                .bands
                .iter()
                .map(|_| 0)
                .chain(self.exact_rects.iter().map(|rect| rect.status))
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 14,
            values: self
                .bands
                .iter()
                .map(|_| 0)
                .chain(self.exact_rects.iter().map(|rect| u8::from(rect.open)))
                .collect(),
        })?;
        builder.finish(max_bytes)
    }
}

impl LeftHeavyResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let data_epoch = data_epoch(&self.meta);
        let nrows = u32::try_from(self.nodes.len())
            .map_err(|_| QueryError::invalid_request("too many Left Heavy rows"))?;
        let mut builder = BqfBuilder::new(FrameKind::LeftHeavy, request_id, data_epoch, nrows)
            .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::U32 {
            id: 1,
            values: self.nodes.iter().map(|node| node.node_id).collect(),
        })?;
        builder.push(Column::U32 {
            id: 2,
            values: self.nodes.iter().map(|node| node.parent_row).collect(),
        })?;
        builder.push(Column::U32 {
            id: 3,
            values: self.nodes.iter().map(|node| node.function_id).collect(),
        })?;
        builder.push(Column::U16 {
            id: 4,
            values: self.nodes.iter().map(|node| node.depth).collect(),
        })?;
        builder.push(Column::U32 {
            id: 5,
            values: self.nodes.iter().map(|node| node.extent_ppm).collect(),
        })?;
        builder.push(Column::U64 {
            id: 6,
            values: self
                .nodes
                .iter()
                .map(|node| node.counters.total_ns)
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 7,
            values: self
                .nodes
                .iter()
                .map(|node| node.counters.self_ns)
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 8,
            values: self
                .nodes
                .iter()
                .map(|node| node.counters.await_ns)
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 9,
            values: self.nodes.iter().map(|node| node.counters.enters).collect(),
        })?;
        builder.push(Column::U64 {
            id: 10,
            values: self
                .nodes
                .iter()
                .map(|node| node.counters.errors())
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 11,
            values: self
                .nodes
                .iter()
                .map(|node| u8::from(node.synthetic_smaller))
                .collect(),
        })?;
        builder.finish(max_bytes)
    }
}

impl RunListing {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let nrows = u32::try_from(self.runs.len())
            .map_err(|_| QueryError::invalid_request("too many run rows"))?;
        let mut builder =
            BqfBuilder::new(FrameKind::Runs, request_id, data_epoch(&self.meta), nrows)
                .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::Utf8 {
            id: 1,
            values: self
                .runs
                .iter()
                .map(|run| run.boundary_id_wire.clone())
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 2,
            values: self.runs.iter().map(|run| run.created_ms).collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 3,
            values: self.runs.iter().map(|run| run.target.clone()).collect(),
        })?;
        builder.push(Column::U8 {
            id: 4,
            values: self.runs.iter().map(|run| run.state as u8).collect(),
        })?;
        builder.push(Column::U8 {
            id: 5,
            values: self
                .runs
                .iter()
                .map(|run| u8::from(run.has_snapshot))
                .collect(),
        })?;
        builder.push(Column::U8 {
            id: 6,
            values: self
                .runs
                .iter()
                .map(|run| u8::from(run.meta_torn_tail))
                .collect(),
        })?;
        builder.finish(max_bytes)
    }
}

fn encode_column_data(column: &Column, output: &mut Vec<u8>) -> Result<(), QueryError> {
    match column {
        Column::U8 { values, .. } => output.extend_from_slice(values),
        Column::U16 { values, .. } => {
            for value in values {
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        Column::U32 { values, .. } => {
            for value in values {
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        Column::U64 { values, .. } => {
            for value in values {
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        Column::I64 { values, .. } => {
            for value in values {
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        Column::Utf8 { values, .. } => {
            let mut offset = 0_u32;
            output.extend_from_slice(&offset.to_le_bytes());
            for value in values {
                let len = u32::try_from(value.len())
                    .map_err(|_| QueryError::invalid_request("BQF1 string exceeds u32"))?;
                offset = offset
                    .checked_add(len)
                    .ok_or_else(|| QueryError::invalid_request("BQF1 string offsets overflow"))?;
                output.extend_from_slice(&offset.to_le_bytes());
            }
        }
    }
    Ok(())
}

fn decode_header(bytes: &[u8]) -> Result<FrameHeader, QueryError> {
    if bytes.len() < BQF_HEADER_LEN + BQF_CRC_LEN {
        return Err(QueryError::invalid_data("truncated BQF1 header"));
    }
    if &bytes[..4] != MAGIC {
        return Err(QueryError::invalid_data("invalid BQF1 magic"));
    }
    if read_u16(bytes, 4) != BQF_VERSION {
        return Err(QueryError::invalid_data("unsupported BQF1 version"));
    }
    if read_u16(bytes, 14) != 0 {
        return Err(QueryError::invalid_data(
            "non-zero BQF1 header reserved bytes",
        ));
    }
    Ok(FrameHeader {
        kind: FrameKind::from_raw(read_u16(bytes, 6))?,
        flags: FrameFlags(read_u32(bytes, 8)),
        ncols: read_u16(bytes, 12),
        nrows: read_u32(bytes, 16),
        request_id: read_u64(bytes, 20),
        data_epoch: read_u64(bytes, 28),
        directory_len: read_u32(bytes, 36),
    })
}

fn decode_directory(bytes: &[u8], index: usize) -> Result<ColumnDirectory, QueryError> {
    let offset = BQF_HEADER_LEN
        .checked_add(index.saturating_mul(BQF_COLUMN_DIRECTORY_LEN))
        .ok_or_else(|| QueryError::invalid_data("BQF1 directory offset overflow"))?;
    let end = offset
        .checked_add(BQF_COLUMN_DIRECTORY_LEN)
        .ok_or_else(|| QueryError::invalid_data("BQF1 directory offset overflow"))?;
    if end > bytes.len() {
        return Err(QueryError::invalid_data("truncated BQF1 directory"));
    }
    if bytes[offset + 3] != 0 || read_u32(bytes, offset + 20) != 0 {
        return Err(QueryError::invalid_data(
            "non-zero BQF1 directory reserved bytes",
        ));
    }
    Ok(ColumnDirectory {
        id: read_u16(bytes, offset),
        kind: ColumnType::from_raw(bytes[offset + 2])?,
        data_offset: read_u32(bytes, offset + 4),
        data_len: read_u32(bytes, offset + 8),
        aux_offset: read_u32(bytes, offset + 12),
        aux_len: read_u32(bytes, offset + 16),
    })
}

fn validate_column_bounds(
    bytes: &[u8],
    directory_len: usize,
    directory: ColumnDirectory,
) -> Result<(), QueryError> {
    let payload_base = BQF_HEADER_LEN + directory_len;
    let crc_offset = bytes.len() - BQF_CRC_LEN;
    validate_region(
        payload_base,
        crc_offset,
        directory.data_offset,
        directory.data_len,
    )?;
    if directory.aux_len != 0 || directory.kind == ColumnType::Utf8 {
        validate_region(
            payload_base,
            crc_offset,
            directory.aux_offset,
            directory.aux_len,
        )?;
    } else if directory.aux_offset != 0 {
        return Err(QueryError::invalid_data(
            "empty BQF1 auxiliary region has non-zero offset",
        ));
    }
    Ok(())
}

fn validate_region(
    payload_base: usize,
    crc_offset: usize,
    offset: u32,
    len: u32,
) -> Result<(), QueryError> {
    let offset = offset as usize;
    let end = offset
        .checked_add(len as usize)
        .ok_or_else(|| QueryError::invalid_data("BQF1 column bounds overflow"))?;
    if offset < payload_base || end > crc_offset || !offset.is_multiple_of(8) {
        return Err(QueryError::invalid_data(
            "BQF1 column is out of bounds or misaligned",
        ));
    }
    Ok(())
}

fn validate_utf8_column(
    bytes: &[u8],
    _directory_len: usize,
    directory: ColumnDirectory,
    nrows: u32,
) -> Result<(), QueryError> {
    let data_start = directory.data_offset as usize;
    let aux_start = directory.aux_offset as usize;
    let aux_end = aux_start + directory.aux_len as usize;
    let mut previous = 0_u32;
    for index in 0..=nrows as usize {
        let offset = read_u32(bytes, data_start + index * 4);
        if index == 0 && offset != 0 || offset < previous || offset > directory.aux_len {
            return Err(QueryError::invalid_data("invalid BQF1 UTF-8 offsets"));
        }
        previous = offset;
    }
    if previous != directory.aux_len || std::str::from_utf8(&bytes[aux_start..aux_end]).is_err() {
        return Err(QueryError::invalid_data("invalid BQF1 UTF-8 payload"));
    }
    Ok(())
}

pub(crate) fn data_epoch(meta: &Completeness) -> u64 {
    meta.snapshot
        .iter()
        .fold(0_u64, |epoch, source| epoch.max(source.source.generation))
}

fn align_8(bytes: &mut Vec<u8>) {
    let padding = (8 - bytes.len() % 8) % 8;
    bytes.resize(bytes.len() + padding, 0);
}

fn to_u32(value: usize, field: &'static str) -> Result<u32, QueryError> {
    u32::try_from(value).map_err(|_| QueryError::invalid_request(format!("{field} exceeds u32")))
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("fixed-width checked slice"),
    )
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

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
