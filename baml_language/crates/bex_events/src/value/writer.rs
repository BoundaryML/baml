//! Minimal `.bamlvalue` append writer.

use std::{
    io,
    sync::{Arc, Mutex},
};

use crate::{
    ids::BoundaryId,
    value::{
        BlobRef, BlobStore, CaptureLossRecord, LogEventRecord, LogRecord, RunCompletedRecord,
        RunStartedRecord, ValueArtifactRef, ValueArtifactSink, ValueCodec, ValueRef,
        encode::{
            encode_capture_loss, encode_header, encode_log_event, encode_run_completed,
            encode_run_started,
        },
    },
};

#[derive(Debug)]
pub struct ValueWriter<S> {
    sink: S,
    value_id_allocator: ValueIdAllocator,
    blob_store: Option<BlobStore>,
    inline_threshold_bytes: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ValueIdAllocator {
    prefix: Arc<str>,
    next_value_id: Arc<Mutex<u64>>,
}

impl ValueIdAllocator {
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self::with_next_value_id(prefix, 1)
    }

    #[must_use]
    pub(crate) fn with_next_value_id(prefix: impl Into<String>, next_value_id: u64) -> Self {
        Self {
            prefix: Arc::from(prefix.into()),
            next_value_id: Arc::new(Mutex::new(next_value_id)),
        }
    }

    #[must_use]
    pub(crate) fn standard() -> Self {
        Self::new("value")
    }

    #[must_use]
    pub fn live_fallback() -> Self {
        Self::new("live_value")
    }

