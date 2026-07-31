//! §6.7 pack index (`.bamlpack.idx`, written at seal via tmp+rename):
//!
//! ```text
//! [header 16 B]  magic "BPKI", version u16 (1), reserved u16, entry_count u32, reserved u32
//! [fanout]       256 × u32 LE — entries with cid[0] < i end at fanout[i]
//! [entries]      sorted by cid: {cid [32], offset u64, logical_len u32, stored_len u32} = 48 B
//! [trailer 8 B]  crc32c(everything above) as u64 LE
//! ```
//!
//! Always rebuildable by scanning the pack; readers keep every idx resident
//! (fanout + binary search, newest-first).

use std::io;

use crate::prof::cct::crc32c;

use super::pack::ChunkMeta;

pub const IDX_MAGIC: [u8; 4] = *b"BPKI";
pub const IDX_VERSION: u16 = 1;
pub const IDX_HEADER_LEN: usize = 16;
pub const IDX_ENTRY_LEN: usize = 48;

#[must_use]
pub fn encode_index(chunks: &[ChunkMeta]) -> Vec<u8> {
    let mut sorted: Vec<&ChunkMeta> = chunks.iter().collect();
    sorted.sort_by_key(|c| c.cid);

    let mut out = Vec::with_capacity(IDX_HEADER_LEN + 1024 + sorted.len() * IDX_ENTRY_LEN + 8);
    out.extend_from_slice(&IDX_MAGIC);
    out.extend_from_slice(&IDX_VERSION.to_le_bytes());
    out.extend_from_slice(&[0u8; 2]);
    out.extend_from_slice(
        &u32::try_from(sorted.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&[0u8; 4]);

    // 256-way fanout: fanout[b] = number of entries with cid[0] <= b.
    let mut fanout = [0u32; 256];
    for c in &sorted {
        fanout[c.cid[0] as usize] += 1;
    }
    let mut acc = 0u32;
    for slot in &mut fanout {
        acc += *slot;
        *slot = acc;
    }
    for slot in &fanout {
        out.extend_from_slice(&slot.to_le_bytes());
    }
    for c in sorted {
        out.extend_from_slice(&c.cid);
        out.extend_from_slice(&c.offset.to_le_bytes());
        out.extend_from_slice(&c.logical_len.to_le_bytes());
        out.extend_from_slice(&c.stored_len.to_le_bytes());
    }
    let crc = u64::from(crc32c::crc32c(&out));
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// A resident, parsed index.
pub struct PackIndex {
    /// Sorted (cid, offset, logical_len, stored_len).
    entries: Vec<([u8; 32], u64, u32, u32)>,
    fanout: [u32; 256],
}

impl PackIndex {
    pub fn decode(bytes: &[u8]) -> io::Result<PackIndex> {
        let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
        if bytes.len() < IDX_HEADER_LEN + 1024 + 8 || bytes[0..4] != IDX_MAGIC {
            return Err(bad("not a BPKI index"));
        }
        if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != IDX_VERSION {
            return Err(bad("unsupported BPKI version"));
        }
        let body = &bytes[..bytes.len() - 8];
        let crc = u64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap());
        if u64::from(crc32c::crc32c(body)) != crc {
            return Err(bad("BPKI crc mismatch"));
        }
        let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let mut fanout = [0u32; 256];
        for (i, slot) in fanout.iter_mut().enumerate() {
            let at = IDX_HEADER_LEN + i * 4;
            *slot = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        }
        let entries_at = IDX_HEADER_LEN + 1024;
        if body.len() < entries_at + count * IDX_ENTRY_LEN {
            return Err(bad("BPKI truncated entries"));
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let at = entries_at + i * IDX_ENTRY_LEN;
            entries.push((
                bytes[at..at + 32].try_into().unwrap(),
                u64::from_le_bytes(bytes[at + 32..at + 40].try_into().unwrap()),
                u32::from_le_bytes(bytes[at + 40..at + 44].try_into().unwrap()),
                u32::from_le_bytes(bytes[at + 44..at + 48].try_into().unwrap()),
            ));
        }
        Ok(PackIndex { entries, fanout })
    }

    /// Look one CID up: `(offset, logical_len, stored_len)`.
    #[must_use]
    pub fn lookup(&self, cid: &[u8; 32]) -> Option<(u64, u32, u32)> {
        let hi = self.fanout[cid[0] as usize] as usize;
        let lo = if cid[0] == 0 {
            0
        } else {
            self.fanout[cid[0] as usize - 1] as usize
        };
        let slice = self.entries.get(lo..hi)?;
        let i = slice.binary_search_by(|e| e.0.cmp(cid)).ok()?;
        let e = &slice[i];
        Some((e.1, e.2, e.3))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate all entries (GC mark/sweep).
    pub fn iter(&self) -> impl Iterator<Item = &([u8; 32], u64, u32, u32)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trip_and_lookup() {
        let chunks: Vec<ChunkMeta> = (0..300u32)
            .map(|i| {
                let mut cid = [0u8; 32];
                cid[0] = (i % 251) as u8;
                cid[1..5].copy_from_slice(&i.to_le_bytes());
                ChunkMeta {
                    kind: 1,
                    storage: 0,
                    cid,
                    logical_len: i,
                    stored_len: i,
                    offset: u64::from(i) * 100,
                }
            })
            .collect();
        let bytes = encode_index(&chunks);
        let index = PackIndex::decode(&bytes).unwrap();
        assert_eq!(index.len(), 300);
        for c in &chunks {
            assert_eq!(
                index.lookup(&c.cid),
                Some((c.offset, c.logical_len, c.stored_len)),
            );
        }
        assert_eq!(index.lookup(&[0xFF; 32]), None);

        // Corruption fails decode.
        let mut torn = bytes.clone();
        let mid = torn.len() / 2;
        torn[mid] ^= 0xFF;
        assert!(PackIndex::decode(&torn).is_err());
    }
}
