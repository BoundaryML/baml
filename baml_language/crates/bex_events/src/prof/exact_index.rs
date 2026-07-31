//! BIX1 sidecar for bounded exact-event artifacts.
//!
//! CCT files already carry block/footer indexes and must never get a BIX1
//! sidecar. This index accepts only exact-event byte offsets from a flight
//! dump or full trace and enforces the 25% default byte cap while building.

use std::collections::BTreeMap;

const MAGIC: &[u8; 4] = b"BIX1";
const VERSION: u16 = 1;
const HEADER_LEN: usize = 40;
const LANE_LEN: usize = 32;
const BUCKET_LEN: usize = 32;
pub const DEFAULT_INDEX_RATIO_NUMERATOR: usize = 1;
pub const DEFAULT_INDEX_RATIO_DENOMINATOR: usize = 4;
pub const MAX_BASE_BUCKETS: u32 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactEventPoint {
    pub lane: u64,
    pub timestamp_ns: u64,
    pub byte_offset: u64,
    pub byte_end: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexBudget {
    pub max_bytes: usize,
}

impl IndexBudget {
    #[must_use]
    pub fn for_segment_bytes(segment_bytes: usize) -> Self {
        Self {
            max_bytes: segment_bytes.saturating_mul(DEFAULT_INDEX_RATIO_NUMERATOR)
                / DEFAULT_INDEX_RATIO_DENOMINATOR,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactIndex {
    pub start_ns: u64,
    pub end_ns: u64,
    pub lanes: Vec<LaneIndex>,
    pub levels_shed: u16,
    pub encoded: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneIndex {
    pub lane: u64,
    pub events: u32,
    pub nominal_buckets: u32,
    pub buckets: Vec<IndexBucket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexBucket {
    pub ordinal: u32,
    pub events: u32,
    pub first_timestamp_ns: u64,
    pub first_byte_offset: u64,
    pub byte_end: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    Empty,
    InvalidPoint,
    BudgetTooSmall { minimum: usize, budget: usize },
    InvalidWire,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => out.write_str("cannot index an empty exact-event artifact"),
            Self::InvalidPoint => out.write_str("exact-event offsets/timestamps are not ordered"),
            Self::BudgetTooSmall { minimum, budget } => write!(
                out,
                "BIX1 budget {budget} bytes is below the {minimum}-byte minimum"
            ),
            Self::InvalidWire => out.write_str("invalid BIX1 sidecar"),
        }
    }
}

impl std::error::Error for IndexError {}

/// Builds successively coarser sparse buckets until the encoded sidecar fits.
/// Sparse emission is important: the specified `4 * events` nominal grid
/// gives good zoom math without serializing empty buckets.
pub fn build_exact_index(
    points: &[ExactEventPoint],
    budget: IndexBudget,
) -> Result<ExactIndex, IndexError> {
    if points.is_empty() {
        return Err(IndexError::Empty);
    }
    validate_points(points)?;
    let start_ns = points
        .iter()
        .map(|point| point.timestamp_ns)
        .min()
        .unwrap_or(0);
    let end_ns = points
        .iter()
        .map(|point| point.timestamp_ns)
        .max()
        .unwrap_or(start_ns);
    let mut by_lane = BTreeMap::<u64, Vec<ExactEventPoint>>::new();
    for point in points {
        by_lane.entry(point.lane).or_default().push(*point);
    }
    for lane in by_lane.values_mut() {
        lane.sort_unstable_by_key(|point| (point.timestamp_ns, point.byte_offset));
    }

    let minimum = HEADER_LEN.saturating_add(by_lane.len().saturating_mul(LANE_LEN));
    if budget.max_bytes < minimum {
        return Err(IndexError::BudgetTooSmall {
            minimum,
            budget: budget.max_bytes,
        });
    }

    for levels_shed in 0..=16_u16 {
        let lanes = by_lane
            .iter()
            .map(|(&lane, points)| build_lane(lane, points, start_ns, end_ns, levels_shed))
            .collect::<Vec<_>>();
        let encoded = encode_index(start_ns, end_ns, levels_shed, &lanes);
        if encoded.len() <= budget.max_bytes {
            return Ok(ExactIndex {
                start_ns,
                end_ns,
                lanes,
                levels_shed,
                encoded,
            });
        }
    }

    // The most compact useful index is one direct byte slab per lane.
    let lanes = by_lane
        .iter()
        .map(|(&lane, points)| one_bucket_lane(lane, points))
        .collect::<Vec<_>>();
    let encoded = encode_index(start_ns, end_ns, 17, &lanes);
    if encoded.len() > budget.max_bytes {
        return Err(IndexError::BudgetTooSmall {
            minimum: encoded.len(),
            budget: budget.max_bytes,
        });
    }
    Ok(ExactIndex {
        start_ns,
        end_ns,
        lanes,
        levels_shed: 17,
        encoded,
    })
}

pub fn decode_exact_index(bytes: &[u8]) -> Result<ExactIndex, IndexError> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC || get_u16(bytes, 4) != VERSION {
        return Err(IndexError::InvalidWire);
    }
    let stored_crc = get_u32(bytes, 36);
    let mut crc_input = bytes.to_vec();
    crc_input[36..40].fill(0);
    if crc32c(&crc_input) != stored_crc {
        return Err(IndexError::InvalidWire);
    }
    let start_ns = get_u64(bytes, 8);
    let end_ns = get_u64(bytes, 16);
    let lane_count = usize::try_from(get_u32(bytes, 24)).map_err(|_| IndexError::InvalidWire)?;
    let levels_shed = get_u16(bytes, 28);
    let mut cursor = HEADER_LEN;
    let directory_end = cursor
        .checked_add(
            lane_count
                .checked_mul(LANE_LEN)
                .ok_or(IndexError::InvalidWire)?,
        )
        .ok_or(IndexError::InvalidWire)?;
    if directory_end > bytes.len() {
        return Err(IndexError::InvalidWire);
    }
    let mut lanes = Vec::with_capacity(lane_count);
    for _ in 0..lane_count {
        let entry = &bytes[cursor..cursor + LANE_LEN];
        cursor += LANE_LEN;
        let lane = get_u64(entry, 0);
        let events = get_u32(entry, 8);
        let nominal_buckets = get_u32(entry, 12);
        let bucket_count =
            usize::try_from(get_u32(entry, 16)).map_err(|_| IndexError::InvalidWire)?;
        let bucket_offset =
            usize::try_from(get_u64(entry, 24)).map_err(|_| IndexError::InvalidWire)?;
        let bucket_end = bucket_offset
            .checked_add(
                bucket_count
                    .checked_mul(BUCKET_LEN)
                    .ok_or(IndexError::InvalidWire)?,
            )
            .ok_or(IndexError::InvalidWire)?;
        if bucket_offset < directory_end || bucket_end > bytes.len() {
            return Err(IndexError::InvalidWire);
        }
        let mut buckets = Vec::with_capacity(bucket_count);
        for chunk in bytes[bucket_offset..bucket_end].chunks_exact(BUCKET_LEN) {
            buckets.push(IndexBucket {
                ordinal: get_u32(chunk, 0),
                events: get_u32(chunk, 4),
                first_timestamp_ns: get_u64(chunk, 8),
                first_byte_offset: get_u64(chunk, 16),
                byte_end: get_u64(chunk, 24),
            });
        }
        lanes.push(LaneIndex {
            lane,
            events,
            nominal_buckets,
            buckets,
        });
    }
    Ok(ExactIndex {
        start_ns,
        end_ns,
        lanes,
        levels_shed,
        encoded: bytes.to_vec(),
    })
}

fn validate_points(points: &[ExactEventPoint]) -> Result<(), IndexError> {
    for point in points {
        if point.byte_end < point.byte_offset {
            return Err(IndexError::InvalidPoint);
        }
    }
    Ok(())
}

fn build_lane(
    lane: u64,
    points: &[ExactEventPoint],
    global_start: u64,
    global_end: u64,
    levels_shed: u16,
) -> LaneIndex {
    let events = u32::try_from(points.len()).unwrap_or(u32::MAX);
    let base = events.saturating_mul(4).min(MAX_BASE_BUCKETS).max(1);
    let nominal_buckets = (base >> levels_shed.min(31)).max(1);
    let span = global_end.saturating_sub(global_start).saturating_add(1);
    let mut sparse = BTreeMap::<u32, IndexBucket>::new();
    for point in points {
        let relative = point.timestamp_ns.saturating_sub(global_start);
        let ordinal_u64 = u64::from(nominal_buckets)
            .saturating_mul(relative)
            .checked_div(span)
            .unwrap_or(0)
            .min(u64::from(nominal_buckets.saturating_sub(1)));
        let ordinal = u32::try_from(ordinal_u64).unwrap_or(nominal_buckets.saturating_sub(1));
        sparse
            .entry(ordinal)
            .and_modify(|bucket| {
                bucket.events = bucket.events.saturating_add(1);
                bucket.first_timestamp_ns = bucket.first_timestamp_ns.min(point.timestamp_ns);
                bucket.first_byte_offset = bucket.first_byte_offset.min(point.byte_offset);
                bucket.byte_end = bucket.byte_end.max(point.byte_end);
            })
            .or_insert(IndexBucket {
                ordinal,
                events: 1,
                first_timestamp_ns: point.timestamp_ns,
                first_byte_offset: point.byte_offset,
                byte_end: point.byte_end,
            });
    }
    LaneIndex {
        lane,
        events,
        nominal_buckets,
        buckets: sparse.into_values().collect(),
    }
}

fn one_bucket_lane(lane: u64, points: &[ExactEventPoint]) -> LaneIndex {
    let first = points.first().expect("non-empty lane");
    LaneIndex {
        lane,
        events: u32::try_from(points.len()).unwrap_or(u32::MAX),
        nominal_buckets: 1,
        buckets: vec![IndexBucket {
            ordinal: 0,
            events: u32::try_from(points.len()).unwrap_or(u32::MAX),
            first_timestamp_ns: points
                .iter()
                .map(|point| point.timestamp_ns)
                .min()
                .unwrap_or(first.timestamp_ns),
            first_byte_offset: points
                .iter()
                .map(|point| point.byte_offset)
                .min()
                .unwrap_or(first.byte_offset),
            byte_end: points
                .iter()
                .map(|point| point.byte_end)
                .max()
                .unwrap_or(first.byte_end),
        }],
    }
}

fn encode_index(start_ns: u64, end_ns: u64, levels_shed: u16, lanes: &[LaneIndex]) -> Vec<u8> {
    let bucket_count = lanes.iter().map(|lane| lane.buckets.len()).sum::<usize>();
    let mut out = vec![
        0_u8;
        HEADER_LEN
            .saturating_add(lanes.len().saturating_mul(LANE_LEN))
            .saturating_add(bucket_count.saturating_mul(BUCKET_LEN))
    ];
    out[..4].copy_from_slice(MAGIC);
    put_u16(&mut out, 4, VERSION);
    put_u16(&mut out, 6, u16::try_from(HEADER_LEN).unwrap_or(u16::MAX));
    put_u64(&mut out, 8, start_ns);
    put_u64(&mut out, 16, end_ns);
    put_u32(&mut out, 24, u32::try_from(lanes.len()).unwrap_or(u32::MAX));
    put_u16(&mut out, 28, levels_shed);
    let mut bucket_offset = HEADER_LEN + lanes.len() * LANE_LEN;
    for (index, lane) in lanes.iter().enumerate() {
        let offset = HEADER_LEN + index * LANE_LEN;
        put_u64(&mut out, offset, lane.lane);
        put_u32(&mut out, offset + 8, lane.events);
        put_u32(&mut out, offset + 12, lane.nominal_buckets);
        put_u32(
            &mut out,
            offset + 16,
            u32::try_from(lane.buckets.len()).unwrap_or(u32::MAX),
        );
        put_u64(
            &mut out,
            offset + 24,
            u64::try_from(bucket_offset).unwrap_or(u64::MAX),
        );
        for bucket in &lane.buckets {
            put_u32(&mut out, bucket_offset, bucket.ordinal);
            put_u32(&mut out, bucket_offset + 4, bucket.events);
            put_u64(&mut out, bucket_offset + 8, bucket.first_timestamp_ns);
            put_u64(&mut out, bucket_offset + 16, bucket.first_byte_offset);
            put_u64(&mut out, bucket_offset + 24, bucket.byte_end);
            bucket_offset += BUCKET_LEN;
        }
    }
    let crc = crc32c(&out);
    put_u32(&mut out, 36, crc);
    out
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("checked BIX1"))
}
fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked BIX1"))
}
fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked BIX1"))
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

fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_index_round_trips_and_carries_direct_byte_offsets() {
        let points = (0..100_u64)
            .map(|index| ExactEventPoint {
                lane: index % 3,
                timestamp_ns: 1_000 + index * 7,
                byte_offset: index * 50,
                byte_end: index * 50 + 49,
            })
            .collect::<Vec<_>>();
        let index = build_exact_index(&points, IndexBudget { max_bytes: 8_192 }).unwrap();
        let decoded = decode_exact_index(&index.encoded).unwrap();
        assert_eq!(decoded, index);
        assert!(decoded.lanes.iter().all(|lane| {
            lane.buckets
                .iter()
                .all(|bucket| bucket.byte_end >= bucket.first_byte_offset)
        }));
    }

    #[test]
    fn builder_sheds_resolution_to_respect_quarter_segment_cap() {
        let points = (0..1_000_u64)
            .map(|index| ExactEventPoint {
                lane: index % 8,
                timestamp_ns: index,
                byte_offset: index * 100,
                byte_end: index * 100 + 99,
            })
            .collect::<Vec<_>>();
        let segment_bytes = 32_768;
        let budget = IndexBudget::for_segment_bytes(segment_bytes);
        let index = build_exact_index(&points, budget).unwrap();
        assert!(index.encoded.len() <= segment_bytes / 4);
        assert!(index.levels_shed > 0);
    }

    #[test]
    fn rejects_impossible_budget_instead_of_writing_oversize_index() {
        let points = [ExactEventPoint {
            lane: 1,
            timestamp_ns: 2,
            byte_offset: 3,
            byte_end: 4,
        }];
        assert!(matches!(
            build_exact_index(&points, IndexBudget { max_bytes: 1 }),
            Err(IndexError::BudgetTooSmall { .. })
        ));
    }

    #[test]
    fn crc_detects_torn_or_corrupt_sidecar() {
        let points = [ExactEventPoint {
            lane: 1,
            timestamp_ns: 2,
            byte_offset: 3,
            byte_end: 4,
        }];
        let mut bytes = build_exact_index(&points, IndexBudget { max_bytes: 512 })
            .unwrap()
            .encoded;
        bytes[10] ^= 1;
        assert_eq!(decode_exact_index(&bytes), Err(IndexError::InvalidWire));
    }
}
