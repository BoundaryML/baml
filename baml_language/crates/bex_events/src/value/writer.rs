//! Minimal `.bamlvalue` append writer.

use std::{
    io,
    sync::{Arc, Mutex},
};

use crate::{
    ids::BoundaryId,
    value::{
        BlobRef, BlobStore, CaptureLossRecord, DagRef, LogEventRecord, LogRecord,
        RunCompletedRecord, RunStartedRecord, ValueArtifactRef, ValueArtifactSink, ValueCapture,
        ValueCodec, ValueRecord, ValueRef,
        encode::{
            encode_capture_loss, encode_header, encode_log_event, encode_record,
            encode_run_completed, encode_run_started,
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
        self.append_body_with_capture_and_dag(codec, body, capture, None)
    }

    /// Append a captured value body that has additionally been re-encoded
    /// canonically and persisted to the project store (§7.4 dual-write):
    /// `dag_ref` addresses the canonical DAG root in that store.
    pub fn append_body_with_capture_and_dag(
        &mut self,
        codec: ValueCodec,
        body: Vec<u8>,
        capture: Option<ValueCapture>,
        dag_ref: Option<DagRef>,
    ) -> io::Result<ValueWriteOutcome> {
        self.append_body_with_capture_dag_and_promotion(codec, body, capture, dag_ref, None)
    }

    /// Like [`Self::append_body_with_capture_and_dag`], additionally
    /// marking the capture as trigger-promoted (§7.2 `role: promoted`):
    /// `promoted_by` names the trigger that made this speculatively staged
    /// capture durable.
    pub fn append_body_with_capture_dag_and_promotion(
        &mut self,
        codec: ValueCodec,
        body: Vec<u8>,
        capture: Option<ValueCapture>,
        dag_ref: Option<DagRef>,
        promoted_by: Option<String>,
    ) -> io::Result<ValueWriteOutcome> {
        let id = self.value_id_allocator.allocate();
        let original_size = body.len();
        let (body, blob_ref) = self.store_body(body)?;
        let retained_size = blob_ref.as_ref().map_or(body.len(), |blob| blob.size_bytes);
        let value_ref = ValueRef::available(id, codec, original_size, retained_size);
        let record = ValueRecord {
            value_ref: value_ref.clone(),
            body,
            blob_ref,
            capture,
            dag_ref,
            promoted_by,
        };
        let mut encoded = Vec::new();
        encode_record(&mut encoded, &record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.sink.write_chunk(&encoded)?;
        Ok(ValueWriteOutcome { value_ref })
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

    /// Consume the writer and return its sink (tests / byte-sink callers).
    pub fn into_sink(self) -> S {
        self.sink
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

    /// Durability point for the segment (fsync where the sink has one).
    /// Call at boundary completion, before the boundary's roots are
    /// pinned — a pinned root must never outlive its capture evidence.
    pub fn sync_data(&mut self) -> io::Result<()> {
        self.sink.sync_data()
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
        ids::BoundaryId,
        store::{Store, canon},
        value::{
            BlobStore, ByteValueArtifactSink, DagRef, ValueArtifactRef, ValueCodec,
            ValueFileRecord, ValueIdAllocator, ValueWriter, read_bamlvalue_from_bytes,
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
            .append_body(ValueCodec::BamlOutboundValue, vec![1])
            .unwrap();
        let second_outcome = second
            .append_body(ValueCodec::BamlOutboundValue, vec![2])
            .unwrap();

        assert_eq!(first_outcome.value_ref.id, "value_1");
        assert_eq!(second_outcome.value_ref.id, "value_2");
    }

    #[test]
    fn dag_ref_round_trips_and_store_serves_the_root() {
        let dir = std::env::temp_dir().join(format!(
            "bamlvalue-writer-dag-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let value = canon::CanonValue::Map(vec![
            ("name".to_string(), canon::CanonValue::String("ada".into())),
            ("count".to_string(), canon::CanonValue::Int(42)),
        ]);
        let encoded = canon::encode(&value);
        let mut store = Store::open(&dir, [5; 16]).unwrap();
        store.put_encoded(&encoded, 1).unwrap();

        let dag_ref = DagRef {
            root_cid: encoded.root_cid,
            node_codec_version: canon::NODE_CODEC_VERSION,
            logical_len: encoded.logical_len,
        };
        let sink = ByteValueArtifactSink::new();
        let mut writer = ValueWriter::new(sink, BoundaryId::from_bytes([2; 16])).unwrap();
        writer
            .append_body_with_capture_and_dag(
                ValueCodec::BamlOutboundValue,
                vec![1, 2, 3],
                None,
                Some(dag_ref),
            )
            .unwrap();

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
        };
        assert_eq!(record.body, vec![1, 2, 3], "legacy body stays (dual-write)");
        assert_eq!(record.dag_ref, Some(dag_ref));
        let root = store
            .get(&dag_ref.root_cid)
            .unwrap()
            .expect("store serves the DAG root");
        assert!(!root.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
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
            .append_body(ValueCodec::BamlOutboundValue, b"abcdef".to_vec())
            .unwrap();

        assert_eq!(outcome.value_ref.id, "value_1");
        assert_eq!(outcome.value_ref.original_size_bytes, Some(6));
        assert_eq!(outcome.value_ref.retained_size_bytes, Some(6));

        let parsed = read_bamlvalue_from_bytes(writer.sink().bytes()).unwrap();
        let ValueFileRecord::CapturedValue(record) = &parsed.records[0] else {
            panic!("expected value record");
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
