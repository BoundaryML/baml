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
        RunStartedRecord, ValueCapture, ValueCodec, ValueWriteOutcome, ValueWriter,
    },
};

pub struct BoundaryWriter {
    path: BoundaryHistoryPath,
    boundary_id: BoundaryId,
    started_at_epoch_ns: u128,
    stack_writers: HashMap<u64, StackSegmentWriter>,
    value_writers: HashMap<u64, ValueWriter<FileValueArtifactSink>>,
    run_started_written: bool,
    run_completed_written: bool,
}

const VALUE_INLINE_THRESHOLD_BYTES: usize = 64 * 1024;

impl BoundaryWriter {
    pub fn create(
        path: BoundaryHistoryPath,
        boundary_id: BoundaryId,
        created_at_ms: u64,
    ) -> io::Result<Self> {
        fs::create_dir_all(&path.boundary_dir)?;
        Ok(Self {
            path,
            boundary_id,
            started_at_epoch_ns: u128::from(created_at_ms).saturating_mul(1_000_000),
            stack_writers: HashMap::new(),
            value_writers: HashMap::new(),
            run_started_written: false,
            run_completed_written: false,
        })
    }

    #[must_use]
    pub fn boundary_dir(&self) -> &std::path::Path {
        &self.path.boundary_dir
    }

    pub fn write_profile_event(
        &mut self,
        envelope: &ProfileEventEnvelope,
        disk_event: &pb::DiskEventV1,
    ) -> io::Result<()> {
        let thread_id = thread_id_for_event(&envelope.event.kind);
        if !self.stack_writers.contains_key(&thread_id) {
            let started_at_epoch_ns = self.started_at_epoch_ns;
            let path = self.path.stack_segment_path(thread_id, 0);
            let writer = StackSegmentWriter::create(
                path,
                envelope.process_euid.0,
                envelope.engine_id.0,
                started_at_epoch_ns,
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
            .append_body_with_capture(codec, body, Some(capture))
    }

    pub fn append_log_body(
        &mut self,
        event: LogEventRecord,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        self.value_writer_for_thread(event.call.thread_id.0)?
            .append_log_body(codec, body, event)
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

    fn value_writer_for_thread(
        &mut self,
        thread_id: u64,
    ) -> io::Result<&mut ValueWriter<FileValueArtifactSink>> {
        if !self.value_writers.contains_key(&thread_id) {
            let sink = FileValueArtifactSink::create(self.path.value_segment_path(thread_id, 0))?;
            let writer = ValueWriter::with_blob_store(
                sink,
                self.boundary_id,
                BlobStore::for_boundary_dir(&self.path.boundary_dir),
                VALUE_INLINE_THRESHOLD_BYTES,
            )?;
            self.value_writers.insert(thread_id, writer);
        }
        Ok(self
            .value_writers
            .get_mut(&thread_id)
            .expect("value writer inserted above"))
    }
}

struct StackSegmentWriter {
    file: File,
    scratch: Vec<u8>,
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
        writer.file.write_all(&writer.scratch)?;
        writer.scratch.clear();
        Ok(writer)
    }

    fn write_event(&mut self, disk_event: &pb::DiskEventV1) -> io::Result<()> {
        encode_disk_event(&mut self.scratch, disk_event);
        self.flush()
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.scratch.is_empty() {
            self.file.write_all(&self.scratch)?;
            self.scratch.clear();
        }
        self.file.flush()
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
