use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Write},
};

use super::path::BoundaryHistoryPath;
use crate::{
    ids::BoundaryId,
    prof::{
        clock::TickConverter,
        encode::{build_header, encode_disk_event, encode_length_delimited_message},
        metadata, pb,
    },
    run::{ProfileEventEnvelope, ProfileEventKind},
    value::{
        BlobStore, CaptureLossRecord, FileValueArtifactSink, LogEventRecord, RunCompletedRecord,
        RunStartedRecord, ValueCapture, ValueCodec, ValueIdAllocator, ValueWriteOutcome,
        ValueWriter,
    },
};

pub struct BoundaryWriter {
    path: BoundaryHistoryPath,
    boundary_id: BoundaryId,
    started_at_epoch_ns: u128,
    rotation_policy: SegmentRotationPolicy,
    stack_writers: HashMap<u64, RotatingStackWriter>,
    value_writers: HashMap<u64, RotatingValueWriter>,
    value_id_allocator: ValueIdAllocator,
    run_started_written: bool,
    run_completed_written: bool,
}

const VALUE_INLINE_THRESHOLD_BYTES: usize = 64 * 1024;
const DEFAULT_STACK_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_STACK_SEGMENT_MAX_EVENTS: u64 = 50_000;
const DEFAULT_VALUE_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_VALUE_SEGMENT_MAX_RECORDS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SegmentRotationPolicy {
    stack_max_bytes: u64,
    stack_max_events: u64,
    value_max_bytes: u64,
    value_max_records: u64,
}

impl Default for SegmentRotationPolicy {
    fn default() -> Self {
        Self {
            stack_max_bytes: DEFAULT_STACK_SEGMENT_MAX_BYTES,
            stack_max_events: DEFAULT_STACK_SEGMENT_MAX_EVENTS,
            value_max_bytes: DEFAULT_VALUE_SEGMENT_MAX_BYTES,
            value_max_records: DEFAULT_VALUE_SEGMENT_MAX_RECORDS,
        }
    }
}

impl SegmentRotationPolicy {
    #[cfg(test)]
    pub(crate) fn for_tests(
        stack_max_bytes: u64,
        stack_max_events: u64,
        value_max_bytes: u64,
        value_max_records: u64,
    ) -> Self {
        Self {
            stack_max_bytes,
            stack_max_events,
            value_max_bytes,
            value_max_records,
        }
    }
}

impl BoundaryWriter {
    pub(crate) fn create_with_rotation_policy(
        path: BoundaryHistoryPath,
        boundary_id: BoundaryId,
        created_at_ms: u64,
        rotation_policy: SegmentRotationPolicy,
    ) -> io::Result<Self> {
        fs::create_dir_all(&path.boundary_dir)?;
        Ok(Self {
            path,
            boundary_id,
            started_at_epoch_ns: u128::from(created_at_ms).saturating_mul(1_000_000),
            rotation_policy,
            stack_writers: HashMap::new(),
            value_writers: HashMap::new(),
            value_id_allocator: ValueIdAllocator::standard(),
            run_started_written: false,
            run_completed_written: false,
        })
    }

    pub fn write_profile_event(
        &mut self,
        envelope: &ProfileEventEnvelope,
        disk_event: &pb::DiskEventV1,
    ) -> io::Result<()> {
        let thread_id = thread_id_for_event(&envelope.event.kind);
        if !self.stack_writers.contains_key(&thread_id) {
            let started_at_epoch_ns = self.started_at_epoch_ns;
            let writer = RotatingStackWriter::create(
                self.path.clone(),
                thread_id,
                envelope.process_euid.0,
                envelope.engine_id.0,
                started_at_epoch_ns,
                self.rotation_policy,
            )?;
            self.stack_writers.insert(thread_id, writer);
        }
        self.stack_writers
            .get_mut(&thread_id)
            .expect("stack writer inserted above")
            .write_event(disk_event)
    }

    pub fn write_run_started(
        &mut self,
        thread_id: u64,
        record: &RunStartedRecord,
    ) -> io::Result<()> {
        if self.run_started_written {
            return Ok(());
        }
        self.value_writer_for_thread(thread_id)?
            .append_run_started(record)?;
        self.run_started_written = true;
        Ok(())
    }

    pub fn write_run_completed(
        &mut self,
        thread_id: u64,
        record: &RunCompletedRecord,
    ) -> io::Result<()> {
        if self.run_completed_written {
            return Ok(());
        }
        self.value_writer_for_thread(thread_id)?
            .append_run_completed(record)?;
        self.run_completed_written = true;
        Ok(())
    }

