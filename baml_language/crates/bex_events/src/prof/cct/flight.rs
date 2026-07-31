//! §5.9 flight recorder: a bounded ring of RAW drained bytes — one memcpy
//! on the drain path, zero transcode until a trigger fires.
//!
//! Chunks are whole drained ranges; eviction is whole-chunk FIFO with the
//! retained-window contract explicit (`evicted_bytes`/`evicted_chunks`).
//! Timestamps are derived lazily at dump time (the drain path stays at
//! one memcpy); dumps transcode through the existing `.bamlprof` framing
//! so every reader works unchanged.

use std::collections::VecDeque;

/// §5.9: 16 MiB native cap (≈200k call pairs ≈ 11 s of a working-agent
/// trace).
pub const FLIGHT_CAP_BYTES: usize = 16 << 20;
/// §3.1 rate limits: minimum spacing between dumps per boundary/engine.
pub const DUMP_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// §3.1: maximum dumps per boundary/engine.
pub const DUMP_MAX_PER_ENGINE: u32 = 16;

pub struct FlightChunk {
    pub engine_id: u64,
    pub bytes: Vec<u8>,
}

/// One per consumer (all engines share the window — a trigger dumps its
/// engine's slice).
pub struct FlightRecorder {
    chunks: VecDeque<FlightChunk>,
    bytes: usize,
    cap: usize,
    pub evicted_chunks: u64,
    pub evicted_bytes: u64,
}

impl FlightRecorder {
    #[must_use]
    pub fn new(cap: usize) -> FlightRecorder {
        FlightRecorder {
            chunks: VecDeque::new(),
            bytes: 0,
            cap,
            evicted_chunks: 0,
            evicted_bytes: 0,
        }
    }

    /// One memcpy: retain this drained range. Whole-chunk FIFO eviction.
    pub fn push(&mut self, engine_id: u64, bytes: &[u8]) {
        if bytes.is_empty() || bytes.len() > self.cap {
            return;
        }
        self.bytes += bytes.len();
        self.chunks.push_back(FlightChunk {
            engine_id,
            bytes: bytes.to_vec(),
        });
        while self.bytes > self.cap {
            let Some(evicted) = self.chunks.pop_front() else {
                break;
            };
            self.bytes -= evicted.bytes.len();
            self.evicted_chunks += 1;
            self.evicted_bytes += evicted.bytes.len() as u64;
        }
    }

    /// The retained window for one engine, oldest → newest.
    pub fn retained(&self, engine_id: u64) -> impl Iterator<Item = &FlightChunk> {
        self.chunks.iter().filter(move |c| c.engine_id == engine_id)
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    /// Drop one engine's chunks (engine closed and dumped/abandoned).
    pub fn forget(&mut self, engine_id: u64) {
        let before = self.bytes;
        self.chunks.retain(|c| {
            if c.engine_id == engine_id {
                false
            } else {
                true
            }
        });
        self.bytes = self.chunks.iter().map(|c| c.bytes.len()).sum();
        debug_assert!(self.bytes <= before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_eviction_is_whole_chunk_and_counted() {
        let mut ring = FlightRecorder::new(250);
        ring.push(1, &[0xAA; 100]);
        ring.push(1, &[0xBB; 100]);
        ring.push(2, &[0xCC; 100]);
        // 300 > 250: oldest chunk evicted whole.
        assert_eq!(ring.evicted_chunks, 1);
        assert_eq!(ring.evicted_bytes, 100);
        assert_eq!(ring.retained_bytes(), 200);
        assert_eq!(ring.retained(1).count(), 1);
        assert_eq!(ring.retained(2).count(), 1);
        assert_eq!(ring.retained(1).next().unwrap().bytes[0], 0xBB);

        ring.forget(1);
        assert_eq!(ring.retained(1).count(), 0);
        assert_eq!(ring.retained_bytes(), 100);
    }

    #[test]
    fn oversized_ranges_are_refused_not_wedged() {
        let mut ring = FlightRecorder::new(50);
        ring.push(1, &[0u8; 100]);
        assert_eq!(ring.retained_bytes(), 0);
        ring.push(1, &[0u8; 40]);
        assert_eq!(ring.retained_bytes(), 40);
    }
}
