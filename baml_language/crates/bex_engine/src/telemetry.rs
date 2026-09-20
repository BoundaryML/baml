//! Engine integration for the chunk processor and recording publisher.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use btel_publisher::{RecordingConfig, RecordingId, RecordingPublisher, SealedFile};

use crate::{BexEngine, EngineError, RuntimeCompiler};

/// One recording per engine. All destinations receive the same encoded files.
/// The callback runs on the processor thread after input chunks are recycled.
/// It must return promptly and must not retain or call back into this engine.
/// Retained `SealedFile`s count against the publisher's resident-byte budget.
/// No sinks means encoding still runs and completed files are immediately freed.
/// Current output retains deferred-capture and unavailable-function-metadata
/// markers; clock observations do not yet claim final recording validity.
pub struct TelemetryRecording {
    id: RecordingId,
    config: RecordingConfig,
    destination: Destination,
}
enum Destination {
    Callback(Box<dyn FnMut(SealedFile) + Send>),
    LocalFiles(PathBuf),
}

impl Default for TelemetryRecording {
    fn default() -> Self {
        Self::new(RecordingConfig::default(), drop)
    }
}
impl TelemetryRecording {
    pub fn new(config: RecordingConfig, receive: impl FnMut(SealedFile) + Send + 'static) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::Callback(Box::new(receive)),
        }
    }

    /// Write to a new `<directory>/<recording-id>` directory. No I/O or worker
    /// starts until engine construction; `BAML_TELEMETRY=off` ignores this output.
    /// Existing recording directories are never reused. Shutdown awaits writes.
    pub fn local_files(directory: impl Into<PathBuf>, config: RecordingConfig) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::LocalFiles(directory.into()),
        }
    }

    pub fn id(&self) -> RecordingId {
        self.id
    }

    pub(crate) fn start(
        self,
        source_snapshot: Option<[u8; 32]>,
    ) -> Result<
        (
            Arc<btel_processor::TelemetryRuntime>,
            Option<Arc<btel_file::FileSink>>,
        ),
        EngineError,
    > {
        let transport = btel_settings::transport::ChunkConfig::default();
        self.config
            .validate_transport(&transport)
            .map_err(|error| EngineError::Other(error.to_owned()))?;
        let (receive, sink): (Box<dyn FnMut(SealedFile) + Send>, _) = match self.destination {
            Destination::Callback(receive) => (receive, None),
            Destination::LocalFiles(root) => {
                let sink = Arc::new(
                    btel_file::FileSink::create(
                        &root,
                        self.id,
                        btel_file::FileSinkConfig::default(),
                    )
                    .map_err(|error| {
                        EngineError::Other(format!("telemetry file startup: {error}"))
                    })?,
                );
                let sender = sink.sender();
                (
                    Box::new(move |file| {
                        // The existing processor guard turns delivery failure into
                        // a terminal, observable transport error. Never drop silently.
                        if let Err(error) = sender.send(file) {
                            panic!("{error}");
                        }
                    }),
                    Some(sink),
                )
            }
        };
        let publisher = RecordingPublisher::new(self.id, self.config, receive)
            .map_err(|error| EngineError::Other(error.to_string()))?
            .with_source_snapshot(source_snapshot);
        let runtime =
            btel_processor::TelemetryRuntime::with_config_and_publisher(transport, publisher)
                .map_err(|error| {
                    EngineError::Other(format!("telemetry processor startup: {error}"))
                })?;
        Ok((runtime, sink))
    }
}

impl BexEngine {
    /// Configure encoded-file delivery before initialization executes. Existing
    /// constructors use the same full pipeline with zero destinations.
    /// `BAML_TELEMETRY=off` disables it even when a recording is supplied.
    pub fn new_with_telemetry_recording(
        program: bex_vm_types::Program,
        sys_ops: Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        runtime_compiler: Option<Arc<dyn RuntimeCompiler>>,
        clock_mode: btel_clock::ClockMode,
        recording: TelemetryRecording,
    ) -> Result<Self, EngineError> {
        Self::build(
            program,
            sys_ops,
            argv,
            runtime_compiler,
            clock_mode,
            Some(recording),
        )
    }

    /// No recording is created when `BAML_TELEMETRY=off`.
    pub fn telemetry_recording_id(&self) -> Option<RecordingId> {
        self.telemetry
            .as_ref()
            .map(|telemetry| telemetry.recording_id)
    }

    /// Local output directory, when configured and telemetry is enabled.
    pub fn telemetry_recording_directory(&self) -> Option<&Path> {
        self.telemetry
            .as_ref()?
            .file_sink
            .as_ref()
            .map(|sink| sink.directory())
    }

    /// None when disabled or while processing. After shutdown, Some(Ok(())) means all published
    /// chunks were consumed and pending bytes delivered to the callback. For
    /// local files, it also waits for completed file writes. It does not assert
    /// complete captures, final clock validity, or a `RecordingEnd` marker.
    pub fn telemetry_result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        self.telemetry
            .as_ref()
            .and_then(super::telemetry_state::EngineTelemetry::result)
    }
}
