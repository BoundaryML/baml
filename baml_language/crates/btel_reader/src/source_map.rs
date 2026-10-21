//! Recorded PCs to source locations, using only the recording's own maps.
//!
//! A map belongs to one function's executed code. It never comes from the
//! current workspace: a line is what the recorded program had, even if the
//! file changed since. Every PC that cannot be placed says why.
use btel_recorder::proto;

/// A resolved location: a half-open byte range in the function's own file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

/// Why a PC has no resolved site, or that it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteState {
    Resolved,
    /// The recording has no PC for this site (an entry call from the host).
    NoCaller,
    /// The PC's function is referenced but its definition is not indexed.
    FunctionUnresolved,
    /// The producer could not look the function up.
    FunctionUnavailable,
    /// Two different definitions were recorded for the function.
    FunctionConflicted,
    /// The definition has no map: an older recording, or no compact code.
    NoSourceMap,
    InvalidSourceMap,
    /// The definition has no source file to point into.
    NoSourceFile,
    /// The VM saturated a PC that did not fit the wire field.
    SentinelPc,
    PcOutOfRange,
    /// The PC lies before the first mapped instruction.
    UnmappedPc,
    /// The mapped span belongs to another compiler file than the function's.
    ForeignFile,
    /// No PC was recorded for this site.
    NoPc,
    /// The frame's function has no telemetry identity to look a map up by.
    NoFunction,
}

impl SiteState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::NoCaller => "no_caller",
            Self::FunctionUnresolved => "function_unresolved",
            Self::FunctionUnavailable => "function_unavailable",
            Self::FunctionConflicted => "function_conflicted",
            Self::NoSourceMap => "no_source_map",
            Self::InvalidSourceMap => "invalid_source_map",
            Self::NoSourceFile => "no_source_file",
            Self::SentinelPc => "sentinel_pc",
            Self::PcOutOfRange => "pc_out_of_range",
            Self::UnmappedPc => "unmapped_pc",
            Self::ForeignFile => "foreign_file",
            Self::NoPc => "no_pc",
            Self::NoFunction => "no_function",
        }
    }
}

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

    /// The source range covering `pc`, or why there is none.
    pub fn resolve(&self, pc: u32) -> Result<Site, SiteState> {
        if pc == u32::MAX {
            return Err(SiteState::SentinelPc);
        }
        if pc >= self.code_bytes {
            return Err(SiteState::PcOutOfRange);
        }
        let index = self.entries.partition_point(|entry| entry.pc <= pc);
        let Some(entry) = index.checked_sub(1).map(|i| self.entries[i]) else {
            return Err(SiteState::UnmappedPc);
        };
        if self.own_file != Some(entry.file_id) {
            return Err(SiteState::ForeignFile);
        }
        Ok(Site {
            line: entry.line,
            start: entry.start,
            end: entry.end,
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

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let word = |at: usize| -> Option<u32> {
            Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
        };
        let code_bytes = word(0)?;
        let file = word(4)?;
        let own_file = match *bytes.get(8)? {
            0 => None,
            1 => Some(file),
            _ => return None,
        };
        let body = bytes.get(9..)?;
        if !body.len().is_multiple_of(20) {
            return None;
        }
        let entries = (0..body.len() / 20)
            .map(|i| {
                let at = 9 + i * 20;
                Some(Entry {
                    pc: word(at)?,
                    file_id: word(at + 4)?,
                    start: word(at + 8)?,
                    end: word(at + 12)?,
                    line: word(at + 16)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            code_bytes,
            own_file,
            entries,
        })
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
    fn resolves_byte_offsets_and_explains_every_miss() {
        let map = SourceMap::from_wire(&wire(vec![0, 7, 12], vec![], 20), Some(3)).unwrap();
        assert_eq!(map.resolve(0).unwrap().line, 1);
        assert_eq!(
            map.resolve(8),
            Ok(Site {
                line: 2,
                start: 10,
                end: 15
            })
        );
        assert_eq!(map.resolve(19).unwrap().line, 3);
        assert_eq!(map.resolve(20), Err(SiteState::PcOutOfRange));
        assert_eq!(map.resolve(u32::MAX), Err(SiteState::SentinelPc));
        let late = SourceMap::from_wire(&wire(vec![4], vec![], 9), Some(3)).unwrap();
        assert_eq!(late.resolve(2), Err(SiteState::UnmappedPc));
        let foreign = SourceMap::from_wire(&wire(vec![0, 4], vec![3, 9], 9), Some(3)).unwrap();
        assert_eq!(foreign.resolve(1).unwrap().line, 1);
        assert_eq!(foreign.resolve(5), Err(SiteState::ForeignFile));
        assert_eq!(SourceMap::decode(&map.encode()), Some(map));
        assert_eq!(SourceMap::decode(&foreign.encode()), Some(foreign));
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
        assert_eq!(SourceMap::decode(&[1, 2, 3]), None);
    }
}
