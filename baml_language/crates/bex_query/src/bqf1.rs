//! §9.3 BQF1: fixed little-endian columnar frames.
//!
//! Not Arrow (schema machinery buys nothing for ~10 known kinds), not JSON
//! (the measured 2.21 GB). Layout:
//!
//! ```text
//! [40 B header]
//!   magic        4B  "BQF1"
//!   kind         u16          (FrameKind)
//!   flags        u16          (bit0 lod_degraded, bit1 partial_tail, bit2 more_lanes)
//!   request_id   u64
//!   epoch        u64          (data generation the frame was computed at)
//!   ncols        u32
//!   nrows        u32
//!   payload_len  u64          (directory + columns, excluding trailer)
//! [column directory]  ncols × 16 B: (col_type u8, pad [u8;7], offset u64 from payload start)
//! [column payloads]   each 8-byte aligned
//!   fixed cols:  nrows × elem_size
//!   str cols:    (nrows+1) × u32 offsets, pad8, then utf8 bytes
//! [8 B trailer]  crc32c(header + directory + payloads) as u64 LE
//! ```
//!
//! Decodes into zero-copy `TypedArray` views in ~150 lines of TS.

use bex_events::prof::cct::crc32c;

pub const BQF1_MAGIC: [u8; 4] = *b"BQF1";
pub const HEADER_LEN: usize = 40;

pub const FLAG_LOD_DEGRADED: u16 = 1 << 0;
pub const FLAG_PARTIAL_TAIL: u16 = 1 << 1;
pub const FLAG_MORE_LANES: u16 = 1 << 2;

/// Frame kinds — the ~10 known shapes of §9.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum FrameKind {
    /// One row per run: the §9.6 runs list.
    RunsList = 1,
    /// Run header + the function dictionary (id ↔ fqn) sent once.
    RunMeta = 2,
    /// Activity bands per (thread × window bucket) — §9.4 aggregate tier.
    Timeline = 3,
    /// Preorder Left-Heavy fold rows.
    LeftHeavy = 4,
    /// Per-function totals table.
    TopFunctions = 5,
    /// Error/status report for a request.
    Status = 6,
    /// Live totals tick (small; live-tail subscriptions).
    LiveTotals = 7,
    /// §9.4 exact-recency tier: recent-call rects.
    RecentCalls = 8,
    /// §8 BQL result: a generic typed table with free-form columns.
    ///
    /// Layout convention (documented here because the columns are not a
    /// fixed shape like the other kinds):
    /// - The LAST column is always a `Str` meta column. Its row 0 carries
    ///   one JSON object `{"columns":[{"name","type"},...],"rows":N,
    ///   "footer":{"sealed","torn","first_ts_ns","last_ts_ns",
    ///   "degraded":[...]}}` — the §8.4 completeness footer every BQL
    ///   result must ship. Rows 1.. of the meta column are empty strings.
    /// - The preceding columns are the data columns named by the meta
    ///   JSON. Frame row 0 is a sentinel row (numeric 0 / empty string);
    ///   data row `i` lives at frame row `i + 1`.
    /// - `nrows = data_rows + 1`, so an EMPTY result is a 1-row frame that
    ///   still carries its footer.
    BqlTable = 9,
}

/// Column element types (u8 in the directory).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColType {
    U32 = 1,
    U64 = 2,
    F64 = 3,
    /// (nrows+1) u32 offsets + utf8 bytes.
    Str = 4,
}

