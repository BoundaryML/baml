use std::{collections::VecDeque, mem::size_of};

use btel_recorder::SealedFile;

struct Segment {
    sequence: u64,
    bytes: Box<[u8]>,
}

/// Unacknowledged metadata only. Eviction deliberately permits unresolved
/// references rather than retaining an unbounded history during an outage.
pub(crate) struct MetadataJournal {
    segments: VecDeque<Segment>,
    bytes: usize,
    limit: usize,
}

impl MetadataJournal {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            segments: VecDeque::new(),
            bytes: 0,
            limit,
        }
    }

    fn pop_front(&mut self) {
        if let Some(segment) = self.segments.pop_front() {
            self.bytes -= segment.bytes.len();
        }
    }

    pub(crate) fn acknowledge(&mut self, sequence: u64) {
        while self
            .segments
            .front()
            .is_some_and(|entry| entry.sequence <= sequence)
        {
            self.pop_front();
        }
    }

    fn retained_bytes(&self) -> usize {
        self.bytes + self.segments.capacity() * size_of::<Segment>()
    }

    fn retention_required(&self, fresh_len: usize) -> usize {
        let capacity = self
            .segments
            .capacity()
            .max(self.segments.len() + usize::from(fresh_len != 0));
        self.bytes
            .saturating_add(fresh_len)
            .saturating_add(capacity.saturating_mul(size_of::<Segment>()))
    }

    /// Every later file carries the retained prefix, so even an out-of-order
    /// success acknowledges all retained segments introduced through its sequence.
    /// Returns the number of segments whose replay guarantee was lost.
    pub(crate) fn prepare(&mut self, file: &mut SealedFile, body_limit: usize) -> u64 {
        let fresh_len = file.metadata_bytes().len();
        let mut evictions = 0_u64;
        while !self.segments.is_empty()
            && (self.bytes > body_limit.saturating_sub(file.bytes().len())
                || self.retention_required(fresh_len) > self.limit)
        {
            self.pop_front();
            evictions = evictions.saturating_add(1);
        }
        file.prepend_metadata(self.segments.iter().map(|entry| entry.bytes.as_ref()));
        evictions.saturating_add(self.retain(file))
    }

    /// Remember new metadata if it fits without evicting the retained prefix.
    /// This also permits metadata from a rejected payload to reach a later file.
    pub(crate) fn retain(&mut self, file: &SealedFile) -> u64 {
        let fresh_len = file.metadata_bytes().len();
        if fresh_len == 0 {
            return 0;
        }
        if self.retention_required(fresh_len) > self.limit {
            self.segments.shrink_to_fit();
        }
        if self.retention_required(fresh_len) > self.limit {
            return 1;
        }
        if self.segments.len() == self.segments.capacity() {
            let available = (self.limit - self.bytes - fresh_len) / size_of::<Segment>();
            let capacity = self
                .segments
                .capacity()
                .max(1)
                .saturating_mul(2)
                .min(available);
            self.segments.reserve_exact(capacity - self.segments.len());
        }
        // Charge actual queue capacity, not just live entries.
        if self.retained_bytes().saturating_add(fresh_len) > self.limit {
            self.segments.shrink_to_fit();
            return 1;
        }
        self.segments.push_back(Segment {
            sequence: file.sequence().get(),
            bytes: file.metadata_bytes().into(),
        });
        self.bytes += fresh_len;
        0
    }
}

#[cfg(test)]
mod tests {
    use btel_processor::AggregateDelta;
    use btel_recorder::{RecordingBuilder, RecordingConfig, RecordingId, proto};
    use btel_records::SpanRecord;
    use btel_types::{CallPathEdge, CallPathId, FunctionIdAllocator, allocate_telemetry_id};
    use prost::Message;

    use super::*;

    fn builder() -> RecordingBuilder {
        RecordingBuilder::new(RecordingId::generate(), RecordingConfig::default())
            .unwrap()
            .with_metadata_replay()
    }

