use crate::{
    prof::pb,
    run::{ProfileEventEnvelope, TraceCallKey, component_event_indices_for_root},
};

pub type HistoryProfileRecordId = u64;

#[derive(Clone, Debug)]
pub struct HistoryProfileRecord {
    pub envelope: ProfileEventEnvelope,
    pub disk_event: pb::DiskEventV1,
}

#[derive(Clone, Debug)]
struct RoutedHistoryProfileRecord {
    id: HistoryProfileRecordId,
    record: HistoryProfileRecord,
}

#[derive(Debug)]
pub struct BoundaryTraceRouter {
    records: Vec<RoutedHistoryProfileRecord>,
    max_records: usize,
    next_record_id: HistoryProfileRecordId,
    dropped_records: u64,
}

impl Default for BoundaryTraceRouter {
    fn default() -> Self {
        Self::new(100_000)
    }
}

impl BoundaryTraceRouter {
    #[must_use]
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::new(),
            max_records,
            next_record_id: 0,
            dropped_records: 0,
        }
    }

    pub fn ingest(&mut self, envelope: ProfileEventEnvelope, disk_event: pb::DiskEventV1) {
        let id = self.next_record_id;
        self.next_record_id = self.next_record_id.saturating_add(1);
        self.records.push(RoutedHistoryProfileRecord {
            id,
            record: HistoryProfileRecord {
                envelope,
                disk_event,
            },
        });
        if self.records.len() > self.max_records {
            let drop_count = self.records.len() - self.max_records;
            self.records.drain(..drop_count);
            self.dropped_records = self
                .dropped_records
                .saturating_add(u64::try_from(drop_count).unwrap_or(u64::MAX));
        }
    }

    #[must_use]
    pub fn component_record_ids(&self, root_trace: TraceCallKey) -> Vec<HistoryProfileRecordId> {
        let envelopes = self.records.iter().map(|record| &record.record.envelope);
        component_event_indices_for_root(envelopes, root_trace)
            .into_iter()
            .filter_map(|index| self.records.get(index).map(|record| record.id))
            .collect()
    }

    /// Drop all buffered records for a closed engine — no further events can
    /// arrive for it and no boundary can claim them anymore.
    pub fn release_engine(&mut self, engine_id: crate::ids::EngineId) {
        self.records
            .retain(|record| record.record.envelope.engine_id != engine_id);
    }

    #[must_use]
    pub fn record(&self, id: HistoryProfileRecordId) -> Option<&HistoryProfileRecord> {
        self.records
            .iter()
            .find(|record| record.id == id)
            .map(|record| &record.record)
    }

    #[must_use]
    pub fn dropped_records(&self) -> u64 {
        self.dropped_records
    }
}