    pub fn append_value_body(
        &mut self,
        capture: ValueCapture,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.value_writer_for_thread(capture.call.thread_id.0)?
            .append_value_body(capture, codec, body)
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
        for writer in self.stack_writers.values_mut() {
            writer.flush()?;
        }
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

struct RotatingStackWriter {
    path: BoundaryHistoryPath,
    thread_id: u64,
    process_id: [u8; 16],
    engine_id: u64,
    started_at_epoch_ns: u128,
    policy: SegmentRotationPolicy,
    segment: u64,
    events_written: u64,
    writer: StackSegmentWriter,
}

impl RotatingStackWriter {
    fn create(
        path: BoundaryHistoryPath,
        thread_id: u64,
        process_id: [u8; 16],
        engine_id: u64,
        started_at_epoch_ns: u128,
        policy: SegmentRotationPolicy,
    ) -> io::Result<Self> {
        let writer = StackSegmentWriter::create(
            path.stack_segment_path(thread_id, 0),
            process_id,
            engine_id,
            started_at_epoch_ns,
        )?;
        Ok(Self {
            path,
            thread_id,
            process_id,
            engine_id,
            started_at_epoch_ns,
            policy,
            segment: 0,
            events_written: 0,
            writer,
        })
    }

    fn write_event(&mut self, disk_event: &pb::DiskEventV1) -> io::Result<()> {
        self.rotate_if_needed()?;
        self.writer.write_event(disk_event);
        self.events_written = self.events_written.saturating_add(1);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn rotate_if_needed(&mut self) -> io::Result<()> {
        if self.events_written == 0 {
            return Ok(());
        }
        if self.events_written < self.policy.stack_max_events
            && self.writer.bytes_written() < self.policy.stack_max_bytes
        {
            return Ok(());
        }
        self.writer.flush()?;
        self.segment = self.segment.saturating_add(1);
        self.writer = StackSegmentWriter::create(
            self.path.stack_segment_path(self.thread_id, self.segment),
            self.process_id,
            self.engine_id,
            self.started_at_epoch_ns,
        )?;
        self.events_written = 0;
        Ok(())
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

    fn append_value_body(
        &mut self,
        capture: ValueCapture,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.rotate_if_needed()?;
        let outcome = self
            .writer
            .append_body_with_capture(codec, body, Some(capture))?;
        self.note_record_written();
        Ok(outcome)
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
        if self.records_written == 0 {
            return Ok(());
        }
        if self.records_written < self.policy.value_max_records
            && self.writer.sink().bytes_written() < self.policy.value_max_bytes
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

struct StackSegmentWriter {
    file: File,
    scratch: Vec<u8>,
    bytes_written: u64,
}

impl StackSegmentWriter {
    fn create(
        path: std::path::PathBuf,
        process_id: [u8; 16],
        engine_id: u64,
        started_at_epoch_ns: u128,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = Self {
            file: File::create(path)?,
            scratch: Vec::new(),
            bytes_written: 0,
        };
        let meta = metadata::get_engine_metadata(engine_id);
        let header = build_header(
            process_id,
            engine_id,
            started_at_epoch_ns,
            meta.as_ref(),
            &TickConverter::identity(),
        );
        encode_length_delimited_message(&mut writer.scratch, &header).map_err(io::Error::other)?;
        let header_len = writer.scratch.len();
        writer.file.write_all(&writer.scratch)?;
        writer.bytes_written = writer
            .bytes_written
            .saturating_add(u64::try_from(header_len).unwrap_or(u64::MAX));
        writer.scratch.clear();
        Ok(writer)
    }

    fn write_event(&mut self, disk_event: &pb::DiskEventV1) {
        let start_len = self.scratch.len();
        encode_disk_event(&mut self.scratch, disk_event);
        let encoded_len = self.scratch.len().saturating_sub(start_len);
        self.bytes_written = self
            .bytes_written
            .saturating_add(u64::try_from(encoded_len).unwrap_or(u64::MAX));
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.scratch.is_empty() {
            self.file.write_all(&self.scratch)?;
            self.scratch.clear();
        }
        self.file.flush()
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}

fn thread_id_for_event(kind: &ProfileEventKind) -> u64 {
    match kind {
        ProfileEventKind::StartThread { thread_id, .. }
        | ProfileEventKind::EndThread { thread_id, .. }
        | ProfileEventKind::CallFunction { thread_id, .. }
        | ProfileEventKind::EndFunction { thread_id, .. } => thread_id.0,
    }
}
