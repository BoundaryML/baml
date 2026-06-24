//! Minimal `.bamlvalue` append writer.

use std::io;

use crate::{
    ids::BoundaryId,
    value::{
        RunCompletedRecord, RunStartedRecord, ValueArtifactRef, ValueArtifactSink, ValueCapture,
        ValueCodec, ValueRecord, ValueRef,
        encode::{encode_header, encode_record, encode_run_completed, encode_run_started},
    },
};

#[derive(Debug)]
pub struct ValueWriter<S> {
    sink: S,
    next_value_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueWriteOutcome {
    pub value_ref: ValueRef,
}

impl<S: ValueArtifactSink> ValueWriter<S> {
    pub fn new(mut sink: S, boundary_id: BoundaryId) -> io::Result<Self> {
        let mut header = Vec::new();
        encode_header(&mut header, boundary_id)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        sink.write_chunk(&header)?;
        Ok(Self {
            sink,
            next_value_id: 1,
        })
    }

    pub fn append_body(
        &mut self,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.append_body_with_capture(codec, body, None)
    }

    pub fn append_body_with_capture(
        &mut self,
        codec: ValueCodec,
        body: Vec<u8>,
        capture: Option<ValueCapture>,
    ) -> io::Result<ValueWriteOutcome> {
        let id = format!("value_{}", self.next_value_id);
        self.next_value_id = self.next_value_id.saturating_add(1);
        let value_ref = ValueRef::available(id, codec, body.len(), body.len());
        let record = ValueRecord {
            value_ref: value_ref.clone(),
            body,
            capture,
        };
        let mut encoded = Vec::new();
        encode_record(&mut encoded, &record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)?;
        Ok(ValueWriteOutcome { value_ref })
    }

    pub fn append_run_started(&mut self, record: &RunStartedRecord) -> io::Result<()> {
        let mut encoded = Vec::new();
        encode_run_started(&mut encoded, record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)
    }

    pub fn append_run_completed(&mut self, record: &RunCompletedRecord) -> io::Result<()> {
        let mut encoded = Vec::new();
        encode_run_completed(&mut encoded, record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)
    }

    pub fn flush(&mut self) -> io::Result<ValueArtifactRef> {
        self.sink.flush()
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    pub fn into_sink(self) -> S {
        self.sink
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ids::BoundaryId,
        value::{
            ByteValueArtifactSink, ValueArtifactRef, ValueCodec, ValueFileRecord, ValueWriter,
            read_bamlvalue_from_bytes,
        },
    };

    #[test]
    fn writer_appends_records_and_retains_bytes() {
        let sink = ByteValueArtifactSink::new();
        let mut writer = ValueWriter::new(sink, BoundaryId::from_bytes([2; 16])).unwrap();
        let outcome = writer
            .append_body(ValueCodec::BamlOutboundValue, vec![1, 2, 3])
            .unwrap();
        assert_eq!(outcome.value_ref.id, "value_1");
        assert_eq!(
            writer.flush().unwrap(),
            ValueArtifactRef::Bytes {
                len: writer.sink().bytes().len(),
                truncated: false,
                dropped_bytes: 0,
                dropped_chunks: 0,
            }
        );

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        assert_eq!(parsed.records.len(), 1);
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        assert_eq!(record.body, vec![1, 2, 3]);
    }
}
