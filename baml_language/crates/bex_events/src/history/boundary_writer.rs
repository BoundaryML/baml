//! Mixed legacy-history writer retained only for non-profile lifecycle and logs.

use std::{collections::HashMap, fs, io};

use super::path::BoundaryHistoryPath;
use crate::{
    ids::BoundaryId,
    value::{
        BlobStore, CaptureLossRecord, FileValueArtifactSink, LogEventRecord, RunCompletedRecord,
        RunStartedRecord, ValueCodec, ValueIdAllocator, ValueWriteOutcome, ValueWriter,
    },
};

pub struct BoundaryWriter {
    path: BoundaryHistoryPath,
    boundary_id: BoundaryId,
    rotation_policy: SegmentRotationPolicy,
    value_writers: HashMap<u64, RotatingValueWriter>,
    value_id_allocator: ValueIdAllocator,
    run_started_written: bool,
    run_completed_written: bool,
}

const VALUE_INLINE_THRESHOLD_BYTES: usize = 64 * 1024;
const DEFAULT_VALUE_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_VALUE_SEGMENT_MAX_RECORDS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SegmentRotationPolicy {
    value_max_bytes: u64,
    value_max_records: u64,
}

impl Default for SegmentRotationPolicy {
    fn default() -> Self {
        Self {
            value_max_bytes: DEFAULT_VALUE_SEGMENT_MAX_BYTES,
            value_max_records: DEFAULT_VALUE_SEGMENT_MAX_RECORDS,
        }
    }
}

impl BoundaryWriter {
    pub(crate) fn create_with_rotation_policy(
        path: BoundaryHistoryPath,
        boundary_id: BoundaryId,
        _created_at_ms: u64,
        rotation_policy: SegmentRotationPolicy,
    ) -> io::Result<Self> {
        fs::create_dir_all(&path.boundary_dir)?;
        Ok(Self {
            path,
            boundary_id,
            rotation_policy,
            value_writers: HashMap::new(),
            value_id_allocator: ValueIdAllocator::standard(),
            run_started_written: false,
            run_completed_written: false,
        })
    }

    pub fn write_run_started(&mut self, record: &RunStartedRecord) -> io::Result<()> {
        if self.run_started_written {
            return Ok(());
        }
        self.value_writer_for_thread(0)?
            .append_run_started(record)?;
        self.run_started_written = true;
        Ok(())
    }

    pub fn write_run_completed(&mut self, record: &RunCompletedRecord) -> io::Result<()> {
        if self.run_completed_written {
            return Ok(());
        }
        self.value_writer_for_thread(0)?
            .append_run_completed(record)?;
        self.run_completed_written = true;
        Ok(())
    }

    pub fn append_log_body(
        &mut self,
        event: LogEventRecord,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.value_writer_for_thread(event.call.thread_id.0)?
            .append_log_body(event, codec, body)
    }

    pub fn append_capture_loss(&mut self, record: &CaptureLossRecord) -> io::Result<()> {
        let thread_id = record.call.map_or(0, |call| call.thread_id.0);
        self.value_writer_for_thread(thread_id)?
            .append_capture_loss(record)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        for writer in self.value_writers.values_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    fn value_writer_for_thread(&mut self, thread_id: u64) -> io::Result<&mut RotatingValueWriter> {
        if !self.value_writers.contains_key(&thread_id) {
            let writer = RotatingValueWriter::create(
                self.path.clone(),
                self.boundary_id,
                thread_id,
                self.rotation_policy,
                self.value_id_allocator.clone(),
            )?;
            self.value_writers.insert(thread_id, writer);
        }
        Ok(self
            .value_writers
            .get_mut(&thread_id)
            .expect("value writer inserted above"))
    }
}

struct RotatingValueWriter {
    path: BoundaryHistoryPath,
    boundary_id: BoundaryId,
    thread_id: u64,
    blob_store: BlobStore,
    value_id_allocator: ValueIdAllocator,
    policy: SegmentRotationPolicy,
    segment: u64,
    records_written: u64,
    writer: ValueWriter<FileValueArtifactSink>,
}

impl RotatingValueWriter {
    fn create(
        path: BoundaryHistoryPath,
        boundary_id: BoundaryId,
        thread_id: u64,
        policy: SegmentRotationPolicy,
        value_id_allocator: ValueIdAllocator,
    ) -> io::Result<Self> {
        let blob_store = BlobStore::for_boundary_dir(&path.boundary_dir);
        let writer = Self::open_writer(
            &path,
            boundary_id,
            thread_id,
            0,
            blob_store.clone(),
            value_id_allocator.clone(),
        )?;
        Ok(Self {
            path,
            boundary_id,
            thread_id,
            blob_store,
            value_id_allocator,
            policy,
            segment: 0,
            records_written: 0,
            writer,
        })
    }

    fn append_log_body(
        &mut self,
        event: LogEventRecord,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.rotate_if_needed()?;
        let outcome = self.writer.append_log_body(codec, body, event)?;
        self.note_record_written();
        Ok(outcome)
    }

    fn append_capture_loss(&mut self, record: &CaptureLossRecord) -> io::Result<()> {
        self.rotate_if_needed()?;
        self.writer.append_capture_loss(record)?;
        self.note_record_written();
        Ok(())
    }

    fn append_run_started(&mut self, record: &RunStartedRecord) -> io::Result<()> {
        self.rotate_if_needed()?;
        self.writer.append_run_started(record)?;
        self.note_record_written();
        Ok(())
    }

    fn append_run_completed(&mut self, record: &RunCompletedRecord) -> io::Result<()> {
        self.rotate_if_needed()?;
        self.writer.append_run_completed(record)?;
        self.note_record_written();
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush().map(|_| ())
    }

    fn rotate_if_needed(&mut self) -> io::Result<()> {
        if self.records_written == 0
            || (self.records_written < self.policy.value_max_records
                && self.writer.sink().bytes_written() < self.policy.value_max_bytes)
        {
            return Ok(());
        }
        self.writer.flush()?;
        self.segment = self.segment.saturating_add(1);
        self.writer = Self::open_writer(
            &self.path,
            self.boundary_id,
            self.thread_id,
            self.segment,
            self.blob_store.clone(),
            self.value_id_allocator.clone(),
        )?;
        self.records_written = 0;
        Ok(())
    }

    fn note_record_written(&mut self) {
        self.records_written = self.records_written.saturating_add(1);
    }

    fn open_writer(
        path: &BoundaryHistoryPath,
        boundary_id: BoundaryId,
        thread_id: u64,
        segment: u64,
        blob_store: BlobStore,
        value_id_allocator: ValueIdAllocator,
    ) -> io::Result<ValueWriter<FileValueArtifactSink>> {
        let sink = FileValueArtifactSink::create(path.value_segment_path(thread_id, segment))?;
        ValueWriter::with_blob_store_and_id_allocator(
            sink,
            boundary_id,
            blob_store,
            VALUE_INLINE_THRESHOLD_BYTES,
            value_id_allocator,
        )
    }
}