/// One column of data to encode.
pub enum Col<'a> {
    U32(&'a [u32]),
    U64(&'a [u64]),
    F64(&'a [f64]),
    Str(&'a [String]),
}

impl Col<'_> {
    fn col_type(&self) -> ColType {
        match self {
            Col::U32(_) => ColType::U32,
            Col::U64(_) => ColType::U64,
            Col::F64(_) => ColType::F64,
            Col::Str(_) => ColType::Str,
        }
    }

    fn nrows(&self) -> usize {
        match self {
            Col::U32(v) => v.len(),
            Col::U64(v) => v.len(),
            Col::F64(v) => v.len(),
            Col::Str(v) => v.len(),
        }
    }
}

fn pad8(out: &mut Vec<u8>) {
    while out.len() % 8 != 0 {
        out.push(0);
    }
}

/// Encode one frame. All columns must share `nrows` (asserted).
#[must_use]
pub fn encode_frame(
    kind: FrameKind,
    flags: u16,
    request_id: u64,
    epoch: u64,
    cols: &[Col<'_>],
) -> Vec<u8> {
    let nrows = cols.first().map_or(0, Col::nrows);
    for c in cols {
        assert_eq!(c.nrows(), nrows, "BQF1 columns must share nrows");
    }

    // Payloads, collecting per-column offsets relative to payload start
    // (payload start = after the directory).
    let mut payload = Vec::new();
    let mut offsets = Vec::with_capacity(cols.len());
    for col in cols {
        pad8(&mut payload);
        offsets.push(payload.len() as u64);
        match col {
            Col::U32(v) => {
                for x in *v {
                    payload.extend_from_slice(&x.to_le_bytes());
                }
            }
            Col::U64(v) => {
                for x in *v {
                    payload.extend_from_slice(&x.to_le_bytes());
                }
            }
            Col::F64(v) => {
                for x in *v {
                    payload.extend_from_slice(&x.to_le_bytes());
                }
            }
            Col::Str(v) => {
                let mut at: u32 = 0;
                payload.extend_from_slice(&at.to_le_bytes());
                let mut bytes = Vec::new();
                for s in *v {
                    at = at.saturating_add(u32::try_from(s.len()).unwrap_or(u32::MAX));
                    bytes.extend_from_slice(s.as_bytes());
                    payload.extend_from_slice(&at.to_le_bytes());
                }
                pad8(&mut payload);
                payload.extend_from_slice(&bytes);
            }
        }
    }
    pad8(&mut payload);

    let dir_len = cols.len() * 16;
    let payload_len = (dir_len + payload.len()) as u64;

    let mut out = Vec::with_capacity(HEADER_LEN + dir_len + payload.len() + 8);
    out.extend_from_slice(&BQF1_MAGIC);
    out.extend_from_slice(&(kind as u16).to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(&epoch.to_le_bytes());
    out.extend_from_slice(&u32::try_from(cols.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(nrows).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&payload_len.to_le_bytes());
    debug_assert_eq!(out.len(), HEADER_LEN);

    for (col, off) in cols.iter().zip(&offsets) {
        out.push(col.col_type() as u8);
        out.extend_from_slice(&[0u8; 7]);
        out.extend_from_slice(&off.to_le_bytes());
    }
    out.extend_from_slice(&payload);

    let crc = u64::from(crc32c::crc32c(&out));
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// A convenience Status frame: one row, `code` + `message` columns.
#[must_use]
pub fn status_frame(request_id: u64, code: u32, message: &str) -> Vec<u8> {
    encode_frame(
        FrameKind::Status,
        0,
        request_id,
        0,
        &[
            Col::U32(&[code]),
            Col::Str(std::slice::from_ref(&message.to_string())),
        ],
    )
}

/// Decoded frame view (host-side tests; the TS decoder is the real
/// consumer). Owns nothing — borrows the frame bytes.
#[derive(Debug)]
pub struct FrameView<'a> {
    pub kind: u16,
    pub flags: u16,
    pub request_id: u64,
    pub epoch: u64,
    pub nrows: u32,
    pub cols: Vec<(ColType, &'a [u8])>,
    payload: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub fn col_u32(&self, i: usize) -> Option<Vec<u32>> {
        let (ColType::U32, bytes) = self.cols.get(i)? else {
            return None;
        };
        Some(
            bytes
                .chunks_exact(4)
                .take(self.nrows as usize)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                .collect(),
        )
    }

    pub fn col_u64(&self, i: usize) -> Option<Vec<u64>> {
        let (ColType::U64, bytes) = self.cols.get(i)? else {
            return None;
        };
        Some(
            bytes
                .chunks_exact(8)
                .take(self.nrows as usize)
                .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                .collect(),
        )
    }

    pub fn col_str(&self, i: usize) -> Option<Vec<String>> {
        let (ColType::Str, bytes) = self.cols.get(i)? else {
            return None;
        };
        let n = self.nrows as usize;
        let mut offsets = Vec::with_capacity(n + 1);
        for c in bytes.chunks_exact(4).take(n + 1) {
            offsets.push(u32::from_le_bytes(c.try_into().unwrap()) as usize);
        }
        // The utf8 blob starts at the 8-aligned boundary after offsets.
        let mut blob_at = (n + 1) * 4;
        blob_at += (8 - blob_at % 8) % 8;
        let blob = &bytes[blob_at..];
        let mut out = Vec::with_capacity(n);
        for w in offsets.windows(2) {
            out.push(String::from_utf8_lossy(blob.get(w[0]..w[1])?).into_owned());
        }
        Some(out)
    }

    /// The payload region (for size accounting in tests).
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    BadMagic,
    BadCrc,
    BadDirectory,
}

/// Host-side decode (tests + native round-trips).
pub fn decode_frame(bytes: &[u8]) -> Result<FrameView<'_>, DecodeError> {
    if bytes.len() < HEADER_LEN + 8 {
        return Err(DecodeError::TooShort);
    }
    if bytes[0..4] != BQF1_MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let body = &bytes[..bytes.len() - 8];
    let crc = u64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap());
    if u64::from(crc32c::crc32c(body)) != crc {
        return Err(DecodeError::BadCrc);
    }
    let kind = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let request_id = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let epoch = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let ncols = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    let nrows = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
    let payload_len = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;

    let dir_end = HEADER_LEN + ncols * 16;
    if dir_end > body.len() || HEADER_LEN + payload_len > body.len() {
        return Err(DecodeError::BadDirectory);
    }
    let payload = &body[dir_end..HEADER_LEN + payload_len];
    let mut cols = Vec::with_capacity(ncols);
    for i in 0..ncols {
        let at = HEADER_LEN + i * 16;
        let col_type = match bytes[at] {
            1 => ColType::U32,
            2 => ColType::U64,
            3 => ColType::F64,
            4 => ColType::Str,
            _ => return Err(DecodeError::BadDirectory),
        };
        let off = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap()) as usize;
        if off > payload.len() {
            return Err(DecodeError::BadDirectory);
        }
        // Column extent = next column's offset (or payload end); views are
        // sliced generously — readers take nrows-bounded prefixes.
        let end = if i + 1 < ncols {
            let next_at = HEADER_LEN + (i + 1) * 16;
            u64::from_le_bytes(bytes[next_at + 8..next_at + 16].try_into().unwrap()) as usize
        } else {
            payload.len()
        };
        cols.push((
            col_type,
            payload.get(off..end).ok_or(DecodeError::BadDirectory)?,
        ));
    }
    Ok(FrameView {
        kind,
        flags,
        request_id,
        epoch,
        nrows,
        cols,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_column_types() {
        let strs = vec!["alpha".to_string(), String::new(), "γreek".to_string()];
        let frame = encode_frame(
            FrameKind::TopFunctions,
            FLAG_PARTIAL_TAIL,
            42,
            7,
            &[
                Col::U32(&[1, 2, 3]),
                Col::U64(&[10, 20, 30]),
                Col::F64(&[0.5, 1.5, 2.5]),
                Col::Str(&strs),
            ],
        );
        let view = decode_frame(&frame).unwrap();
        assert_eq!(view.kind, FrameKind::TopFunctions as u16);
        assert_eq!(view.flags, FLAG_PARTIAL_TAIL);
        assert_eq!(view.request_id, 42);
        assert_eq!(view.epoch, 7);
        assert_eq!(view.nrows, 3);
        assert_eq!(view.col_u32(0).unwrap(), vec![1, 2, 3]);
        assert_eq!(view.col_u64(1).unwrap(), vec![10, 20, 30]);
        assert_eq!(view.col_str(3).unwrap(), strs);
    }

    #[test]
    fn corrupt_frame_fails_crc() {
        let mut frame = encode_frame(FrameKind::Status, 0, 1, 0, &[Col::U32(&[9])]);
        let mid = frame.len() / 2;
        frame[mid] ^= 0xFF;
        assert_eq!(decode_frame(&frame).unwrap_err(), DecodeError::BadCrc);
    }
}
