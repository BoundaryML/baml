use std::io;

use super::path::BoundaryHistoryPath;
use crate::{
    ids::BoundaryId,
    value::{FileValueArtifactSink, RunCompletedRecord, RunStartedRecord, ValueWriter},
};

pub(super) struct LifecycleWriter {
    path: BoundaryHistoryPath,
    boundary_id: BoundaryId,
}

impl LifecycleWriter {
    pub(super) fn new(path: BoundaryHistoryPath, boundary_id: BoundaryId) -> Self {
        Self { path, boundary_id }
    }

    pub(super) fn write_run_started(&self, record: &RunStartedRecord) -> io::Result<()> {
        self.write(0, |writer| writer.append_run_started(record))
    }

    pub(super) fn write_run_completed(&self, record: &RunCompletedRecord) -> io::Result<()> {
        self.write(1, |writer| writer.append_run_completed(record))
    }

    fn write(
        &self,
        segment: u64,
        append: impl FnOnce(&mut ValueWriter<FileValueArtifactSink>) -> io::Result<()>,
    ) -> io::Result<()> {
        let sink = FileValueArtifactSink::create(self.path.value_segment_path(0, segment))?;
        let mut writer = ValueWriter::new(sink, self.boundary_id)?;
        append(&mut writer)?;
        writer.flush().map(|_| ())
    }
}
