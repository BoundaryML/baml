//! Lifecycle-only writer. Log evidence and values belong to the shared store.

use std::io;

use super::{
    RunCompletedRecord, RunStartedRecord, ValueArtifactRef, ValueArtifactSink, ValueFileRecord,
    encode::{encode_file_record, encode_header},
};
use crate::ids::BoundaryId;

#[derive(Debug)]
pub struct ValueWriter<S: ValueArtifactSink> {
    sink: S,
}

impl<S: ValueArtifactSink> ValueWriter<S> {
    pub fn new(mut sink: S, boundary_id: BoundaryId) -> io::Result<Self> {
        let mut header = Vec::new();
        encode_header(&mut header, boundary_id)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        sink.write_chunk(&header)?;
        Ok(Self { sink })
    }

    pub fn append_run_started(&mut self, record: &RunStartedRecord) -> io::Result<()> {
        self.append(&ValueFileRecord::RunStarted(record.clone()))
    }

    pub fn append_run_completed(&mut self, record: &RunCompletedRecord) -> io::Result<()> {
        self.append(&ValueFileRecord::RunCompleted(record.clone()))
    }

    fn append(&mut self, record: &ValueFileRecord) -> io::Result<()> {
        let mut encoded = Vec::new();
        encode_file_record(&mut encoded, record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.sink.write_chunk(&encoded)
    }

    pub fn flush(&mut self) -> io::Result<ValueArtifactRef> {
        self.sink.flush()
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        run::RunStatus,
        value::{ByteValueArtifactSink, read_bamlvalue_from_bytes},
    };

    #[test]
    fn writes_lifecycle_records_without_value_bodies() {
        let mut writer = ValueWriter::new(
            ByteValueArtifactSink::new(),
            BoundaryId::from_bytes([1; 16]),
        )
        .unwrap();
        let completed = RunCompletedRecord {
            status: RunStatus::Succeeded,
            completed_at_ms: 10,
            renderer_hint: None,
            result_value_ref: None,
            error: None,
            cancellation: None,
        };
        writer.append_run_completed(&completed).unwrap();
        writer.flush().unwrap();
        let contents = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        assert_eq!(
            contents.records,
            vec![ValueFileRecord::RunCompleted(completed)]
        );
    }
}