    fn allocate(&self) -> String {
        let mut next_value_id = self
            .next_value_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = format!("{}_{}", self.prefix, *next_value_id);
        *next_value_id = next_value_id.saturating_add(1);
        id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueWriteOutcome {
    pub value_ref: ValueRef,
}

impl<S: ValueArtifactSink> ValueWriter<S> {
    pub fn new(sink: S, boundary_id: BoundaryId) -> io::Result<Self> {
        Self::new_with_id_allocator(sink, boundary_id, ValueIdAllocator::standard())
    }

    pub fn new_with_id_allocator(
        mut sink: S,
        boundary_id: BoundaryId,
        value_id_allocator: ValueIdAllocator,
    ) -> io::Result<Self> {
        let mut header = Vec::new();
        encode_header(&mut header, boundary_id)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        sink.write_chunk(&header)?;
        Ok(Self {
            sink,
            value_id_allocator,
            blob_store: None,
            inline_threshold_bytes: None,
        })
    }

    pub fn with_blob_store(
        sink: S,
        boundary_id: BoundaryId,
        blob_store: BlobStore,
        inline_threshold_bytes: usize,
    ) -> io::Result<Self> {
        Self::with_blob_store_and_id_allocator(
            sink,
            boundary_id,
            blob_store,
            inline_threshold_bytes,
            ValueIdAllocator::standard(),
        )
    }

    pub(crate) fn with_blob_store_and_id_allocator(
        mut sink: S,
        boundary_id: BoundaryId,
        blob_store: BlobStore,
        inline_threshold_bytes: usize,
        value_id_allocator: ValueIdAllocator,
    ) -> io::Result<Self> {
        let mut header = Vec::new();
        encode_header(&mut header, boundary_id)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        sink.write_chunk(&header)?;
        Ok(Self {
            sink,
            value_id_allocator,
            blob_store: Some(blob_store),
            inline_threshold_bytes: Some(inline_threshold_bytes),
        })
    }

    pub fn append_log_body(
        &mut self,
        codec: ValueCodec,
        body: Vec<u8>,
        event: LogEventRecord,
    ) -> io::Result<ValueWriteOutcome> {
        let id = self.value_id_allocator.allocate();
        let original_size = body.len();
        let (body, blob_ref) = self.store_body(body)?;
        let retained_size = blob_ref.as_ref().map_or(body.len(), |blob| blob.size_bytes);
        let value_ref = ValueRef::available(id, codec, original_size, retained_size);
        let record = LogRecord {
            value_ref: value_ref.clone(),
            body,
            blob_ref,
            event,
        };
        let mut encoded = Vec::new();
        encode_log_event(&mut encoded, &record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)?;
        Ok(ValueWriteOutcome { value_ref })
    }

    pub fn append_capture_loss(&mut self, record: &CaptureLossRecord) -> io::Result<()> {
        let mut encoded = Vec::new();
        encode_capture_loss(&mut encoded, record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)
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

    fn store_body(&self, body: Vec<u8>) -> io::Result<(Vec<u8>, Option<BlobRef>)> {
        let Some(blob_store) = &self.blob_store else {
            return Ok((body, None));
        };
        let Some(threshold) = self.inline_threshold_bytes else {
            return Ok((body, None));
        };
        if body.len() <= threshold {
            return Ok((body, None));
        }
        let blob_ref = blob_store.write_blob(&body)?;
        Ok((Vec::new(), Some(blob_ref)))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::TraceCallKey,
        value::{
            BlobStore, ByteValueArtifactSink, LogEventRecord, ValueArtifactRef, ValueCodec,
            ValueFileRecord, ValueIdAllocator, ValueWriter, read_bamlvalue_from_bytes,
        },
    };

    fn log_event() -> LogEventRecord {
        LogEventRecord {
            call: TraceCallKey {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(3),
                call_id: BexCallId(4),
            },
            level: Some("info".to_string()),
            source: None,
            timestamp_ms: 5,
            message_preview: Some("message".to_string()),
        }
    }

    #[test]
    fn writer_appends_records_and_retains_bytes() {
        let sink = ByteValueArtifactSink::new();
        let mut writer = ValueWriter::new(sink, BoundaryId::from_bytes([2; 16])).unwrap();
        let outcome = writer
            .append_log_body(ValueCodec::BamlOutboundValue, vec![1, 2, 3], log_event())
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
        let ValueFileRecord::LogEvent(record) = &parsed.records[0] else {
            panic!("expected log record");
        };
        assert_eq!(record.body, vec![1, 2, 3]);
        assert!(record.blob_ref.is_none());
    }

    #[test]
    fn shared_allocator_makes_value_ids_unique_across_writers() {
        let boundary_id = BoundaryId::from_bytes([2; 16]);
        let allocator = ValueIdAllocator::standard();
        let mut first = ValueWriter::new_with_id_allocator(
            ByteValueArtifactSink::new(),
            boundary_id,
            allocator.clone(),
        )
        .unwrap();
        let mut second = ValueWriter::new_with_id_allocator(
            ByteValueArtifactSink::new(),
            boundary_id,
            allocator,
        )
        .unwrap();

        let first_outcome = first
            .append_log_body(ValueCodec::BamlOutboundValue, vec![1], log_event())
            .unwrap();
        let second_outcome = second
            .append_log_body(ValueCodec::BamlOutboundValue, vec![2], log_event())
            .unwrap();

        assert_eq!(first_outcome.value_ref.id, "value_1");
        assert_eq!(second_outcome.value_ref.id, "value_2");
    }

    #[test]
    fn blob_writer_externalizes_bodies_above_threshold() {
        let root = std::env::temp_dir().join(format!(
            "bamlvalue-writer-blob-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let blob_store = BlobStore::new(&root);
        let sink = ByteValueArtifactSink::new();
        let mut writer =
            ValueWriter::with_blob_store(sink, BoundaryId::from_bytes([2; 16]), blob_store, 3)
                .unwrap();
        let outcome = writer
            .append_log_body(
                ValueCodec::BamlOutboundValue,
                b"abcdef".to_vec(),
                log_event(),
            )
            .unwrap();

        assert_eq!(outcome.value_ref.id, "value_1");
        assert_eq!(outcome.value_ref.original_size_bytes, Some(6));
        assert_eq!(outcome.value_ref.retained_size_bytes, Some(6));

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::LogEvent(record) = &parsed.records[0] else {
            panic!("expected log record");
        };
        assert!(record.body.is_empty());
        let blob_ref = record.blob_ref.as_ref().expect("blob ref recorded");
        assert_eq!(
            std::fs::read(BlobStore::new(&root).path_for(blob_ref).unwrap()).unwrap(),
            b"abcdef"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
