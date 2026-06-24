use crate::{
    prof::pb,
    run::{ProfileEventEnvelope, TraceCallKey, component_event_indices_for_root},
};

#[derive(Clone, Debug)]
pub struct HistoryProfileRecord {
    pub envelope: ProfileEventEnvelope,
    pub disk_event: pb::DiskEventV1,
}

#[derive(Debug)]
pub struct BoundaryTraceRouter {
    records: Vec<HistoryProfileRecord>,
    max_records: usize,
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
            dropped_records: 0,
        }
    }

    pub fn ingest(&mut self, envelope: ProfileEventEnvelope, disk_event: pb::DiskEventV1) {
        self.records.push(HistoryProfileRecord {
            envelope,
            disk_event,
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
    pub fn component_indices(&self, root_trace: TraceCallKey) -> Vec<usize> {
        let envelopes = self
            .records
            .iter()
            .map(|record| record.envelope.clone())
            .collect::<Vec<_>>();
        component_event_indices_for_root(&envelopes, root_trace)
    }

    #[must_use]
    pub fn record(&self, index: usize) -> Option<&HistoryProfileRecord> {
        self.records.get(index)
    }

    #[must_use]
    pub fn dropped_records(&self) -> u64 {
        self.dropped_records
    }
}
