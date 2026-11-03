//! Recorded PC-to-source maps, validated and encoded for the index.
//!
//! A map belongs to one function's executed code. It never comes from the
//! current workspace: a line is what the recorded program had, even if the
//! file changed since.
use btel_recorder::proto;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    pc: u32,
    file_id: u32,
    start: u32,
    end: u32,
    line: u32,
}

/// A validated map. `own_file` is the compiler file of the function's
/// definition span; entries in another file are not resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    code_bytes: u32,
    own_file: Option<u32>,
    entries: Vec<Entry>,
}

/// Why a recorded map was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidMap {
    Coordinate,
    Lengths,
    PcOrder,
    PcBeyondCode,
    Range,
}

impl SourceMap {
    /// Validate a wire map. `own_file` comes from the same definition's span.
    pub fn from_wire(map: &proto::SourceMap, own_file: Option<u32>) -> Result<Self, InvalidMap> {
        if map.coordinate != proto::PcCoordinate::CompactByteOffset as i32 {
            return Err(InvalidMap::Coordinate);
        }
        let n = map.pc.len();
        if map.start.len() != n
            || map.end.len() != n
            || map.line.len() != n
            || !(map.file_id.is_empty() || map.file_id.len() == n)
        {
            return Err(InvalidMap::Lengths);
        }
        if map.file_id.is_empty() && n > 0 && own_file.is_none() {
            // "Same file as the definition" needs a definition file.
            return Err(InvalidMap::Lengths);
        }
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let entry = Entry {
                pc: map.pc[i],
                file_id: map.file_id.get(i).copied().or(own_file).unwrap_or_default(),
                start: map.start[i],
                end: map.end[i],
                line: map.line[i],
            };
            if entry.start > entry.end {
                return Err(InvalidMap::Range);
            }
            if entry.pc >= map.code_bytes {
                return Err(InvalidMap::PcBeyondCode);
            }
            if entries
                .last()
                .is_some_and(|last: &Entry| last.pc >= entry.pc)
            {
                return Err(InvalidMap::PcOrder);
            }
            entries.push(entry);
        }
        Ok(Self {
            code_bytes: map.code_bytes,
            own_file,
            entries,
        })
    }

    /// Stable storage encoding for an index: little-endian u32 fields.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.entries.len() * 20);
        out.extend_from_slice(&self.code_bytes.to_le_bytes());
        out.extend_from_slice(&self.own_file.unwrap_or(u32::MAX).to_le_bytes());
        out.push(u8::from(self.own_file.is_some()));
        for entry in &self.entries {
            for value in [entry.pc, entry.file_id, entry.start, entry.end, entry.line] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(pc: Vec<u32>, file_id: Vec<u32>, code_bytes: u32) -> proto::SourceMap {
        let n = u32::try_from(pc.len()).unwrap();
        proto::SourceMap {
            coordinate: proto::PcCoordinate::CompactByteOffset as i32,
            code_bytes,
            start: (0..n).map(|i| i * 10).collect(),
            end: (0..n).map(|i| i * 10 + 5).collect(),
            line: (1..=n).collect(),
            pc,
            file_id,
        }
    }

    #[test]
    fn malformed_maps_are_rejected() {
        let mut bad = wire(vec![0, 7], vec![], 20);
        bad.coordinate = 0;
        assert_eq!(
            SourceMap::from_wire(&bad, Some(1)),
            Err(InvalidMap::Coordinate)
        );
        assert_eq!(
            SourceMap::from_wire(&wire(vec![7, 7], vec![], 20), Some(1)),
            Err(InvalidMap::PcOrder)
        );
        assert_eq!(
            SourceMap::from_wire(&wire(vec![0, 30], vec![], 20), Some(1)),
            Err(InvalidMap::PcBeyondCode)
        );
        let mut short = wire(vec![0, 7], vec![], 20);
        short.line.pop();
        assert_eq!(
            SourceMap::from_wire(&short, Some(1)),
            Err(InvalidMap::Lengths)
        );
        assert_eq!(
            SourceMap::from_wire(&wire(vec![0], vec![], 20), None),
            Err(InvalidMap::Lengths)
        );
    }
}
