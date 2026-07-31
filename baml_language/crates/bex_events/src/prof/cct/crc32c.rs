//! CRC32C (Castagnoli) — the checksum of the BCCT container (§6.2) and the
//! other observability formats. In-tree software implementation (slice-by-
//! one table): the worst-case segment write rate is ~2 MB/s, three orders
//! below where a hardware CRC would matter, and this compiles identically
//! on wasm. NOT crc32fast: that is IEEE 802.3, a different polynomial.

const POLY: u32 = 0x82F6_3B78; // reflected CRC-32C (Castagnoli)

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = build_table();

/// One-shot CRC32C.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    extend(0, bytes)
}

/// Streaming form: `extend(extend(0, a), b) == crc32c(a ++ b)`.
#[must_use]
pub fn extend(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = !crc;
    for &byte in bytes {
        crc = (crc >> 8) ^ TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize];
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // RFC 3720 §B.4 test vectors.
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(crc32c(&[0u8; 32]), 0x8A91_36AA);
        assert_eq!(crc32c(&[0xFFu8; 32]), 0x62A8_AB43);
        assert_eq!(crc32c(b""), 0);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let split = extend(extend(0, &data[..10]), &data[10..]);
        assert_eq!(split, crc32c(data));
    }
}