    fn define(builder: &mut RecordingBuilder, path: u32) {
        builder.span_reference(
            allocate_telemetry_id(),
            &SpanRecord::CallPathDefined {
                call_path: CallPathId::new_non_root(path).unwrap(),
                parent_call_path: CallPathId::ROOT,
                visible_caller: None,
                caller_pc: 0,
                callee: FunctionIdAllocator::default().allocate().unwrap(),
                edge: CallPathEdge::Synchronous,
            },
        );
    }

    fn seal(builder: &mut RecordingBuilder) -> SealedFile {
        builder.aggregate(AggregateDelta {
            count: 1,
            ..AggregateDelta::default()
        });
        builder.finish_recording().unwrap().unwrap()
    }

    #[test]
    fn out_of_order_ack_releases_only_its_prefix_without_replaying_counts() {
        let mut builder = builder();
        let mut journal = MetadataJournal::new(4096);
        define(&mut builder, 1);
        let mut first = seal(&mut builder);
        assert_eq!(journal.prepare(&mut first, 4096), 0);
        define(&mut builder, 2);
        let mut second = seal(&mut builder);
        assert_eq!(journal.prepare(&mut second, 4096), 0);
        define(&mut builder, 3);
        let mut third = seal(&mut builder);
        assert_eq!(journal.prepare(&mut third, 4096), 0);
        journal.acknowledge(2);
        journal.acknowledge(1);
        let mut fourth = seal(&mut builder);
        assert_eq!(journal.prepare(&mut fourth, 4096), 0);
        let decoded = proto::RecordingFile::decode(fourth.bytes()).unwrap();
        assert_eq!(decoded.sequence, 4);
        let paths = decoded.definitions.unwrap().call_paths;
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].call_path_id, 3);
        assert_eq!(decoded.aggregates.unwrap().entries[0].count, 1);
        assert!(decoded.spans.unwrap().sections.is_empty());
        journal.acknowledge(4);
        assert_eq!(journal.bytes, 0);
        assert!(journal.segments.is_empty());
    }

    #[test]
    fn outage_retention_and_replay_body_are_bounded_including_capacity() {
        let mut builder = builder();
        let mut journal = MetadataJournal::new(256);
        let mut evictions = 0;
        for path in 1..100 {
            define(&mut builder, path);
            let mut file = seal(&mut builder);
            evictions += journal.prepare(&mut file, 256);
            assert!(file.bytes().len() <= 256);
            assert!(journal.retained_bytes() <= 256);
        }
        assert!(evictions > 0);
        journal.acknowledge(100);
        assert_eq!(journal.bytes, 0);
        assert!(journal.retained_bytes() <= 256);
    }

    #[test]
    fn indivisible_metadata_is_not_retained_forever() {
        let mut builder = builder();
        let mut journal = MetadataJournal::new(1);
        define(&mut builder, 1);
        let mut file = seal(&mut builder);
        assert_eq!(journal.prepare(&mut file, 1), 1);
        assert_eq!(journal.retained_bytes(), 0);
        assert!(journal.segments.is_empty());
    }

    #[test]
    fn rejected_payload_metadata_cannot_displace_a_full_retained_prefix() {
        let mut builder = builder();
        define(&mut builder, 1);
        let mut first = seal(&mut builder);
        let mut journal = MetadataJournal::new(first.metadata_bytes().len() + size_of::<Segment>());
        assert_eq!(journal.prepare(&mut first, 4096), 0);
        define(&mut builder, 2);
        let rejected = seal(&mut builder);
        assert_eq!(journal.retain(&rejected), 1);
        let mut later = seal(&mut builder);
        assert_eq!(journal.prepare(&mut later, 4096), 0);
        let paths = proto::RecordingFile::decode(later.bytes())
            .unwrap()
            .definitions
            .unwrap()
            .call_paths;
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].call_path_id, 1);
        assert!(journal.retained_bytes() <= journal.limit);
    }
}
