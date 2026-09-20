//! Engine integration for the chunk processor and recording publisher.
use std::sync::Arc;

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
    receive: Box<dyn FnMut(SealedFile) + Send>,
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
            receive: Box::new(receive),
        }
    }

    pub fn id(&self) -> RecordingId {
        self.id
    }

    pub(crate) fn start(
        self,
        source_snapshot: Option<[u8; 32]>,
    ) -> Result<Arc<btel_processor::TelemetryRuntime>, EngineError> {
        let publisher = RecordingPublisher::new(self.id, self.config, self.receive)
            .map_err(|error| EngineError::Other(error.to_string()))?
            .with_source_snapshot(source_snapshot);
        btel_processor::TelemetryRuntime::with_publisher(publisher)
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))
    }
}

impl BexEngine {
    /// Configure encoded-file delivery before initialization executes. Existing
    /// constructors use the same full pipeline with zero destinations.
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
            recording,
        )
    }

    pub fn telemetry_recording_id(&self) -> RecordingId {
        self.telemetry_recording_id
    }

    /// None while processing. After shutdown, Some(Ok(())) means all published
    /// chunks were consumed and pending bytes delivered to the callback. It does
    /// not assert durable storage, complete captures, or final clock validity.
    pub fn telemetry_result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        self.telemetry_runtime.result()
    }
}
